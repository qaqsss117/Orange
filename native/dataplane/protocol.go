package dataplane

import (
	"bytes"
	"context"
	"encoding/binary"
	"encoding/json"
	"errors"
	"io"
	"sync"
	"time"
)

const (
	ProtocolVersion     = 1
	MaxFrameBytes       = 4 << 10
	MaxConcurrentProbes = 8
	MinProbeTimeout     = 100 * time.Millisecond
	MaxProbeTimeout     = 60 * time.Second
)

type responseFrame struct {
	Version            int     `json:"version"`
	Kind               string  `json:"kind"`
	ID                 uint64  `json:"id,omitempty"`
	Result             string  `json:"result,omitempty"`
	ErrorCode          string  `json:"errorCode,omitempty"`
	SelectedNodeID     *string `json:"selectedNodeId,omitempty"`
	DelayMS            *uint32 `json:"delayMs,omitempty"`
	UploadBytesTotal   *uint64 `json:"uploadBytesTotal,omitempty"`
	DownloadBytesTotal *uint64 `json:"downloadBytesTotal,omitempty"`
}

type requestFrame struct {
	ID              uint64
	Command         string
	SelectorID      string
	NodeID          string
	TimeoutMS       uint64
	TargetRequestID uint64
}

type Server struct {
	controller Controller
	reader     io.Reader
	writer     io.Writer
	writeMu    sync.Mutex
	activeMu   sync.Mutex
	active     map[uint64]context.CancelFunc
	lastID     uint64
	probeSlots chan struct{}
	probes     sync.WaitGroup
}

func NewServer(controller Controller, reader io.Reader, writer io.Writer) *Server {
	return &Server{
		controller: controller,
		reader:     reader,
		writer:     writer,
		active:     make(map[uint64]context.CancelFunc),
		probeSlots: make(chan struct{}, MaxConcurrentProbes),
	}
}

func (s *Server) Run(ctx context.Context) error {
	if err := s.write(responseFrame{Version: ProtocolVersion, Kind: "ready"}); err != nil {
		return err
	}
	for {
		payload, err := readFrame(s.reader)
		if errors.Is(err, io.EOF) {
			s.cancelAll()
			s.probes.Wait()
			return nil
		}
		if err != nil {
			s.cancelAll()
			s.probes.Wait()
			return err
		}
		request, err := decodeRequest(payload)
		clear(payload)
		if err != nil || request.ID <= s.lastID {
			s.cancelAll()
			s.probes.Wait()
			return errInvalidRequest
		}
		s.lastID = request.ID
		if request.Command == "probe_delay" {
			s.startProbe(ctx, request)
			continue
		}
		response := s.handle(ctx, request)
		if err := s.write(response); err != nil {
			s.cancelAll()
			s.probes.Wait()
			return err
		}
	}
}

func (s *Server) handle(_ context.Context, request requestFrame) responseFrame {
	switch request.Command {
	case "select_node":
		if err := s.controller.SelectNode(request.SelectorID, request.NodeID); err != nil {
			return errorResponse(request.ID, err)
		}
		return okResponse(request.ID)
	case "read_selected_node":
		selected, err := s.controller.ReadSelectedNode(request.SelectorID)
		if err != nil {
			return errorResponse(request.ID, err)
		}
		response := okResponse(request.ID)
		response.SelectedNodeID = &selected
		return response
	case "traffic":
		upload, download, err := s.controller.TrafficTotals()
		if err != nil {
			return errorResponse(request.ID, err)
		}
		response := okResponse(request.ID)
		response.UploadBytesTotal = &upload
		response.DownloadBytesTotal = &download
		return response
	case "cancel_probe":
		s.activeMu.Lock()
		cancel := s.active[request.TargetRequestID]
		s.activeMu.Unlock()
		if cancel == nil {
			return errorResponse(request.ID, errInvalidRequest)
		}
		cancel()
		return okResponse(request.ID)
	default:
		return errorResponse(request.ID, errInvalidRequest)
	}
}

func (s *Server) startProbe(parent context.Context, request requestFrame) {
	select {
	case s.probeSlots <- struct{}{}:
	default:
		_ = s.write(errorResponse(request.ID, errUnavailable))
		return
	}
	probeCtx, cancel := context.WithTimeout(parent, time.Duration(request.TimeoutMS)*time.Millisecond)
	s.activeMu.Lock()
	s.active[request.ID] = cancel
	s.activeMu.Unlock()
	s.probes.Add(1)
	go func() {
		defer s.probes.Done()
		defer func() { <-s.probeSlots }()
		defer cancel()
		delay, err := s.controller.ProbeDelay(probeCtx, request.SelectorID, request.NodeID)
		response := okResponse(request.ID)
		if err != nil || delay == 0 || uint64(delay) > request.TimeoutMS {
			response = errorResponse(request.ID, classifyProbeError(probeCtx, err))
		} else {
			response.DelayMS = &delay
		}
		s.activeMu.Lock()
		delete(s.active, request.ID)
		s.activeMu.Unlock()
		_ = s.write(response)
	}()
}

func (s *Server) cancelAll() {
	s.activeMu.Lock()
	for _, cancel := range s.active {
		cancel()
	}
	s.activeMu.Unlock()
}

func (s *Server) write(response responseFrame) error {
	s.writeMu.Lock()
	defer s.writeMu.Unlock()
	return writeFrame(s.writer, response)
}

func decodeRequest(payload []byte) (requestFrame, error) {
	fields, err := decodeObject(payload)
	if err != nil || len(fields) == 0 {
		return requestFrame{}, errInvalidRequest
	}
	var version int
	var kind string
	var request requestFrame
	if !decodeField(fields, "version", &version) || version != ProtocolVersion ||
		!decodeField(fields, "kind", &kind) || kind != "request" ||
		!decodeField(fields, "id", &request.ID) || request.ID == 0 ||
		!decodeField(fields, "command", &request.Command) {
		return requestFrame{}, errInvalidRequest
	}
	allowed := map[string]bool{"version": true, "kind": true, "id": true, "command": true}
	switch request.Command {
	case "select_node":
		allowed["selectorId"], allowed["nodeId"] = true, true
		if !decodeField(fields, "selectorId", &request.SelectorID) ||
			!decodeField(fields, "nodeId", &request.NodeID) ||
			!validPublicID(request.SelectorID) || !validPublicID(request.NodeID) {
			return requestFrame{}, errInvalidRequest
		}
	case "read_selected_node":
		allowed["selectorId"] = true
		if !decodeField(fields, "selectorId", &request.SelectorID) || !validPublicID(request.SelectorID) {
			return requestFrame{}, errInvalidRequest
		}
	case "probe_delay":
		allowed["selectorId"], allowed["nodeId"], allowed["timeoutMs"] = true, true, true
		if !decodeField(fields, "selectorId", &request.SelectorID) ||
			!decodeField(fields, "nodeId", &request.NodeID) ||
			!decodeField(fields, "timeoutMs", &request.TimeoutMS) ||
			!validPublicID(request.SelectorID) || !validPublicID(request.NodeID) ||
			time.Duration(request.TimeoutMS)*time.Millisecond < MinProbeTimeout ||
			time.Duration(request.TimeoutMS)*time.Millisecond > MaxProbeTimeout {
			return requestFrame{}, errInvalidRequest
		}
	case "traffic":
	case "cancel_probe":
		allowed["targetRequestId"] = true
		if !decodeField(fields, "targetRequestId", &request.TargetRequestID) ||
			request.TargetRequestID == 0 || request.TargetRequestID >= request.ID {
			return requestFrame{}, errInvalidRequest
		}
	default:
		return requestFrame{}, errInvalidRequest
	}
	for field := range fields {
		if !allowed[field] {
			return requestFrame{}, errInvalidRequest
		}
	}
	return request, nil
}

func decodeObject(payload []byte) (map[string]json.RawMessage, error) {
	decoder := json.NewDecoder(bytes.NewReader(payload))
	token, err := decoder.Token()
	if err != nil || token != json.Delim('{') {
		return nil, errInvalidRequest
	}
	fields := make(map[string]json.RawMessage)
	for decoder.More() {
		token, err := decoder.Token()
		name, ok := token.(string)
		if err != nil || !ok {
			return nil, errInvalidRequest
		}
		if _, duplicate := fields[name]; duplicate {
			return nil, errInvalidRequest
		}
		var value json.RawMessage
		if err := decoder.Decode(&value); err != nil {
			return nil, errInvalidRequest
		}
		fields[name] = value
	}
	if token, err := decoder.Token(); err != nil || token != json.Delim('}') {
		return nil, errInvalidRequest
	}
	var trailing json.RawMessage
	if err := decoder.Decode(&trailing); !errors.Is(err, io.EOF) {
		return nil, errInvalidRequest
	}
	return fields, nil
}

func decodeField(fields map[string]json.RawMessage, name string, destination any) bool {
	value, found := fields[name]
	return found && json.Unmarshal(value, destination) == nil
}

func readFrame(reader io.Reader) ([]byte, error) {
	var header [4]byte
	if _, err := io.ReadFull(reader, header[:]); err != nil {
		return nil, err
	}
	size := binary.BigEndian.Uint32(header[:])
	if size == 0 || size > MaxFrameBytes {
		return nil, errInvalidRequest
	}
	payload := make([]byte, size)
	if _, err := io.ReadFull(reader, payload); err != nil {
		clear(payload)
		return nil, err
	}
	return payload, nil
}

func writeFrame(writer io.Writer, value any) error {
	payload, err := json.Marshal(value)
	if err != nil || len(payload) == 0 || len(payload) > MaxFrameBytes {
		return errInvalidRequest
	}
	defer clear(payload)
	var header [4]byte
	binary.BigEndian.PutUint32(header[:], uint32(len(payload)))
	if err := writeAll(writer, header[:]); err != nil {
		return err
	}
	return writeAll(writer, payload)
}

func writeAll(writer io.Writer, content []byte) error {
	for len(content) > 0 {
		written, err := writer.Write(content)
		if written < 0 || written > len(content) {
			return io.ErrShortWrite
		}
		content = content[written:]
		if err != nil {
			return err
		}
		if written == 0 {
			return io.ErrShortWrite
		}
	}
	return nil
}

func okResponse(id uint64) responseFrame {
	return responseFrame{Version: ProtocolVersion, Kind: "response", ID: id, Result: "ok"}
}

func errorResponse(id uint64, err error) responseFrame {
	code := "unavailable"
	switch {
	case errors.Is(err, errInvalidRequest):
		code = "invalid_request"
	case errors.Is(err, errUnknownSelector):
		code = "unknown_selector"
	case errors.Is(err, errUnknownNode):
		code = "unknown_node"
	case errors.Is(err, context.DeadlineExceeded):
		code = "timed_out"
	case errors.Is(err, context.Canceled):
		code = "cancelled"
	}
	return responseFrame{
		Version:   ProtocolVersion,
		Kind:      "response",
		ID:        id,
		Result:    "error",
		ErrorCode: code,
	}
}

func classifyProbeError(ctx context.Context, err error) error {
	if ctx.Err() != nil {
		return ctx.Err()
	}
	return err
}

package main

import (
	"context"
	"encoding/binary"
	"encoding/json"
	"errors"
	"io"
	"os"
	"regexp"
	"sync"
	"time"

	"orange.dev/native/controlplane"
)

const (
	protocolVersion = 1
	maxFrameBytes   = 2 << 20
)

var requestIDPattern = regexp.MustCompile(`^[A-Za-z0-9._-]{1,64}$`)

type wireOutbound struct {
	Protocol          controlplane.Protocol `json:"protocol"`
	Server            string                `json:"server"`
	Port              uint16                `json:"port"`
	Credential        []byte                `json:"credential"`
	TLSServerName     string                `json:"tlsServerName,omitempty"`
	ShadowsocksMethod string                `json:"shadowsocksMethod,omitempty"`
}

type wireLimits struct {
	ConnectTimeoutMS uint32 `json:"connectTimeoutMs"`
	RequestTimeoutMS uint32 `json:"requestTimeoutMs"`
	MaxConcurrent    int    `json:"maxConcurrent"`
	MaxRequestBytes  int64  `json:"maxRequestBytes"`
	MaxResponseBytes int64  `json:"maxResponseBytes"`
}

type wireStartupDNS struct {
	Protocol      controlplane.DNSProtocol `json:"protocol"`
	Server        string                   `json:"server"`
	Port          uint16                   `json:"port"`
	TLSServerName string                   `json:"tlsServerName,omitempty"`
}

type wireConfig struct {
	Outbound     wireOutbound     `json:"outbound"`
	StartupDNS   []wireStartupDNS `json:"startupDns"`
	AllowedHosts []string         `json:"allowedHosts"`
	Limits       wireLimits       `json:"limits"`
}

func (c *wireConfig) take() controlplane.Config {
	credential := string(c.Outbound.Credential)
	clear(c.Outbound.Credential)
	c.Outbound.Credential = nil
	startupDNS := make([]controlplane.StartupDNS, len(c.StartupDNS))
	for index, server := range c.StartupDNS {
		startupDNS[index] = controlplane.StartupDNS{
			Protocol:      server.Protocol,
			Server:        server.Server,
			Port:          server.Port,
			TLSServerName: server.TLSServerName,
		}
	}
	return controlplane.Config{
		Outbound: controlplane.OutboundConfig{
			Protocol:          c.Outbound.Protocol,
			Server:            c.Outbound.Server,
			Port:              c.Outbound.Port,
			Credential:        credential,
			TLSServerName:     c.Outbound.TLSServerName,
			ShadowsocksMethod: c.Outbound.ShadowsocksMethod,
		},
		StartupDNS:   startupDNS,
		AllowedHosts: c.AllowedHosts,
		Limits: controlplane.Limits{
			ConnectTimeout:   time.Duration(c.Limits.ConnectTimeoutMS) * time.Millisecond,
			RequestTimeout:   time.Duration(c.Limits.RequestTimeoutMS) * time.Millisecond,
			MaxConcurrent:    c.Limits.MaxConcurrent,
			MaxRequestBytes:  c.Limits.MaxRequestBytes,
			MaxResponseBytes: c.Limits.MaxResponseBytes,
		},
	}
}

type inboundFrame struct {
	Version int                   `json:"version"`
	Kind    string                `json:"kind"`
	ID      string                `json:"id,omitempty"`
	Config  *wireConfig           `json:"config,omitempty"`
	Request *controlplane.Request `json:"request,omitempty"`
}

type outboundFrame struct {
	Version   int                    `json:"version"`
	Kind      string                 `json:"kind"`
	ID        string                 `json:"id,omitempty"`
	Response  *controlplane.Response `json:"response,omitempty"`
	ErrorCode controlplane.ErrorCode `json:"errorCode,omitempty"`
}

func readFrame(reader io.Reader) (inboundFrame, error) {
	var header [4]byte
	if _, err := io.ReadFull(reader, header[:]); err != nil {
		return inboundFrame{}, err
	}
	size := binary.BigEndian.Uint32(header[:])
	if size == 0 || size > maxFrameBytes {
		return inboundFrame{}, errors.New("invalid frame size")
	}
	payload := make([]byte, size)
	defer clear(payload)
	if _, err := io.ReadFull(reader, payload); err != nil {
		return inboundFrame{}, err
	}
	var frame inboundFrame
	decoder := json.NewDecoder(newByteReader(payload))
	decoder.DisallowUnknownFields()
	if err := decoder.Decode(&frame); err != nil {
		return inboundFrame{}, errors.New("invalid frame")
	}
	var trailing struct{}
	if err := decoder.Decode(&trailing); !errors.Is(err, io.EOF) {
		return inboundFrame{}, errors.New("invalid frame")
	}
	return frame, nil
}

type byteReader struct {
	data   []byte
	offset int
}

func newByteReader(data []byte) *byteReader {
	return &byteReader{data: data}
}

func (r *byteReader) Read(destination []byte) (int, error) {
	if r.offset == len(r.data) {
		return 0, io.EOF
	}
	count := copy(destination, r.data[r.offset:])
	r.offset += count
	return count, nil
}

func writeFrame(writer io.Writer, frame outboundFrame) error {
	payload, err := json.Marshal(frame)
	if err != nil {
		return err
	}
	defer clear(payload)
	if len(payload) > maxFrameBytes {
		return errors.New("output frame is too large")
	}
	var header [4]byte
	binary.BigEndian.PutUint32(header[:], uint32(len(payload)))
	if err = writeAll(writer, header[:]); err != nil {
		return err
	}
	return writeAll(writer, payload)
}

func writeAll(writer io.Writer, content []byte) error {
	for len(content) > 0 {
		written, err := writer.Write(content)
		if err != nil {
			return err
		}
		if written <= 0 || written > len(content) {
			return io.ErrShortWrite
		}
		content = content[written:]
	}
	return nil
}

type session struct {
	bridge  *controlplane.Bridge
	writer  io.Writer
	writeMu sync.Mutex
	access  sync.Mutex
	cancels map[string]context.CancelFunc
	wait    sync.WaitGroup
}

func (s *session) send(frame outboundFrame) error {
	s.writeMu.Lock()
	defer s.writeMu.Unlock()
	return writeFrame(s.writer, frame)
}

func (s *session) sendError(id string, code controlplane.ErrorCode) error {
	return s.send(outboundFrame{Version: protocolVersion, Kind: "error", ID: id, ErrorCode: code})
}

func (s *session) startRequest(parent context.Context, frame inboundFrame) error {
	if !requestIDPattern.MatchString(frame.ID) || frame.Request == nil || frame.Config != nil {
		return s.sendError(frame.ID, controlplane.ErrorInvalidRequest)
	}
	requestContext, cancel := context.WithCancel(parent)
	s.access.Lock()
	if _, exists := s.cancels[frame.ID]; exists {
		s.access.Unlock()
		cancel()
		return s.sendError(frame.ID, controlplane.ErrorInvalidRequest)
	}
	s.cancels[frame.ID] = cancel
	s.access.Unlock()

	s.wait.Add(1)
	go func() {
		defer s.wait.Done()
		defer clear(frame.Request.Body)
		defer clear(frame.Request.AccessToken)
		response, err := s.bridge.Execute(requestContext, *frame.Request)
		s.access.Lock()
		delete(s.cancels, frame.ID)
		s.access.Unlock()
		cancel()
		if err != nil {
			var bridgeError *controlplane.BridgeError
			if errors.As(err, &bridgeError) {
				_ = s.sendError(frame.ID, bridgeError.Code)
			} else {
				_ = s.sendError(frame.ID, controlplane.ErrorUnavailable)
			}
			return
		}
		_ = s.send(outboundFrame{Version: protocolVersion, Kind: "response", ID: frame.ID, Response: &response})
		clear(response.Body)
	}()
	return nil
}

func (s *session) cancelRequest(frame inboundFrame) error {
	if !requestIDPattern.MatchString(frame.ID) || frame.Config != nil || frame.Request != nil {
		return s.sendError(frame.ID, controlplane.ErrorInvalidRequest)
	}
	s.access.Lock()
	cancel := s.cancels[frame.ID]
	s.access.Unlock()
	if cancel == nil {
		return s.sendError(frame.ID, controlplane.ErrorInvalidRequest)
	}
	cancel()
	return nil
}

func (s *session) cancelAll() {
	s.access.Lock()
	defer s.access.Unlock()
	for _, cancel := range s.cancels {
		cancel()
	}
}

func run(ctx context.Context, reader io.Reader, writer io.Writer) error {
	initial, err := readFrame(reader)
	if err != nil {
		return err
	}
	if initial.Version != protocolVersion || initial.Kind != "init" || initial.ID != "" || initial.Config == nil || initial.Request != nil {
		_ = writeFrame(writer, outboundFrame{Version: protocolVersion, Kind: "error", ErrorCode: controlplane.ErrorInvalidRequest})
		return errors.New("invalid initialization frame")
	}
	config := initial.Config.take()
	bridge, err := controlplane.New(ctx, config)
	config.Outbound.Credential = ""
	if err != nil {
		var bridgeError *controlplane.BridgeError
		code := controlplane.ErrorInvalidConfig
		if errors.As(err, &bridgeError) {
			code = bridgeError.Code
		}
		_ = writeFrame(writer, outboundFrame{Version: protocolVersion, Kind: "error", ErrorCode: code})
		return errors.New("control plane initialization failed")
	}
	defer bridge.Close()

	sessionContext, cancelSession := context.WithCancel(ctx)
	defer cancelSession()
	s := &session{bridge: bridge, writer: writer, cancels: make(map[string]context.CancelFunc)}
	if err = s.send(outboundFrame{Version: protocolVersion, Kind: "ready"}); err != nil {
		return err
	}
	for {
		frame, readErr := readFrame(reader)
		if errors.Is(readErr, io.EOF) {
			break
		}
		if readErr != nil {
			cancelSession()
			s.cancelAll()
			s.wait.Wait()
			return readErr
		}
		if frame.Version != protocolVersion {
			_ = s.sendError(frame.ID, controlplane.ErrorInvalidRequest)
			continue
		}
		switch frame.Kind {
		case "request":
			err = s.startRequest(sessionContext, frame)
		case "cancel":
			err = s.cancelRequest(frame)
		default:
			err = s.sendError(frame.ID, controlplane.ErrorInvalidRequest)
		}
		if err != nil {
			cancelSession()
			break
		}
	}
	cancelSession()
	s.cancelAll()
	s.wait.Wait()
	return err
}

func main() {
	if run(context.Background(), os.Stdin, os.Stdout) != nil {
		os.Exit(1)
	}
}

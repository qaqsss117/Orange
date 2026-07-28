package dataplane

import (
	"bytes"
	"context"
	"encoding/binary"
	"encoding/json"
	"errors"
	"io"
	"sync"
	"testing"
)

type fakeController struct {
	mu       sync.Mutex
	selected string
	upload   uint64
	download uint64
	probe    func(context.Context, string, string) (uint32, error)
}

type shortWriter struct {
	destination bytes.Buffer
	maximum     int
}

func (w *shortWriter) Write(content []byte) (int, error) {
	if len(content) > w.maximum {
		content = content[:w.maximum]
	}
	return w.destination.Write(content)
}

func (f *fakeController) SelectNode(selectorID, nodeID string) error {
	if selectorID != "proxy" {
		return errUnknownSelector
	}
	if nodeID != "node-a" && nodeID != "node-b" {
		return errUnknownNode
	}
	f.mu.Lock()
	f.selected = nodeID
	f.mu.Unlock()
	return nil
}

func (f *fakeController) ReadSelectedNode(selectorID string) (string, error) {
	if selectorID != "proxy" {
		return "", errUnknownSelector
	}
	f.mu.Lock()
	defer f.mu.Unlock()
	return f.selected, nil
}

func (f *fakeController) ProbeDelay(ctx context.Context, selectorID, nodeID string) (uint32, error) {
	if f.probe != nil {
		return f.probe(ctx, selectorID, nodeID)
	}
	return 23, nil
}

func (f *fakeController) TrafficTotals() (uint64, uint64, error) {
	return f.upload, f.download, nil
}

func TestFixedCommandsRoundTripWithoutCapabilityFields(t *testing.T) {
	controller := &fakeController{selected: "node-a", upload: 101, download: 303}
	requests := new(bytes.Buffer)
	writeTestRequest(t, requests, map[string]any{
		"version": 1, "kind": "request", "id": 1, "command": "select_node",
		"selectorId": "proxy", "nodeId": "node-b",
	})
	writeTestRequest(t, requests, map[string]any{
		"version": 1, "kind": "request", "id": 2, "command": "read_selected_node",
		"selectorId": "proxy",
	})
	writeTestRequest(t, requests, map[string]any{
		"version": 1, "kind": "request", "id": 3, "command": "probe_delay",
		"selectorId": "proxy", "nodeId": "node-b", "timeoutMs": 1000,
	})
	writeTestRequest(t, requests, map[string]any{
		"version": 1, "kind": "request", "id": 4, "command": "traffic",
	})
	responses := new(bytes.Buffer)
	if err := NewServer(controller, requests, responses).Run(context.Background()); err != nil {
		t.Fatal(err)
	}
	frames := readTestResponses(t, responses)
	if len(frames) != 5 || frames[0].Kind != "ready" {
		t.Fatalf("unexpected frames: %#v", frames)
	}
	byID := make(map[uint64]responseFrame)
	for _, frame := range frames[1:] {
		byID[frame.ID] = frame
	}
	if byID[1].Result != "ok" || value(byID[2].SelectedNodeID) != "node-b" || value(byID[3].DelayMS) != 23 {
		t.Fatalf("unexpected command responses: %#v", byID)
	}
	if value(byID[4].UploadBytesTotal) != 101 || value(byID[4].DownloadBytesTotal) != 303 {
		t.Fatalf("unexpected traffic response: %#v", byID[4])
	}
}

func TestProbeCanBeCancelledByCorrelatedRequest(t *testing.T) {
	started := make(chan struct{})
	controller := &fakeController{
		selected: "node-a",
		probe: func(ctx context.Context, _, _ string) (uint32, error) {
			close(started)
			<-ctx.Done()
			return 0, ctx.Err()
		},
	}
	inputReader, inputWriter := io.Pipe()
	outputReader, outputWriter := io.Pipe()
	serverDone := make(chan error, 1)
	go func() {
		serverDone <- NewServer(controller, inputReader, outputWriter).Run(context.Background())
		_ = outputWriter.Close()
	}()
	ready := readTestResponse(t, outputReader)
	if ready.Kind != "ready" {
		t.Fatalf("unexpected readiness: %#v", ready)
	}
	writeTestRequest(t, inputWriter, map[string]any{
		"version": 1, "kind": "request", "id": 1, "command": "probe_delay",
		"selectorId": "proxy", "nodeId": "node-a", "timeoutMs": 1000,
	})
	<-started
	writeTestRequest(t, inputWriter, map[string]any{
		"version": 1, "kind": "request", "id": 2, "command": "cancel_probe",
		"targetRequestId": 1,
	})
	first := readTestResponse(t, outputReader)
	second := readTestResponse(t, outputReader)
	responses := map[uint64]responseFrame{first.ID: first, second.ID: second}
	if responses[2].Result != "ok" || responses[1].ErrorCode != "cancelled" {
		t.Fatalf("unexpected cancellation responses: %#v", responses)
	}
	_ = inputWriter.Close()
	if err := <-serverDone; err != nil {
		t.Fatal(err)
	}
}

func TestUnknownFieldsAndNonMonotonicIDsFailClosed(t *testing.T) {
	for _, requests := range [][]map[string]any{
		{{"version": 1, "kind": "request", "id": 1, "command": "traffic", "url": "https://example.invalid"}},
		{
			{"version": 1, "kind": "request", "id": 2, "command": "traffic"},
			{"version": 1, "kind": "request", "id": 1, "command": "traffic"},
		},
		{{"version": 1, "kind": "request", "id": 1, "command": "shell"}},
	} {
		input := new(bytes.Buffer)
		for _, request := range requests {
			writeTestRequest(t, input, request)
		}
		err := NewServer(&fakeController{}, input, io.Discard).Run(context.Background())
		if !errors.Is(err, errInvalidRequest) {
			t.Fatalf("expected invalid request, got %v", err)
		}
	}
	duplicate := []byte(`{"version":1,"kind":"request","id":1,"id":2,"command":"traffic"}`)
	if _, err := decodeRequest(duplicate); !errors.Is(err, errInvalidRequest) {
		t.Fatalf("duplicate JSON field was accepted: %v", err)
	}
}

func TestFrameAndSemanticBoundsAreEnforced(t *testing.T) {
	oversized := make([]byte, 4)
	binary.BigEndian.PutUint32(oversized, MaxFrameBytes+1)
	if _, err := readFrame(bytes.NewReader(oversized)); !errors.Is(err, errInvalidRequest) {
		t.Fatalf("expected frame rejection, got %v", err)
	}
	for _, request := range []map[string]any{
		{"version": 1, "kind": "request", "id": 1, "command": "probe_delay", "selectorId": "proxy", "nodeId": "node-a", "timeoutMs": 99},
		{"version": 1, "kind": "request", "id": 1, "command": "select_node", "selectorId": "orange-private", "nodeId": "node-a"},
		{"version": 2, "kind": "request", "id": 1, "command": "traffic"},
	} {
		payload, err := json.Marshal(request)
		if err != nil {
			t.Fatal(err)
		}
		if _, err := decodeRequest(payload); !errors.Is(err, errInvalidRequest) {
			t.Fatalf("expected semantic rejection for %#v, got %v", request, err)
		}
	}
	valid, err := json.Marshal(map[string]any{"version": 1, "kind": "request", "id": 1, "command": "traffic"})
	if err != nil {
		t.Fatal(err)
	}
	if _, err := decodeRequest(append(valid, []byte(" null")...)); !errors.Is(err, errInvalidRequest) {
		t.Fatalf("trailing JSON value was accepted: %v", err)
	}
}

func TestZeroTrafficCountersRemainExplicit(t *testing.T) {
	input := new(bytes.Buffer)
	writeTestRequest(t, input, map[string]any{
		"version": 1, "kind": "request", "id": 1, "command": "traffic",
	})
	output := new(bytes.Buffer)
	if err := NewServer(&fakeController{}, input, output).Run(context.Background()); err != nil {
		t.Fatal(err)
	}
	_ = readTestResponse(t, output)
	payload, err := readFrame(output)
	if err != nil {
		t.Fatal(err)
	}
	var response map[string]any
	if err := json.Unmarshal(payload, &response); err != nil {
		t.Fatal(err)
	}
	if response["uploadBytesTotal"] != float64(0) || response["downloadBytesTotal"] != float64(0) {
		t.Fatalf("zero counters were omitted: %#v", response)
	}
}

func TestWriteFrameCompletesShortWrites(t *testing.T) {
	writer := &shortWriter{maximum: 2}
	expected := okResponse(7)
	if err := writeFrame(writer, expected); err != nil {
		t.Fatal(err)
	}
	actual := readTestResponse(t, &writer.destination)
	if actual != expected {
		t.Fatalf("unexpected response after short writes: %#v", actual)
	}
}

func TestProbeConcurrencyIsBounded(t *testing.T) {
	controller := &fakeController{
		probe: func(ctx context.Context, _, _ string) (uint32, error) {
			<-ctx.Done()
			return 0, ctx.Err()
		},
	}
	input := new(bytes.Buffer)
	for id := uint64(1); id <= MaxConcurrentProbes+1; id++ {
		writeTestRequest(t, input, map[string]any{
			"version": 1, "kind": "request", "id": id, "command": "probe_delay",
			"selectorId": "proxy", "nodeId": "node-a", "timeoutMs": 1000,
		})
	}
	output := new(bytes.Buffer)
	if err := NewServer(controller, input, output).Run(context.Background()); err != nil {
		t.Fatal(err)
	}
	responses := readTestResponses(t, output)
	byID := make(map[uint64]responseFrame)
	for _, response := range responses {
		byID[response.ID] = response
	}
	overflow := byID[MaxConcurrentProbes+1]
	if overflow.Result != "error" || overflow.ErrorCode != "unavailable" {
		t.Fatalf("ninth concurrent probe was not rejected: %#v", overflow)
	}
}

func writeTestRequest(t *testing.T, writer io.Writer, value any) {
	t.Helper()
	if err := writeFrame(writer, value); err != nil {
		t.Fatal(err)
	}
}

func readTestResponses(t *testing.T, reader io.Reader) []responseFrame {
	t.Helper()
	var responses []responseFrame
	for {
		payload, err := readFrame(reader)
		if errors.Is(err, io.EOF) {
			return responses
		}
		if err != nil {
			t.Fatal(err)
		}
		var response responseFrame
		if err := json.Unmarshal(payload, &response); err != nil {
			t.Fatal(err)
		}
		responses = append(responses, response)
	}
}

func readTestResponse(t *testing.T, reader io.Reader) responseFrame {
	t.Helper()
	payload, err := readFrame(reader)
	if err != nil {
		t.Fatal(err)
	}
	var response responseFrame
	if err := json.Unmarshal(payload, &response); err != nil {
		t.Fatal(err)
	}
	return response
}

func value[T comparable](pointer *T) T {
	if pointer == nil {
		var zero T
		return zero
	}
	return *pointer
}

package main

import (
	"bytes"
	"context"
	"encoding/binary"
	"encoding/json"
	"errors"
	"io"
	"testing"

	"orange.dev/native/controlplane"
)

func encodedInputFrame(t *testing.T, value object) []byte {
	t.Helper()
	payload, err := json.Marshal(value)
	if err != nil {
		t.Fatal(err)
	}
	result := make([]byte, 4+len(payload))
	binary.BigEndian.PutUint32(result[:4], uint32(len(payload)))
	copy(result[4:], payload)
	return result
}

type object = map[string]any

func TestFrameRoundTrip(t *testing.T) {
	input := encodedInputFrame(t, object{
		"version": protocolVersion,
		"kind":    "request",
		"id":      "request-1",
		"request": object{
			"method": "POST",
			"host":   "api.orange.invalid",
			"path":   "/v1/test",
			"body":   []byte("body"),
		},
	})
	frame, err := readFrame(bytes.NewReader(input))
	if err != nil {
		t.Fatal(err)
	}
	if frame.ID != "request-1" || frame.Request == nil || string(frame.Request.Body) != "body" {
		t.Fatalf("unexpected frame: %#v", frame)
	}

	var output bytes.Buffer
	response := controlplane.Response{StatusCode: 200, Body: []byte("ok")}
	if err = writeFrame(&output, outboundFrame{Version: protocolVersion, Kind: "response", ID: frame.ID, Response: &response}); err != nil {
		t.Fatal(err)
	}
	var header [4]byte
	if _, err = io.ReadFull(&output, header[:]); err != nil {
		t.Fatal(err)
	}
	payload := make([]byte, binary.BigEndian.Uint32(header[:]))
	if _, err = io.ReadFull(&output, payload); err != nil {
		t.Fatal(err)
	}
	if !bytes.Contains(payload, []byte(`"kind":"response"`)) || !bytes.Contains(payload, []byte(`"statusCode":200`)) {
		t.Fatalf("unexpected output frame: %s", payload)
	}
}

func TestFrameRejectsUnknownFieldsAndOversize(t *testing.T) {
	unknown := encodedInputFrame(t, object{"version": 1, "kind": "cancel", "unknown": true})
	if _, err := readFrame(bytes.NewReader(unknown)); err == nil {
		t.Fatal("unknown field was accepted")
	}
	var oversized [4]byte
	binary.BigEndian.PutUint32(oversized[:], maxFrameBytes+1)
	if _, err := readFrame(bytes.NewReader(oversized[:])); err == nil {
		t.Fatal("oversized frame was accepted")
	}
	truncated := encodedInputFrame(t, object{"version": 1, "kind": "cancel", "id": "request-1"})
	if _, err := readFrame(bytes.NewReader(truncated[:len(truncated)-1])); !errors.Is(err, io.ErrUnexpectedEOF) {
		t.Fatalf("truncated frame returned %v", err)
	}
}

type shortWriter struct {
	output bytes.Buffer
}

func (w *shortWriter) Write(content []byte) (int, error) {
	if len(content) > 3 {
		content = content[:3]
	}
	return w.output.Write(content)
}

func TestWriteFrameHandlesShortWrites(t *testing.T) {
	writer := new(shortWriter)
	if err := writeFrame(writer, outboundFrame{Version: 1, Kind: "ready"}); err != nil {
		t.Fatal(err)
	}
	if writer.output.Len() <= 4 {
		t.Fatal("frame payload was not written")
	}
}

func TestRunRejectsNonInitializationFrame(t *testing.T) {
	input := encodedInputFrame(t, object{"version": 1, "kind": "request", "id": "request-1"})
	var output bytes.Buffer
	if err := run(context.Background(), bytes.NewReader(input), &output); err == nil {
		t.Fatal("non-initialization frame was accepted")
	}
	if output.Len() <= 4 || !bytes.Contains(output.Bytes()[4:], []byte(`"errorCode":"invalid-request"`)) {
		t.Fatalf("missing redacted protocol error: %x", output.Bytes())
	}
}

func TestRequestIDPolicy(t *testing.T) {
	for _, valid := range []string{"a", "request-1", "request_2", "request.3"} {
		if !requestIDPattern.MatchString(valid) {
			t.Fatalf("valid request ID rejected: %s", valid)
		}
	}
	for _, invalid := range []string{"", "request id", "../request", string(bytes.Repeat([]byte("a"), 65))} {
		if requestIDPattern.MatchString(invalid) {
			t.Fatalf("invalid request ID accepted: %q", invalid)
		}
	}
}

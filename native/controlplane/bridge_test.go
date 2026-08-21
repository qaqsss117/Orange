package controlplane

import (
	"context"
	"errors"
	"io"
	"net/http"
	"net/http/httptrace"
	"strings"
	"sync/atomic"
	"testing"
	"time"
)

func TestRouteCombinationsPreferLastSuccessAndSkipCoolingRoutes(t *testing.T) {
	preferred := routeCombination{hostIndex: 1, outboundIndex: 1}
	cooling := routeCombination{hostIndex: 0, outboundIndex: 0}
	bridge := &Bridge{
		hostOrder:    []string{"api-a.example", "api-b.example"},
		outbounds:    make([]outboundDialFunc, 2),
		preferred:    preferred,
		hasPreferred: true,
		failedUntil:  map[routeCombination]time.Time{cooling: time.Now().Add(time.Minute)},
	}

	combinations := bridge.routeCombinations("api-a.example", true)
	if len(combinations) != 3 {
		t.Fatalf("expected three usable combinations, got %d", len(combinations))
	}
	if combinations[0] != preferred {
		t.Fatalf("expected preferred combination first, got %#v", combinations[0])
	}
	for _, combination := range combinations {
		if combination == cooling {
			t.Fatal("cooling combination was not removed")
		}
	}
}

func TestAttemptLimitAppliesToAllMethods(t *testing.T) {
	if got := attemptLimit(http.MethodPost, 8, 3); got != 3 {
		t.Fatalf("POST attempt limit = %d, want 3", got)
	}
	if got := attemptLimit(http.MethodGet, 8, 3); got != 3 {
		t.Fatalf("GET attempt limit = %d, want 3", got)
	}
}

func TestExecuteUsesTheSelectedOutboundClient(t *testing.T) {
	var calls [2]atomic.Int32
	clients := make([]*http.Client, 2)
	for index := range clients {
		clientIndex := index
		clients[index] = &http.Client{Transport: roundTripFunc(func(*http.Request) (*http.Response, error) {
			calls[clientIndex].Add(1)
			return &http.Response{
				StatusCode: http.StatusOK,
				Header:     make(http.Header),
				Body:       io.NopCloser(strings.NewReader("ok")),
			}, nil
		})}
	}
	bridge := testBridge(roundTripFunc(func(*http.Request) (*http.Response, error) {
		t.Fatal("fallback client was used")
		return nil, nil
	}))
	bridge.outbounds = make([]outboundDialFunc, 2)
	bridge.routeClients = clients
	bridge.preferred = routeCombination{hostIndex: 0, outboundIndex: 1}
	bridge.hasPreferred = true
	bridge.limits.MaxAttempts = 1

	response, err := bridge.Execute(context.Background(), Request{
		Method: http.MethodGet,
		Host:   "api-a.example",
		Path:   "/health",
	})
	if err != nil || string(response.Body) != "ok" {
		t.Fatalf("selected outbound request failed: response=%#v err=%v", response, err)
	}
	if calls[0].Load() != 0 || calls[1].Load() != 1 {
		t.Fatalf("outbound client calls = [%d, %d], want [0, 1]", calls[0].Load(), calls[1].Load())
	}
}

func TestGetRotatesToTheNextOutboundClient(t *testing.T) {
	var calls [2]atomic.Int32
	bridge := testBridge(roundTripFunc(func(*http.Request) (*http.Response, error) {
		t.Fatal("fallback client was used")
		return nil, nil
	}))
	bridge.outbounds = make([]outboundDialFunc, 2)
	bridge.routeClients = []*http.Client{
		{Transport: roundTripFunc(func(*http.Request) (*http.Response, error) {
			calls[0].Add(1)
			return nil, errors.New("first proxy failed")
		})},
		{Transport: roundTripFunc(func(*http.Request) (*http.Response, error) {
			calls[1].Add(1)
			return &http.Response{
				StatusCode: http.StatusOK,
				Header:     make(http.Header),
				Body:       io.NopCloser(strings.NewReader("ok")),
			}, nil
		})},
	}

	response, err := bridge.Execute(context.Background(), Request{
		Method: http.MethodGet,
		Host:   "api-a.example",
		Path:   "/health",
	})
	if err != nil || string(response.Body) != "ok" {
		t.Fatalf("GET outbound rotation failed: response=%#v err=%v", response, err)
	}
	if calls[0].Load() != 1 || calls[1].Load() != 1 {
		t.Fatalf("outbound client calls = [%d, %d], want [1, 1]", calls[0].Load(), calls[1].Load())
	}
}

func TestRouteReturnsAfterCircuitBreakerCooldown(t *testing.T) {
	failed := routeCombination{hostIndex: 0, outboundIndex: 0}
	bridge := &Bridge{
		hostOrder:   []string{"api-a.example"},
		outbounds:   make([]outboundDialFunc, 2),
		failedUntil: map[routeCombination]time.Time{failed: time.Now().Add(time.Minute)},
	}
	for _, combination := range bridge.routeCombinations("api-a.example", false) {
		if combination == failed {
			t.Fatal("cooling route was available before cooldown elapsed")
		}
	}
	bridge.failedUntil[failed] = time.Now().Add(-time.Millisecond)
	found := false
	for _, combination := range bridge.routeCombinations("api-a.example", false) {
		found = found || combination == failed
	}
	if !found {
		t.Fatal("route did not recover after circuit breaker cooldown")
	}
}

type roundTripFunc func(*http.Request) (*http.Response, error)

func (function roundTripFunc) RoundTrip(request *http.Request) (*http.Response, error) {
	return function(request)
}

func testBridge(transport http.RoundTripper) *Bridge {
	lifecycle, cancel := context.WithCancel(context.Background())
	return &Bridge{
		client:       &http.Client{Transport: transport},
		transport:    &http.Transport{},
		allowedHosts: map[string]struct{}{"api-a.example": {}, "api-b.example": {}},
		hostOrder:    []string{"api-a.example", "api-b.example"},
		outbounds:    make([]outboundDialFunc, 1),
		limits: Limits{
			RequestTimeout:   time.Second,
			MaxConcurrent:    1,
			MaxRequestBytes:  1024,
			MaxResponseBytes: 1024,
			MaxAttempts:      2,
			BackoffBase:      time.Millisecond,
		},
		slots:       make(chan struct{}, 1),
		lifecycle:   lifecycle,
		cancel:      cancel,
		failedUntil: make(map[routeCombination]time.Time),
	}
}

func TestPostSwitchesHostOnlyBeforeRequestIsWritten(t *testing.T) {
	var calls atomic.Int32
	bridge := testBridge(roundTripFunc(func(request *http.Request) (*http.Response, error) {
		if calls.Add(1) == 1 {
			return nil, errors.New("connect failed")
		}
		if request.URL.Hostname() != "api-b.example" {
			t.Fatalf("second attempt used %q, want api-b.example", request.URL.Hostname())
		}
		return &http.Response{
			StatusCode: http.StatusOK,
			Header:     make(http.Header),
			Body:       io.NopCloser(strings.NewReader("ok")),
		}, nil
	}))

	response, err := bridge.Execute(context.Background(), Request{
		Method:     http.MethodPost,
		UsePrimary: true,
		Path:       "/submit",
	})
	if err != nil || string(response.Body) != "ok" {
		t.Fatalf("POST preflight fallback failed: response=%#v err=%v", response, err)
	}
	if calls.Load() != 2 {
		t.Fatalf("POST attempts = %d, want 2", calls.Load())
	}
}

func TestPostDoesNotRetryAfterRequestWasWritten(t *testing.T) {
	var calls atomic.Int32
	bridge := testBridge(roundTripFunc(func(request *http.Request) (*http.Response, error) {
		calls.Add(1)
		trace := httptrace.ContextClientTrace(request.Context())
		trace.WroteRequest(httptrace.WroteRequestInfo{})
		return nil, errors.New("response lost")
	}))

	_, err := bridge.Execute(context.Background(), Request{
		Method:     http.MethodPost,
		UsePrimary: true,
		Path:       "/submit",
	})
	if err == nil {
		t.Fatal("POST with an uncertain outcome unexpectedly succeeded")
	}
	if calls.Load() != 1 {
		t.Fatalf("POST attempts = %d, want 1", calls.Load())
	}
}

package controlplane

import (
	"bytes"
	"context"
	"crypto/x509"
	"encoding/json"
	"io"
	"net"
	"net/http"
	"net/http/httptest"
	"net/netip"
	"net/url"
	"os"
	"strconv"
	"strings"
	"sync"
	"sync/atomic"
	"testing"
	"time"

	mDNS "github.com/miekg/dns"
	box "github.com/sagernet/sing-box"
	"github.com/sagernet/sing-box/adapter/endpoint"
	"github.com/sagernet/sing-box/adapter/inbound"
	"github.com/sagernet/sing-box/adapter/outbound"
	boxservice "github.com/sagernet/sing-box/adapter/service"
	"github.com/sagernet/sing-box/constant"
	"github.com/sagernet/sing-box/dns"
	"github.com/sagernet/sing-box/dns/transport/local"
	"github.com/sagernet/sing-box/option"
	"github.com/sagernet/sing-box/protocol/shadowsocks"
	"github.com/sagernet/sing/common/json/badoption"
)

const (
	testMethod   = "chacha20-ietf-poly1305"
	testPassword = "orange-direct-dial-test-only"
)

type testAPI struct {
	server *httptest.Server
	host   string
	port   uint16
	roots  *x509.CertPool
}

func startTestAPI(t *testing.T, handler http.Handler) testAPI {
	t.Helper()
	server := httptest.NewTLSServer(handler)
	t.Cleanup(server.Close)
	parsed, err := url.Parse(server.URL)
	if err != nil {
		t.Fatal(err)
	}
	host, portText, err := net.SplitHostPort(parsed.Host)
	if err != nil {
		t.Fatal(err)
	}
	port, err := strconv.ParseUint(portText, 10, 16)
	if err != nil {
		t.Fatal(err)
	}
	roots := x509.NewCertPool()
	roots.AddCert(server.Certificate())
	return testAPI{server: server, host: host, port: uint16(port), roots: roots}
}

func reserveTCPPort(t *testing.T) uint16 {
	t.Helper()
	listener, err := net.Listen("tcp4", "127.0.0.1:0")
	if err != nil {
		t.Fatal(err)
	}
	port := listener.Addr().(*net.TCPAddr).Port
	if err = listener.Close(); err != nil {
		t.Fatal(err)
	}
	return uint16(port)
}

func startTestProxy(t *testing.T) (*box.Box, uint16) {
	t.Helper()
	port := reserveTCPPort(t)
	inboundRegistry := inbound.NewRegistry()
	shadowsocks.RegisterInbound(inboundRegistry)
	dnsRegistry := dns.NewTransportRegistry()
	local.RegisterTransport(dnsRegistry)
	address := badoption.Addr(netip.MustParseAddr("127.0.0.1"))
	ctx := box.Context(
		context.Background(),
		inboundRegistry,
		outbound.NewRegistry(),
		endpoint.NewRegistry(),
		dnsRegistry,
		boxservice.NewRegistry(),
	)
	instance, err := box.New(box.Options{
		Context: ctx,
		Options: option.Options{
			Log: &option.LogOptions{Disabled: true},
			Inbounds: []option.Inbound{{
				Type: constant.TypeShadowsocks,
				Tag:  "test-proxy",
				Options: &option.ShadowsocksInboundOptions{
					ListenOptions: option.ListenOptions{
						Listen:     &address,
						ListenPort: port,
					},
					Network:  option.NetworkList("tcp"),
					Method:   testMethod,
					Password: testPassword,
				},
			}},
		},
	})
	if err != nil {
		t.Fatal(err)
	}
	if err = instance.Start(); err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = instance.Close() })
	return instance, port
}

func startTestDNS(t *testing.T, host string, address net.IP) (uint16, *atomic.Int32) {
	t.Helper()
	packetConn, err := net.ListenPacket("udp4", "127.0.0.1:0")
	if err != nil {
		t.Fatal(err)
	}
	queries := new(atomic.Int32)
	started := make(chan struct{})
	server := &mDNS.Server{
		PacketConn: packetConn,
		NotifyStartedFunc: func() {
			close(started)
		},
		Handler: mDNS.HandlerFunc(func(writer mDNS.ResponseWriter, request *mDNS.Msg) {
			response := new(mDNS.Msg)
			response.SetReply(request)
			response.Authoritative = true
			for _, question := range request.Question {
				if question.Name == mDNS.Fqdn(host) && question.Qtype == mDNS.TypeA {
					queries.Add(1)
					response.Answer = append(response.Answer, &mDNS.A{
						Hdr: mDNS.RR_Header{
							Name:   question.Name,
							Rrtype: mDNS.TypeA,
							Class:  mDNS.ClassINET,
							Ttl:    60,
						},
						A: address,
					})
				}
			}
			_ = writer.WriteMsg(response)
		}),
	}
	go func() { _ = server.ActivateAndServe() }()
	select {
	case <-started:
	case <-time.After(3 * time.Second):
		t.Fatal("test DNS server did not start")
	}
	t.Cleanup(func() { _ = server.Shutdown() })
	return uint16(packetConn.LocalAddr().(*net.UDPAddr).Port), queries
}

func testConfig(proxyPort uint16, host string, limits Limits) Config {
	return Config{
		Outbound: OutboundConfig{
			Protocol:          ProtocolShadowsocks,
			Server:            "127.0.0.1",
			Port:              proxyPort,
			Credential:        testPassword,
			ShadowsocksMethod: testMethod,
		},
		StartupDNS: []StartupDNS{{
			Protocol: DNSProtocolUDP,
			Server:   "127.0.0.1",
			Port:     53,
		}},
		AllowedHosts: []string{host},
		Limits:       limits,
	}
}

func startTestBridge(t *testing.T, proxyPort uint16, api testAPI, limits Limits) *Bridge {
	t.Helper()
	bridge, err := newBridge(context.Background(), testConfig(proxyPort, api.host, limits), bridgeOptions{
		targetPort: api.port,
		rootCAs:    api.roots,
	})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = bridge.Close() })
	return bridge
}

func testLimits() Limits {
	limits := DefaultLimits()
	limits.ConnectTimeout = 500 * time.Millisecond
	limits.RequestTimeout = 3 * time.Second
	return limits
}

func TestControlPlaneConfigurationHasNoInboundOrDirectFallback(t *testing.T) {
	config, err := validateConfig(testConfig(1234, "api.orange.invalid", testLimits()))
	if err != nil {
		t.Fatal(err)
	}
	options, err := buildBoxOptions(context.Background(), config)
	if err != nil {
		t.Fatal(err)
	}
	if len(options.Inbounds) != 0 {
		t.Fatalf("Control Plane has %d inbounds", len(options.Inbounds))
	}
	if len(options.Outbounds) != 1 || options.Outbounds[0].Type != constant.TypeShadowsocks {
		t.Fatalf("unexpected outbound set: %#v", options.Outbounds)
	}
	if options.Route == nil || options.Route.Final != controlPlaneTag {
		t.Fatal("Control Plane route does not fail closed to the proxy outbound")
	}
	shadowsocksOptions := options.Outbounds[0].Options.(*option.ShadowsocksOutboundOptions)
	if shadowsocksOptions.DomainResolver == nil || shadowsocksOptions.DomainResolver.Server != startupDNSTag(0) {
		t.Fatal("proxy outbound does not use the explicit startup DNS")
	}
	if options.DNS == nil || options.DNS.Final != startupDNSTag(0) || len(options.DNS.Servers) != 1 {
		t.Fatalf("unexpected startup DNS configuration: %#v", options.DNS)
	}
}

func TestStartupDNSProtocolsAndValidation(t *testing.T) {
	config := testConfig(1234, "api.orange.invalid", testLimits())
	config.Outbound.Server = "PROXY.ORANGE.INVALID"
	config.StartupDNS = []StartupDNS{
		{Protocol: DNSProtocolUDP, Server: "1.1.1.1", Port: 53},
		{Protocol: DNSProtocolTCP, Server: "8.8.8.8", Port: 53},
		{Protocol: DNSProtocolTLS, Server: "9.9.9.9", Port: 853, TLSServerName: "DNS.QUAD9.NET"},
	}
	validated, err := validateConfig(config)
	if err != nil {
		t.Fatal(err)
	}
	if validated.Outbound.Server != "proxy.orange.invalid" || validated.StartupDNS[2].TLSServerName != "dns.quad9.net" {
		t.Fatal("bootstrap hosts were not normalized")
	}
	options, err := buildBoxOptions(context.Background(), validated)
	if err != nil {
		t.Fatal(err)
	}
	if len(options.DNS.Servers) != 3 || options.DNS.Servers[0].Type != constant.DNSTypeUDP ||
		options.DNS.Servers[1].Type != constant.DNSTypeTCP || options.DNS.Servers[2].Type != constant.DNSTypeTLS {
		t.Fatalf("unexpected DNS transports: %#v", options.DNS.Servers)
	}
	tlsOptions := options.DNS.Servers[2].Options.(*option.RemoteTLSDNSServerOptions)
	if tlsOptions.TLS == nil || !tlsOptions.TLS.Enabled || tlsOptions.TLS.ServerName != "dns.quad9.net" || tlsOptions.TLS.MinVersion != "1.2" {
		t.Fatalf("unexpected DNS TLS options: %#v", tlsOptions.TLS)
	}

	domainDNSConfig := testConfig(1234, "api.orange.invalid", testLimits())
	domainDNSConfig.StartupDNS[0].Server = "resolver.orange.invalid"
	domainDNSConfig, err = validateConfig(domainDNSConfig)
	if err != nil {
		t.Fatal(err)
	}
	domainDNSOptions := buildDNSOptions(domainDNSConfig)
	if len(domainDNSOptions.Servers) != 2 || domainDNSOptions.Servers[0].Tag != startupDNSSystemTag {
		t.Fatalf("missing system bootstrap resolver: %#v", domainDNSOptions.Servers)
	}
	remote := domainDNSOptions.Servers[1].Options.(*option.RemoteDNSServerOptions)
	if remote.DomainResolver == nil || remote.DomainResolver.Server != startupDNSSystemTag {
		t.Fatal("domain-based DNS server does not have a bootstrap resolver")
	}

	cloneConfig := func() Config {
		value := config
		value.StartupDNS = append([]StartupDNS(nil), config.StartupDNS...)
		return value
	}
	invalid := []Config{
		func() Config { value := cloneConfig(); value.StartupDNS = nil; return value }(),
		func() Config {
			value := cloneConfig()
			value.StartupDNS = append(value.StartupDNS, value.StartupDNS[0], value.StartupDNS[0])
			return value
		}(),
		func() Config { value := cloneConfig(); value.StartupDNS[0].Protocol = "https"; return value }(),
		func() Config { value := cloneConfig(); value.StartupDNS[2].TLSServerName = ""; return value }(),
		func() Config { value := cloneConfig(); value.StartupDNS[0].TLSServerName = "dns.invalid"; return value }(),
	}
	for index, value := range invalid {
		if _, err = validateConfig(value); err == nil || !IsErrorCode(err, ErrorInvalidConfig) {
			t.Fatalf("invalid startup DNS case %d was accepted: %v", index, err)
		}
	}
}

func TestProxyDomainUsesExplicitStartupDNS(t *testing.T) {
	_, proxyPort := startTestProxy(t)
	dnsPort, queries := startTestDNS(t, "proxy.orange.invalid", net.ParseIP("127.0.0.1"))
	api := startTestAPI(t, http.HandlerFunc(func(writer http.ResponseWriter, _ *http.Request) {
		writer.WriteHeader(http.StatusNoContent)
	}))
	config := testConfig(proxyPort, api.host, testLimits())
	config.Outbound.Server = "proxy.orange.invalid"
	config.StartupDNS[0].Port = dnsPort
	bridge, err := newBridge(context.Background(), config, bridgeOptions{targetPort: api.port, rootCAs: api.roots})
	if err != nil {
		t.Fatal(err)
	}
	defer bridge.Close()
	response, err := bridge.Execute(context.Background(), Request{Method: http.MethodGet, Host: api.host, Path: "/"})
	if err != nil {
		t.Fatal(err)
	}
	if response.StatusCode != http.StatusNoContent || queries.Load() == 0 {
		t.Fatalf("explicit startup DNS was not used: status=%d queries=%d", response.StatusCode, queries.Load())
	}
}

func TestDirectDialGETAndPOSTThroughShadowsocks(t *testing.T) {
	_, proxyPort := startTestProxy(t)
	api := startTestAPI(t, http.HandlerFunc(func(writer http.ResponseWriter, request *http.Request) {
		writer.Header().Set("Content-Type", "application/json")
		switch request.URL.Path {
		case "/get":
			_ = json.NewEncoder(writer).Encode(map[string]string{"query": request.URL.Query().Get("probe")})
		case "/post":
			body, _ := io.ReadAll(request.Body)
			_ = json.NewEncoder(writer).Encode(map[string]string{"body": string(body)})
		default:
			http.NotFound(writer, request)
		}
	}))
	bridge := startTestBridge(t, proxyPort, api, testLimits())

	getResponse, err := bridge.Execute(context.Background(), Request{
		Method: http.MethodGet,
		Host:   api.host,
		Path:   "/get?probe=orange",
	})
	if err != nil {
		t.Fatal(err)
	}
	if getResponse.StatusCode != http.StatusOK || !strings.Contains(string(getResponse.Body), `"query":"orange"`) {
		t.Fatalf("unexpected GET response: %#v", getResponse)
	}

	postResponse, err := bridge.Execute(context.Background(), Request{
		Method:      http.MethodPost,
		Host:        api.host,
		Path:        "/post",
		ContentType: "application/json",
		Body:        []byte(`{"hello":"orange"}`),
	})
	if err != nil {
		t.Fatal(err)
	}
	if postResponse.StatusCode != http.StatusOK || !strings.Contains(string(postResponse.Body), `\"hello\":\"orange\"`) {
		t.Fatalf("unexpected POST response: %#v", postResponse)
	}
}

func TestAccessTokenIsInjectedAsBearerAndCleared(t *testing.T) {
	_, proxyPort := startTestProxy(t)
	api := startTestAPI(t, http.HandlerFunc(func(writer http.ResponseWriter, request *http.Request) {
		if request.Header.Get("Authorization") != "Bearer access-token.fixture" {
			http.Error(writer, "unauthorized", http.StatusUnauthorized)
			return
		}
		writer.WriteHeader(http.StatusNoContent)
	}))
	bridge := startTestBridge(t, proxyPort, api, testLimits())

	token := []byte("access-token.fixture")
	response, err := bridge.Execute(context.Background(), Request{
		Method:      http.MethodGet,
		Host:        api.host,
		Path:        "/authorized",
		AccessToken: token,
	})
	if err != nil {
		t.Fatal(err)
	}
	if response.StatusCode != http.StatusNoContent || !bytes.Equal(token, make([]byte, len(token))) {
		t.Fatalf("access token was not injected and cleared: status=%d", response.StatusCode)
	}

	invalid := []byte("token\r\ninjected")
	_, err = bridge.Execute(context.Background(), Request{
		Method:      http.MethodGet,
		Host:        api.host,
		Path:        "/authorized",
		AccessToken: invalid,
	})
	if err == nil || !IsErrorCode(err, ErrorInvalidRequest) || !bytes.Equal(invalid, make([]byte, len(invalid))) {
		t.Fatalf("invalid access token was not rejected and cleared: %v", err)
	}
}

func TestLiveDirectDialGETAndPOST(t *testing.T) {
	if os.Getenv("ORANGE_RUN_LIVE_CONTROL_PLANE") != "1" {
		t.Skip("set ORANGE_RUN_LIVE_CONTROL_PLANE=1 to run the overseas API PoC")
	}
	_, proxyPort := startTestProxy(t)
	limits := testLimits()
	limits.ConnectTimeout = 10 * time.Second
	limits.RequestTimeout = 20 * time.Second
	api := testAPI{host: "postman-echo.com", port: 443}
	bridge := startTestBridge(t, proxyPort, api, limits)

	getResponse, err := bridge.Execute(context.Background(), Request{
		Method: http.MethodGet,
		Host:   api.host,
		Path:   "/get?probe=orange",
	})
	if err != nil {
		t.Fatal(err)
	}
	if getResponse.StatusCode != http.StatusOK || !strings.Contains(string(getResponse.Body), `"probe":"orange"`) {
		t.Fatalf("unexpected live GET response: status=%d bytes=%d", getResponse.StatusCode, len(getResponse.Body))
	}

	postResponse, err := bridge.Execute(context.Background(), Request{
		Method:      http.MethodPost,
		Host:        api.host,
		Path:        "/post",
		ContentType: "application/json",
		Body:        []byte(`{"probe":"orange"}`),
	})
	if err != nil {
		t.Fatal(err)
	}
	var postEcho struct {
		Data map[string]string `json:"data"`
	}
	if json.Unmarshal(postResponse.Body, &postEcho) != nil || postResponse.StatusCode != http.StatusOK || postEcho.Data["probe"] != "orange" {
		t.Fatalf("unexpected live POST response: status=%d bytes=%d", postResponse.StatusCode, len(postResponse.Body))
	}
}

func TestBlockedProxyDoesNotFallBackToAPI(t *testing.T) {
	var hits atomic.Int32
	api := startTestAPI(t, http.HandlerFunc(func(writer http.ResponseWriter, _ *http.Request) {
		hits.Add(1)
		writer.WriteHeader(http.StatusOK)
	}))
	bridge := startTestBridge(t, reserveTCPPort(t), api, testLimits())
	_, err := bridge.Execute(context.Background(), Request{Method: http.MethodGet, Host: api.host, Path: "/"})
	if err == nil || !IsErrorCode(err, ErrorUnavailable) {
		t.Fatalf("expected fail-closed unavailable error, got %v", err)
	}
	if hits.Load() != 0 {
		t.Fatal("API was reached after the proxy was blocked")
	}
}

func TestRedirectIsReturnedWithoutFollowing(t *testing.T) {
	_, proxyPort := startTestProxy(t)
	var redirectedHits atomic.Int32
	redirected := startTestAPI(t, http.HandlerFunc(func(writer http.ResponseWriter, _ *http.Request) {
		redirectedHits.Add(1)
		writer.WriteHeader(http.StatusNoContent)
	}))
	api := startTestAPI(t, http.HandlerFunc(func(writer http.ResponseWriter, request *http.Request) {
		http.Redirect(writer, request, redirected.server.URL+"/not-approved", http.StatusFound)
	}))
	bridge := startTestBridge(t, proxyPort, api, testLimits())

	response, err := bridge.Execute(context.Background(), Request{
		Method: http.MethodGet,
		Host:   api.host,
		Path:   "/redirect",
	})
	if err != nil {
		t.Fatal(err)
	}
	if response.StatusCode != http.StatusFound {
		t.Fatalf("unexpected redirect response: %d", response.StatusCode)
	}
	if redirectedHits.Load() != 0 {
		t.Fatal("Control Plane followed an unapproved redirect")
	}
}

func TestTLSAndDNSFailuresAreRejected(t *testing.T) {
	_, proxyPort := startTestProxy(t)
	api := startTestAPI(t, http.HandlerFunc(func(writer http.ResponseWriter, _ *http.Request) {
		writer.WriteHeader(http.StatusOK)
	}))

	t.Run("TLS", func(t *testing.T) {
		bridge, err := newBridge(context.Background(), testConfig(proxyPort, api.host, testLimits()), bridgeOptions{targetPort: api.port})
		if err != nil {
			t.Fatal(err)
		}
		defer bridge.Close()
		_, err = bridge.Execute(context.Background(), Request{Method: http.MethodGet, Host: api.host, Path: "/"})
		if err == nil || !IsErrorCode(err, ErrorTLS) {
			t.Fatalf("expected TLS error, got %v", err)
		}
	})

	t.Run("DNS", func(t *testing.T) {
		config := testConfig(proxyPort, "missing.orange.invalid", testLimits())
		bridge, err := newBridge(context.Background(), config, bridgeOptions{targetPort: 443, rootCAs: api.roots})
		if err != nil {
			t.Fatal(err)
		}
		defer bridge.Close()
		_, err = bridge.Execute(context.Background(), Request{Method: http.MethodGet, Host: "missing.orange.invalid", Path: "/"})
		if err == nil {
			t.Fatal("expected DNS failure")
		}
	})
}

func TestTimeoutCancellationAndResponseLimit(t *testing.T) {
	_, proxyPort := startTestProxy(t)
	started := make(chan struct{}, 3)
	api := startTestAPI(t, http.HandlerFunc(func(writer http.ResponseWriter, request *http.Request) {
		switch request.URL.Path {
		case "/slow":
			started <- struct{}{}
			select {
			case <-request.Context().Done():
			case <-time.After(2 * time.Second):
				writer.WriteHeader(http.StatusOK)
			}
		case "/large":
			_, _ = writer.Write([]byte(strings.Repeat("x", 65)))
		}
	}))
	limits := testLimits()
	limits.RequestTimeout = 600 * time.Millisecond
	limits.MaxResponseBytes = 64
	bridge := startTestBridge(t, proxyPort, api, limits)

	_, err := bridge.Execute(context.Background(), Request{Method: http.MethodGet, Host: api.host, Path: "/slow"})
	if err == nil || !IsErrorCode(err, ErrorTimeout) {
		t.Fatalf("expected timeout, got %v", err)
	}
	<-started

	cancelContext, cancel := context.WithCancel(context.Background())
	result := make(chan error, 1)
	go func() {
		_, executeErr := bridge.Execute(cancelContext, Request{Method: http.MethodGet, Host: api.host, Path: "/slow"})
		result <- executeErr
	}()
	<-started
	cancel()
	if err = <-result; err == nil || !IsErrorCode(err, ErrorCanceled) {
		t.Fatalf("expected canceled error, got %v", err)
	}

	_, err = bridge.Execute(context.Background(), Request{Method: http.MethodGet, Host: api.host, Path: "/large"})
	if err == nil || !IsErrorCode(err, ErrorResponseTooLarge) {
		t.Fatalf("expected response limit error, got %v", err)
	}
}

func TestConcurrencyLimit(t *testing.T) {
	_, proxyPort := startTestProxy(t)
	var active atomic.Int32
	var maximum atomic.Int32
	arrived := make(chan struct{}, 6)
	release := make(chan struct{})
	api := startTestAPI(t, http.HandlerFunc(func(writer http.ResponseWriter, _ *http.Request) {
		current := active.Add(1)
		defer active.Add(-1)
		for current > maximum.Load() && !maximum.CompareAndSwap(maximum.Load(), current) {
		}
		arrived <- struct{}{}
		<-release
		writer.WriteHeader(http.StatusOK)
	}))
	limits := testLimits()
	limits.MaxConcurrent = 2
	bridge := startTestBridge(t, proxyPort, api, limits)

	const requests = 6
	var wait sync.WaitGroup
	wait.Add(requests)
	errorsFound := make(chan error, requests)
	for range requests {
		go func() {
			defer wait.Done()
			_, err := bridge.Execute(context.Background(), Request{Method: http.MethodGet, Host: api.host, Path: "/"})
			errorsFound <- err
		}()
	}
	<-arrived
	<-arrived
	time.Sleep(100 * time.Millisecond)
	if maximum.Load() != 2 || active.Load() != 2 {
		t.Fatalf("concurrency limit was not enforced: active=%d maximum=%d", active.Load(), maximum.Load())
	}
	close(release)
	wait.Wait()
	close(errorsFound)
	for err := range errorsFound {
		if err != nil {
			t.Fatal(err)
		}
	}
}

func TestInvalidRequestsAndClose(t *testing.T) {
	_, proxyPort := startTestProxy(t)
	api := startTestAPI(t, http.HandlerFunc(func(writer http.ResponseWriter, _ *http.Request) {
		writer.WriteHeader(http.StatusOK)
	}))
	bridge := startTestBridge(t, proxyPort, api, testLimits())
	requests := []Request{
		{Method: http.MethodDelete, Host: api.host, Path: "/"},
		{Method: http.MethodGet, Host: "other.orange.invalid", Path: "/"},
		{Method: http.MethodGet, Host: api.host, Path: "https://other.orange.invalid/"},
		{Method: http.MethodGet, Host: api.host, Path: "/", Body: []byte("not-allowed")},
		{Method: http.MethodPost, Host: api.host, Path: "/", ContentType: "text/plain\r\ninjected: yes"},
	}
	for _, request := range requests {
		if _, err := bridge.Execute(context.Background(), request); err == nil || !IsErrorCode(err, ErrorInvalidRequest) {
			t.Fatalf("request was not rejected: %#v, error=%v", request, err)
		}
	}
	if err := bridge.Close(); err != nil {
		t.Fatal(err)
	}
	if _, err := bridge.Execute(context.Background(), Request{Method: http.MethodGet, Host: api.host, Path: "/"}); err == nil || !IsErrorCode(err, ErrorClosed) {
		t.Fatalf("closed bridge accepted a request: %v", err)
	}
}

func TestCloseCancelsAndWaitsForActiveRequest(t *testing.T) {
	_, proxyPort := startTestProxy(t)
	started := make(chan struct{})
	api := startTestAPI(t, http.HandlerFunc(func(_ http.ResponseWriter, request *http.Request) {
		close(started)
		<-request.Context().Done()
	}))
	bridge := startTestBridge(t, proxyPort, api, testLimits())
	requestDone := make(chan error, 1)
	go func() {
		_, err := bridge.Execute(context.Background(), Request{Method: http.MethodGet, Host: api.host, Path: "/"})
		requestDone <- err
	}()
	<-started
	closeDone := make(chan error, 1)
	go func() { closeDone <- bridge.Close() }()
	if err := <-requestDone; err == nil || !IsErrorCode(err, ErrorCanceled) {
		t.Fatalf("active request was not canceled by Close: %v", err)
	}
	if err := <-closeDone; err != nil {
		t.Fatal(err)
	}
	if err := bridge.Close(); err != nil {
		t.Fatal(err)
	}
}

package controlplane

import (
	"bytes"
	"context"
	"crypto/rand"
	"crypto/tls"
	"crypto/x509"
	"errors"
	"io"
	"net"
	"net/http"
	"net/http/httptrace"
	"net/url"
	"strconv"
	"strings"
	"sync"
	"sync/atomic"
	"syscall"
	"time"

	box "github.com/sagernet/sing-box"
	M "github.com/sagernet/sing/common/metadata"
)

type bridgeOptions struct {
	targetPort uint16
	rootCAs    *x509.CertPool
}

type outboundDialError struct {
	cause error
}

type outboundDialFunc func(context.Context, string, M.Socksaddr) (net.Conn, error)

type routeCombination struct {
	hostIndex     int
	outboundIndex int
}

func (e *outboundDialError) Error() string {
	return "control plane outbound dial failed"
}

func (e *outboundDialError) Unwrap() error {
	return e.cause
}

type Bridge struct {
	instance        *box.Box
	client          *http.Client
	transport       *http.Transport
	routeClients    []*http.Client
	routeTransports []*http.Transport
	allowedHosts    map[string]struct{}
	hostOrder       []string
	outbounds       []outboundDialFunc
	limits          Limits
	targetPort      uint16
	slots           chan struct{}
	lifecycle       context.Context
	cancel          context.CancelFunc
	access          sync.Mutex
	routing         sync.Mutex
	preferred       routeCombination
	hasPreferred    bool
	failedUntil     map[routeCombination]time.Time
	wait            sync.WaitGroup
	closed          bool
	closeDone       chan struct{}
	closeErr        error
}

func New(ctx context.Context, config Config) (*Bridge, error) {
	return newBridge(ctx, config, bridgeOptions{targetPort: 443})
}

func newBridge(ctx context.Context, config Config, options bridgeOptions) (*Bridge, error) {
	validated, err := validateConfig(config)
	if err != nil {
		return nil, err
	}
	if options.targetPort == 0 {
		return nil, invalidConfig("API port")
	}
	lifecycle, cancelLifecycle := context.WithCancel(context.WithoutCancel(ctx))
	boxOptions, err := buildBoxOptions(lifecycle, validated)
	if err != nil {
		cancelLifecycle()
		return nil, err
	}
	instance, err := box.New(boxOptions)
	if err != nil {
		cancelLifecycle()
		return nil, errorWithCode(ErrorInvalidConfig, err)
	}
	if err = instance.Start(); err != nil {
		cancelLifecycle()
		return nil, errorWithCode(ErrorUnavailable, err)
	}

	allowedHosts := make(map[string]struct{}, len(validated.AllowedHosts))
	for _, host := range validated.AllowedHosts {
		allowedHosts[host] = struct{}{}
	}
	outboundDialers := make([]outboundDialFunc, 0, len(validated.Outbounds))
	for index := range validated.Outbounds {
		outboundDialer, found := instance.Outbound().Outbound(controlPlaneTag(index))
		if !found {
			cancelLifecycle()
			_ = instance.Close()
			return nil, errorWithCode(ErrorUnavailable, errors.New("control plane outbound is unavailable"))
		}
		outboundDialers = append(outboundDialers, outboundDialer.DialContext)
	}

	routeTransports := make([]*http.Transport, 0, len(outboundDialers))
	routeClients := make([]*http.Client, 0, len(outboundDialers))
	for _, outboundDialer := range outboundDialers {
		transport := &http.Transport{
			Proxy:                  nil,
			ForceAttemptHTTP2:      true,
			TLSHandshakeTimeout:    validated.Limits.ConnectTimeout,
			ResponseHeaderTimeout:  validated.Limits.RequestTimeout,
			MaxResponseHeaderBytes: 64 << 10,
			TLSClientConfig: &tls.Config{
				MinVersion: tls.VersionTLS12,
				RootCAs:    options.rootCAs,
			},
		}
		transport.DialContext = func(ctx context.Context, network, address string) (net.Conn, error) {
			host, port, splitErr := splitAddress(address)
			if splitErr != nil || network != "tcp" || port != options.targetPort {
				return nil, errorWithCode(ErrorInvalidRequest, splitErr)
			}
			if _, allowed := allowedHosts[host]; !allowed {
				return nil, errorWithCode(ErrorInvalidRequest, errors.New("dial target is not allowed"))
			}
			connection, dialErr := outboundDialer(ctx, "tcp", M.ParseSocksaddrHostPort(host, port))
			if dialErr != nil {
				return nil, &outboundDialError{cause: dialErr}
			}
			return connection, nil
		}
		routeTransports = append(routeTransports, transport)
		routeClients = append(routeClients, &http.Client{
			Transport: transport,
			CheckRedirect: func(_ *http.Request, _ []*http.Request) error {
				return http.ErrUseLastResponse
			},
		})
	}

	bridge := &Bridge{
		instance:        instance,
		routeClients:    routeClients,
		routeTransports: routeTransports,
		allowedHosts:    allowedHosts,
		hostOrder:       append([]string(nil), validated.AllowedHosts...),
		outbounds:       outboundDialers,
		limits:          validated.Limits,
		targetPort:      options.targetPort,
		slots:           make(chan struct{}, validated.Limits.MaxConcurrent),
		lifecycle:       lifecycle,
		cancel:          cancelLifecycle,
		closeDone:       make(chan struct{}),
		failedUntil:     make(map[routeCombination]time.Time),
	}
	return bridge, nil
}

func (b *Bridge) Execute(ctx context.Context, request Request) (Response, error) {
	defer clear(request.AccessToken)
	b.access.Lock()
	if b.closed {
		b.access.Unlock()
		return Response{}, errorWithCode(ErrorClosed, nil)
	}
	b.wait.Add(1)
	b.access.Unlock()
	defer b.wait.Done()

	method := request.Method
	if method != http.MethodGet && method != http.MethodPost {
		return Response{}, errorWithCode(ErrorInvalidRequest, errors.New("method is not allowed"))
	}
	host := strings.ToLower(request.Host)
	if request.UsePrimary {
		if len(b.hostOrder) == 0 {
			return Response{}, errorWithCode(ErrorUnavailable, errors.New("no API hosts configured"))
		}
		host = b.hostOrder[0]
	}
	if _, allowed := b.allowedHosts[host]; !allowed {
		return Response{}, errorWithCode(ErrorInvalidRequest, errors.New("host is not allowed"))
	}
	parsedPath, err := url.ParseRequestURI(request.Path)
	if err != nil || parsedPath.IsAbs() || parsedPath.Host != "" || !strings.HasPrefix(request.Path, "/") || parsedPath.Fragment != "" {
		return Response{}, errorWithCode(ErrorInvalidRequest, errors.New("path is invalid"))
	}
	if len(request.Path) > 8192 || len(request.ContentType) > 256 || !validHeaderValue(request.ContentType) {
		return Response{}, errorWithCode(ErrorInvalidRequest, errors.New("request metadata is too large"))
	}
	if method == http.MethodGet && len(request.Body) != 0 {
		return Response{}, errorWithCode(ErrorInvalidRequest, errors.New("GET request body is not allowed"))
	}
	if int64(len(request.Body)) > b.limits.MaxRequestBytes {
		return Response{}, errorWithCode(ErrorInvalidRequest, errors.New("request body is too large"))
	}
	if len(request.AccessToken) != 0 && !validBearerToken(request.AccessToken) {
		return Response{}, errorWithCode(ErrorInvalidRequest, errors.New("access token is invalid"))
	}

	requestContext, cancel := context.WithTimeout(ctx, b.limits.RequestTimeout)
	defer cancel()
	stopLifecycleCancel := context.AfterFunc(b.lifecycle, cancel)
	defer stopLifecycleCancel()
	select {
	case b.slots <- struct{}{}:
		defer func() { <-b.slots }()
	case <-requestContext.Done():
		return Response{}, classifyTransportError(requestContext.Err())
	}

	combinations := b.routeCombinations(host, request.UsePrimary)
	maxAttempts := attemptLimit(method, len(combinations), b.limits.MaxAttempts)
	var lastErr error
	for attempt, combination := range combinations[:maxAttempts] {
		if attempt > 0 && !b.waitBackoff(requestContext, attempt) {
			return Response{}, classifyTransportError(requestContext.Err())
		}
		candidateHost := b.hostOrder[combination.hostIndex]
		target := &url.URL{
			Scheme:   "https",
			Host:     net.JoinHostPort(candidateHost, strconv.Itoa(int(b.targetPort))),
			Path:     parsedPath.Path,
			RawPath:  parsedPath.RawPath,
			RawQuery: parsedPath.RawQuery,
		}
		attemptContext := requestContext
		var requestWritten atomic.Bool
		attemptContext = httptrace.WithClientTrace(attemptContext, &httptrace.ClientTrace{
			WroteRequest: func(httptrace.WroteRequestInfo) {
				requestWritten.Store(true)
			},
		})
		httpRequest, err := http.NewRequestWithContext(attemptContext, method, target.String(), bytes.NewReader(request.Body))
		if err != nil {
			return Response{}, errorWithCode(ErrorInvalidRequest, err)
		}
		if request.ContentType != "" {
			httpRequest.Header.Set("Content-Type", request.ContentType)
		}
		if len(request.AccessToken) != 0 {
			httpRequest.Header.Set("Authorization", "Bearer "+string(request.AccessToken))
		}

		httpResponse, err := b.clientForOutbound(combination.outboundIndex).Do(httpRequest)
		if err != nil {
			lastErr = classifyTransportError(err)
			b.markRouteFailure(combination)
			b.closeIdleConnections(combination.outboundIndex)
			if method != http.MethodGet && requestWritten.Load() {
				return Response{}, lastErr
			}
			continue
		}
		body, err := io.ReadAll(io.LimitReader(httpResponse.Body, b.limits.MaxResponseBytes+1))
		httpResponse.Body.Close()
		if err != nil {
			clear(body)
			lastErr = classifyTransportError(err)
			b.markRouteFailure(combination)
			b.closeIdleConnections(combination.outboundIndex)
			if method != http.MethodGet {
				return Response{}, lastErr
			}
			continue
		}
		if int64(len(body)) > b.limits.MaxResponseBytes {
			clear(body)
			return Response{}, errorWithCode(ErrorResponseTooLarge, nil)
		}
		b.markRouteSuccess(combination)
		return Response{
			StatusCode:  httpResponse.StatusCode,
			ContentType: httpResponse.Header.Get("Content-Type"),
			Body:        body,
		}, nil
	}
	if lastErr != nil {
		return Response{}, lastErr
	}
	return Response{}, errorWithCode(ErrorUnavailable, errors.New("API host rotation exhausted"))
}

func (b *Bridge) clientForOutbound(index int) *http.Client {
	if index >= 0 && index < len(b.routeClients) {
		return b.routeClients[index]
	}
	return b.client
}

func (b *Bridge) closeIdleConnections(index int) {
	if index >= 0 && index < len(b.routeTransports) {
		b.routeTransports[index].CloseIdleConnections()
		return
	}
	if b.transport != nil {
		b.transport.CloseIdleConnections()
	}
}

func attemptLimit(method string, available int, configured int) int {
	_ = method
	if configured > available {
		return available
	}
	return configured
}

func (b *Bridge) routeCombinations(host string, usePrimary bool) []routeCombination {
	hostIndexes := make([]int, 0, len(b.hostOrder))
	for index, candidate := range b.hostOrder {
		if usePrimary || candidate == host {
			hostIndexes = append(hostIndexes, index)
		}
	}
	all := make([]routeCombination, 0, len(hostIndexes)*len(b.outbounds))
	for _, outboundIndex := range sequence(len(b.outbounds)) {
		for _, hostIndex := range hostIndexes {
			all = append(all, routeCombination{hostIndex: hostIndex, outboundIndex: outboundIndex})
		}
	}
	b.routing.Lock()
	defer b.routing.Unlock()
	ordered := make([]routeCombination, 0, len(all))
	appendIfUsable := func(combination routeCombination, allowCooling bool) {
		for _, existing := range ordered {
			if existing == combination {
				return
			}
		}
		if !allowCooling && time.Now().Before(b.failedUntil[combination]) {
			return
		}
		ordered = append(ordered, combination)
	}
	if b.hasPreferred {
		for _, combination := range all {
			if combination == b.preferred {
				appendIfUsable(combination, false)
			}
		}
	}
	for _, combination := range all {
		appendIfUsable(combination, false)
	}
	// If every route is cooling down, probe them in deterministic order so a
	// recovered route can re-enter service without waiting for app restart.
	if len(ordered) == 0 {
		for _, combination := range all {
			appendIfUsable(combination, true)
		}
	}
	return ordered
}

func sequence(length int) []int {
	values := make([]int, length)
	for index := range values {
		values[index] = index
	}
	return values
}

func (b *Bridge) markRouteSuccess(combination routeCombination) {
	b.routing.Lock()
	b.preferred = combination
	b.hasPreferred = true
	delete(b.failedUntil, combination)
	b.routing.Unlock()
}

func (b *Bridge) markRouteFailure(combination routeCombination) {
	b.routing.Lock()
	cooldown := b.limits.BackoffBase * 8
	if cooldown > 30*time.Second {
		cooldown = 30 * time.Second
	}
	b.failedUntil[combination] = time.Now().Add(cooldown)
	b.routing.Unlock()
}

func (b *Bridge) waitBackoff(ctx context.Context, attempt int) bool {
	delay := b.limits.BackoffBase
	for index := 1; index < attempt && delay < time.Second; index++ {
		delay *= 2
	}
	if delay > time.Second {
		delay = time.Second
	}
	var random [1]byte
	if _, err := rand.Read(random[:]); err == nil {
		delay = delay/2 + time.Duration(random[0])*delay/255
	}
	timer := time.NewTimer(delay)
	defer timer.Stop()
	select {
	case <-timer.C:
		return true
	case <-ctx.Done():
		return false
	}
}

func (b *Bridge) Close() error {
	b.access.Lock()
	if b.closed {
		done := b.closeDone
		b.access.Unlock()
		<-done
		return b.closeErr
	}
	b.closed = true
	b.cancel()
	b.access.Unlock()
	for _, transport := range b.routeTransports {
		transport.CloseIdleConnections()
	}
	if b.transport != nil {
		b.transport.CloseIdleConnections()
	}
	b.wait.Wait()
	err := b.instance.Close()
	b.access.Lock()
	b.closeErr = err
	close(b.closeDone)
	b.access.Unlock()
	return err
}

func validHeaderValue(value string) bool {
	for _, char := range value {
		if char < 0x20 || char > 0x7e {
			return false
		}
	}
	return true
}

func validBearerToken(value []byte) bool {
	if len(value) == 0 || len(value) > 16<<10 {
		return false
	}
	for _, char := range value {
		if (char < 'a' || char > 'z') && (char < 'A' || char > 'Z') &&
			(char < '0' || char > '9') && !strings.ContainsRune("-._~+/=", rune(char)) {
			return false
		}
	}
	return true
}

func classifyTransportError(err error) error {
	if errors.Is(err, context.Canceled) {
		return errorWithCode(ErrorCanceled, err)
	}
	if errors.Is(err, context.DeadlineExceeded) {
		return errorWithCode(ErrorTimeout, err)
	}
	var dnsError *net.DNSError
	if errors.As(err, &dnsError) {
		return errorWithCode(ErrorDNS, err)
	}
	var certificateError *tls.CertificateVerificationError
	if errors.As(err, &certificateError) {
		return errorWithCode(ErrorTLS, err)
	}
	var authorityError x509.UnknownAuthorityError
	var hostnameError x509.HostnameError
	if errors.As(err, &authorityError) || errors.As(err, &hostnameError) {
		return errorWithCode(ErrorTLS, err)
	}
	var outboundError *outboundDialError
	if errors.As(err, &outboundError) {
		message := strings.ToLower(outboundError.cause.Error())
		if errors.Is(outboundError.cause, io.EOF) || errors.Is(outboundError.cause, io.ErrUnexpectedEOF) ||
			errors.Is(outboundError.cause, syscall.ECONNRESET) || errors.Is(outboundError.cause, syscall.ECONNABORTED) ||
			strings.Contains(message, "reality") || strings.Contains(message, "tls") || strings.Contains(message, "handshake") {
			return errorWithCode(ErrorTLS, err)
		}
	}
	if errors.Is(err, io.EOF) || errors.Is(err, io.ErrUnexpectedEOF) {
		return errorWithCode(ErrorTLS, err)
	}
	return errorWithCode(ErrorUnavailable, err)
}

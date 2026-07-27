package controlplane

import (
	"bytes"
	"context"
	"crypto/tls"
	"crypto/x509"
	"errors"
	"io"
	"net"
	"net/http"
	"net/url"
	"strconv"
	"strings"
	"sync"

	box "github.com/sagernet/sing-box"
	M "github.com/sagernet/sing/common/metadata"
)

type bridgeOptions struct {
	targetPort uint16
	rootCAs    *x509.CertPool
}

type Bridge struct {
	instance     *box.Box
	client       *http.Client
	transport    *http.Transport
	allowedHosts map[string]struct{}
	limits       Limits
	targetPort   uint16
	slots        chan struct{}
	lifecycle    context.Context
	cancel       context.CancelFunc
	access       sync.Mutex
	wait         sync.WaitGroup
	closed       bool
	closeDone    chan struct{}
	closeErr     error
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
	outboundDialer, found := instance.Outbound().Outbound(controlPlaneTag)
	if !found {
		cancelLifecycle()
		_ = instance.Close()
		return nil, errorWithCode(ErrorUnavailable, errors.New("control plane outbound is unavailable"))
	}

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
		return outboundDialer.DialContext(ctx, "tcp", M.ParseSocksaddrHostPort(host, port))
	}

	bridge := &Bridge{
		instance:     instance,
		transport:    transport,
		allowedHosts: allowedHosts,
		limits:       validated.Limits,
		targetPort:   options.targetPort,
		slots:        make(chan struct{}, validated.Limits.MaxConcurrent),
		lifecycle:    lifecycle,
		cancel:       cancelLifecycle,
		closeDone:    make(chan struct{}),
	}
	bridge.client = &http.Client{
		Transport: transport,
		CheckRedirect: func(_ *http.Request, _ []*http.Request) error {
			return http.ErrUseLastResponse
		},
	}
	return bridge, nil
}

func (b *Bridge) Execute(ctx context.Context, request Request) (Response, error) {
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

	target := &url.URL{
		Scheme:   "https",
		Host:     net.JoinHostPort(host, strconv.Itoa(int(b.targetPort))),
		Path:     parsedPath.Path,
		RawPath:  parsedPath.RawPath,
		RawQuery: parsedPath.RawQuery,
	}
	httpRequest, err := http.NewRequestWithContext(requestContext, method, target.String(), bytes.NewReader(request.Body))
	if err != nil {
		return Response{}, errorWithCode(ErrorInvalidRequest, err)
	}
	if request.ContentType != "" {
		httpRequest.Header.Set("Content-Type", request.ContentType)
	}

	httpResponse, err := b.client.Do(httpRequest)
	if err != nil {
		return Response{}, classifyTransportError(err)
	}
	defer httpResponse.Body.Close()
	body, err := io.ReadAll(io.LimitReader(httpResponse.Body, b.limits.MaxResponseBytes+1))
	if err != nil {
		clear(body)
		return Response{}, classifyTransportError(err)
	}
	if int64(len(body)) > b.limits.MaxResponseBytes {
		clear(body)
		return Response{}, errorWithCode(ErrorResponseTooLarge, nil)
	}
	return Response{
		StatusCode:  httpResponse.StatusCode,
		ContentType: httpResponse.Header.Get("Content-Type"),
		Body:        body,
	}, nil
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
	b.transport.CloseIdleConnections()
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
	return errorWithCode(ErrorUnavailable, err)
}

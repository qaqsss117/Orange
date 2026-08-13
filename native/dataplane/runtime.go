package dataplane

import (
	"context"
	"errors"
	"io"
	"net"
	"net/netip"
	"os"
	"sync"
	"sync/atomic"

	box "github.com/sagernet/sing-box"
	"github.com/sagernet/sing-box/adapter"
	"github.com/sagernet/sing-box/adapter/endpoint"
	"github.com/sagernet/sing-box/adapter/inbound"
	"github.com/sagernet/sing-box/adapter/outbound"
	boxservice "github.com/sagernet/sing-box/adapter/service"
	"github.com/sagernet/sing-box/common/urltest"
	"github.com/sagernet/sing-box/constant"
	"github.com/sagernet/sing-box/dns"
	dnsTransport "github.com/sagernet/sing-box/dns/transport"
	"github.com/sagernet/sing-box/dns/transport/local"
	"github.com/sagernet/sing-box/option"
	"github.com/sagernet/sing-box/protocol/direct"
	"github.com/sagernet/sing-box/protocol/group"
	"github.com/sagernet/sing-box/protocol/hysteria2"
	"github.com/sagernet/sing-box/protocol/mixed"
	"github.com/sagernet/sing-box/protocol/shadowsocks"
	"github.com/sagernet/sing-box/protocol/trojan"
	"github.com/sagernet/sing-box/protocol/tun"
	"github.com/sagernet/sing-box/protocol/vless"
	"github.com/sagernet/sing/common/bufio"
	json "github.com/sagernet/sing/common/json"
	N "github.com/sagernet/sing/common/network"
)

const (
	MaxConfigBytes     = 1 << 20
	MaxEventInteger    = 9_007_199_254_740_991
	delayTestTargetURL = "https://cp.cloudflare.com/generate_204"
)

var (
	errInvalidRequest  = errors.New("invalid request")
	errUnknownSelector = errors.New("unknown selector")
	errUnknownNode     = errors.New("unknown node")
	errUnavailable     = errors.New("unavailable")
)

type Controller interface {
	SelectNode(selectorID, nodeID string) error
	ReadSelectedNode(selectorID string) (string, error)
	ProbeDelay(ctx context.Context, selectorID, nodeID string) (uint32, error)
	TrafficTotals() (uint64, uint64, error)
}

type Runtime struct {
	instance  *box.Box
	traffic   *trafficTracker
	selection sync.Mutex
}

func LoadOptions(path string) (context.Context, option.Options, error) {
	if path == "" {
		return nil, option.Options{}, errInvalidRequest
	}
	file, err := os.Open(path)
	if err != nil {
		return nil, option.Options{}, errUnavailable
	}
	defer file.Close()
	content, err := readBounded(file, MaxConfigBytes)
	if err != nil {
		return nil, option.Options{}, err
	}
	defer clear(content)
	ctx := registryContext(context.Background())
	options, err := json.UnmarshalExtendedContext[option.Options](ctx, content)
	if err != nil || !allowedOptions(options) {
		return nil, option.Options{}, errInvalidRequest
	}
	return ctx, options, nil
}

func Check(path string) error {
	ctx, options, err := LoadOptions(path)
	if err != nil {
		return err
	}
	instance, err := box.New(box.Options{Context: ctx, Options: options})
	if err != nil {
		return errInvalidRequest
	}
	_ = instance.Close()
	return nil
}

func Start(path string) (*Runtime, error) {
	ctx, options, err := LoadOptions(path)
	if err != nil {
		return nil, err
	}
	instance, err := box.New(box.Options{Context: ctx, Options: options})
	if err != nil {
		return nil, errInvalidRequest
	}
	tracker := new(trafficTracker)
	instance.Router().AppendTracker(tracker)
	if err := instance.Start(); err != nil {
		_ = instance.Close()
		return nil, errUnavailable
	}
	return &Runtime{instance: instance, traffic: tracker}, nil
}

func (r *Runtime) Close() error {
	if r == nil || r.instance == nil {
		return nil
	}
	return r.instance.Close()
}

func (r *Runtime) SelectNode(selectorID, nodeID string) error {
	if !validPublicID(selectorID) || !validPublicID(nodeID) {
		return errInvalidRequest
	}
	r.selection.Lock()
	defer r.selection.Unlock()
	selector, err := r.selector(selectorID)
	if err != nil {
		return err
	}
	if !contains(selector.All(), nodeID) {
		return errUnknownNode
	}
	if !selector.SelectOutbound(nodeID) || selector.Now() != nodeID {
		return errUnavailable
	}
	return nil
}

func (r *Runtime) ReadSelectedNode(selectorID string) (string, error) {
	if !validPublicID(selectorID) {
		return "", errInvalidRequest
	}
	r.selection.Lock()
	defer r.selection.Unlock()
	selector, err := r.selector(selectorID)
	if err != nil {
		return "", err
	}
	selected := selector.Now()
	if !validPublicID(selected) || !contains(selector.All(), selected) {
		return "", errUnavailable
	}
	return selected, nil
}

func (r *Runtime) ProbeDelay(ctx context.Context, selectorID, nodeID string) (uint32, error) {
	if !validPublicID(selectorID) || !validPublicID(nodeID) {
		return 0, errInvalidRequest
	}
	selector, err := r.selector(selectorID)
	if err != nil {
		return 0, err
	}
	if !contains(selector.All(), nodeID) {
		return 0, errUnknownNode
	}
	node, found := r.instance.Outbound().Outbound(nodeID)
	if !found {
		return 0, errUnavailable
	}
	delay, err := urltest.URLTest(ctx, delayTestTargetURL, node)
	if err != nil || delay == 0 {
		if ctx.Err() != nil {
			return 0, ctx.Err()
		}
		return 0, errUnavailable
	}
	return uint32(delay), nil
}

func (r *Runtime) TrafficTotals() (uint64, uint64, error) {
	upload := r.traffic.upload.Load()
	download := r.traffic.download.Load()
	if upload < 0 || download < 0 || upload > MaxEventInteger || download > MaxEventInteger {
		return 0, 0, errUnavailable
	}
	return uint64(upload), uint64(download), nil
}

func (r *Runtime) selector(selectorID string) (*group.Selector, error) {
	selected, found := r.instance.Outbound().Outbound(selectorID)
	if !found {
		return nil, errUnknownSelector
	}
	selector, ok := selected.(*group.Selector)
	if !ok {
		return nil, errUnknownSelector
	}
	return selector, nil
}

func registryContext(ctx context.Context) context.Context {
	inboundRegistry := inbound.NewRegistry()
	tun.RegisterInbound(inboundRegistry)
	mixed.RegisterInbound(inboundRegistry)
	outboundRegistry := outbound.NewRegistry()
	direct.RegisterOutbound(outboundRegistry)
	shadowsocks.RegisterOutbound(outboundRegistry)
	trojan.RegisterOutbound(outboundRegistry)
	hysteria2.RegisterOutbound(outboundRegistry)
	vless.RegisterOutbound(outboundRegistry)
	group.RegisterSelector(outboundRegistry)
	dnsRegistry := dns.NewTransportRegistry()
	local.RegisterTransport(dnsRegistry)
	dnsTransport.RegisterTLS(dnsRegistry)
	return box.Context(
		ctx,
		inboundRegistry,
		outboundRegistry,
		endpoint.NewRegistry(),
		dnsRegistry,
		boxservice.NewRegistry(),
	)
}

func allowedOptions(options option.Options) bool {
	if options.Experimental != nil || options.NTP != nil || options.Certificate != nil ||
		len(options.Endpoints) != 0 || len(options.Services) != 0 || len(options.Inbounds) != 1 ||
		len(options.Outbounds) == 0 || len(options.Outbounds) > 64 {
		return false
	}
	if options.Log == nil || !options.Log.Disabled || options.Log.Output != "" {
		return false
	}
	switch options.Inbounds[0].Type {
	case constant.TypeTun:
	case constant.TypeMixed:
		if !allowedMixedInbound(options.Inbounds[0].Options) {
			return false
		}
	default:
		return false
	}
	selectors := 0
	for _, configured := range options.Outbounds {
		switch configured.Type {
		case constant.TypeDirect, constant.TypeShadowsocks, constant.TypeTrojan, constant.TypeHysteria2, constant.TypeVLESS:
		case constant.TypeSelector:
			selectors++
		default:
			return false
		}
	}
	return selectors > 0 && selectors <= 8
}

func allowedMixedInbound(value any) bool {
	configured, ok := value.(*option.HTTPMixedInboundOptions)
	if !ok || configured.Listen == nil || configured.ListenPort == 0 ||
		len(configured.Users) != 0 || configured.DomainResolver != nil ||
		configured.SetSystemProxy || configured.TLS != nil {
		return false
	}
	listenAddress := netip.Addr(*configured.Listen)
	if listenAddress != netip.AddrFrom4([4]byte{127, 0, 0, 1}) && listenAddress != netip.IPv6Loopback() {
		return false
	}
	extraListenOptions := configured.ListenOptions
	extraListenOptions.Listen = nil
	extraListenOptions.ListenPort = 0
	return extraListenOptions == (option.ListenOptions{})
}

type trafficTracker struct {
	upload   atomic.Int64
	download atomic.Int64
}

func (t *trafficTracker) RoutedConnection(
	_ context.Context,
	connection net.Conn,
	_ adapter.InboundContext,
	_ adapter.Rule,
	_ adapter.Outbound,
) net.Conn {
	return bufio.NewCounterConn(
		connection,
		[]N.CountFunc{func(count int64) { t.upload.Add(count) }},
		[]N.CountFunc{func(count int64) { t.download.Add(count) }},
	)
}

func (t *trafficTracker) RoutedPacketConnection(
	_ context.Context,
	connection N.PacketConn,
	_ adapter.InboundContext,
	_ adapter.Rule,
	_ adapter.Outbound,
) N.PacketConn {
	return bufio.NewCounterPacketConn(
		connection,
		[]N.CountFunc{func(count int64) { t.upload.Add(count) }},
		[]N.CountFunc{func(count int64) { t.download.Add(count) }},
	)
}

func contains(values []string, expected string) bool {
	for _, value := range values {
		if value == expected {
			return true
		}
	}
	return false
}

func readBounded(reader *os.File, maximum int64) ([]byte, error) {
	info, err := reader.Stat()
	if err != nil || info.Size() <= 0 || info.Size() > maximum {
		return nil, errInvalidRequest
	}
	content := make([]byte, info.Size())
	if _, err := io.ReadFull(reader, content); err != nil {
		clear(content)
		return nil, errUnavailable
	}
	var trailing [1]byte
	if count, err := reader.Read(trailing[:]); count != 0 || !errors.Is(err, io.EOF) {
		clear(content)
		return nil, errInvalidRequest
	}
	return content, nil
}

func validPublicID(value string) bool {
	if value == "" || len(value) > 64 || len(value) >= 7 && value[:7] == "orange-" {
		return false
	}
	for _, character := range []byte(value) {
		if !('a' <= character && character <= 'z') &&
			!('A' <= character && character <= 'Z') &&
			!('0' <= character && character <= '9') &&
			character != '.' && character != '_' && character != '-' {
			return false
		}
	}
	return true
}

var _ Controller = (*Runtime)(nil)
var _ adapter.ConnectionTracker = (*trafficTracker)(nil)

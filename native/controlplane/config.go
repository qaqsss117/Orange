package controlplane

import (
	"context"
	"net"
	"net/netip"
	"strings"
	"time"

	box "github.com/sagernet/sing-box"
	"github.com/sagernet/sing-box/adapter/endpoint"
	"github.com/sagernet/sing-box/adapter/inbound"
	"github.com/sagernet/sing-box/adapter/outbound"
	boxservice "github.com/sagernet/sing-box/adapter/service"
	"github.com/sagernet/sing-box/constant"
	"github.com/sagernet/sing-box/dns"
	"github.com/sagernet/sing-box/dns/transport/local"
	"github.com/sagernet/sing-box/option"
	"github.com/sagernet/sing-box/protocol/hysteria2"
	"github.com/sagernet/sing-box/protocol/shadowsocks"
	"github.com/sagernet/sing-box/protocol/trojan"
	"github.com/sagernet/sing/common/json/badoption"
)

const controlPlaneTag = "orange-control-plane"

var shadowsocksMethods = map[string]struct{}{
	"2022-blake3-aes-128-gcm": {},
	"2022-blake3-aes-256-gcm": {},
	"aes-128-gcm":             {},
	"aes-256-gcm":             {},
	"chacha20-ietf-poly1305":  {},
}

func validateConfig(config Config) (Config, error) {
	if !validHost(config.Outbound.Server) {
		return Config{}, invalidConfig("outbound server")
	}
	if config.Outbound.Port == 0 || config.Outbound.Credential == "" || len(config.Outbound.Credential) > 512 {
		return Config{}, invalidConfig("outbound credentials")
	}

	switch config.Outbound.Protocol {
	case ProtocolShadowsocks:
		if _, found := shadowsocksMethods[config.Outbound.ShadowsocksMethod]; !found || config.Outbound.TLSServerName != "" {
			return Config{}, invalidConfig("shadowsocks options")
		}
	case ProtocolTrojan, ProtocolHysteria2:
		if !validHost(config.Outbound.TLSServerName) || config.Outbound.ShadowsocksMethod != "" {
			return Config{}, invalidConfig("TLS outbound options")
		}
	default:
		return Config{}, invalidConfig("outbound protocol")
	}

	if len(config.AllowedHosts) == 0 || len(config.AllowedHosts) > 16 {
		return Config{}, invalidConfig("API host allowlist")
	}
	seen := make(map[string]struct{}, len(config.AllowedHosts))
	for index, host := range config.AllowedHosts {
		normalized := strings.ToLower(host)
		if !validHost(normalized) {
			return Config{}, invalidConfig("API host allowlist")
		}
		if _, found := seen[normalized]; found {
			return Config{}, invalidConfig("API host allowlist")
		}
		seen[normalized] = struct{}{}
		config.AllowedHosts[index] = normalized
	}

	limits := config.Limits
	if limits == (Limits{}) {
		limits = DefaultLimits()
	}
	if limits.ConnectTimeout < 500*time.Millisecond || limits.ConnectTimeout > 30*time.Second ||
		limits.RequestTimeout < limits.ConnectTimeout || limits.RequestTimeout > 2*time.Minute ||
		limits.MaxConcurrent < 1 || limits.MaxConcurrent > 64 ||
		limits.MaxRequestBytes < 1 || limits.MaxRequestBytes > 8<<20 ||
		limits.MaxResponseBytes < 1 || limits.MaxResponseBytes > 16<<20 {
		return Config{}, invalidConfig("request limits")
	}
	config.Limits = limits
	return config, nil
}

func validHost(value string) bool {
	if value == "" || len(value) > 253 || value != strings.ToLower(value) || !isASCII(value) {
		return false
	}
	if _, err := netip.ParseAddr(value); err == nil {
		return true
	}
	labels := strings.Split(value, ".")
	for _, label := range labels {
		if label == "" || len(label) > 63 || label[0] == '-' || label[len(label)-1] == '-' {
			return false
		}
		for _, char := range label {
			if (char < 'a' || char > 'z') && (char < '0' || char > '9') && char != '-' {
				return false
			}
		}
	}
	return true
}

func isASCII(value string) bool {
	for _, char := range value {
		if char > 127 {
			return false
		}
	}
	return true
}

func registryContext(ctx context.Context) context.Context {
	inboundRegistry := inbound.NewRegistry()
	outboundRegistry := outbound.NewRegistry()
	shadowsocks.RegisterOutbound(outboundRegistry)
	trojan.RegisterOutbound(outboundRegistry)
	hysteria2.RegisterOutbound(outboundRegistry)
	dnsRegistry := dns.NewTransportRegistry()
	local.RegisterTransport(dnsRegistry)
	return box.Context(
		ctx,
		inboundRegistry,
		outboundRegistry,
		endpoint.NewRegistry(),
		dnsRegistry,
		boxservice.NewRegistry(),
	)
}

func buildBoxOptions(ctx context.Context, config Config) (box.Options, error) {
	dialerOptions := option.DialerOptions{
		ConnectTimeout: badoption.Duration(config.Limits.ConnectTimeout),
	}
	serverOptions := option.ServerOptions{
		Server:     config.Outbound.Server,
		ServerPort: config.Outbound.Port,
	}

	var outboundOptions option.Outbound
	outboundOptions.Tag = controlPlaneTag
	switch config.Outbound.Protocol {
	case ProtocolShadowsocks:
		outboundOptions.Type = constant.TypeShadowsocks
		outboundOptions.Options = &option.ShadowsocksOutboundOptions{
			DialerOptions: dialerOptions,
			ServerOptions: serverOptions,
			Method:        config.Outbound.ShadowsocksMethod,
			Password:      config.Outbound.Credential,
			Network:       option.NetworkList("tcp"),
		}
	case ProtocolTrojan:
		outboundOptions.Type = constant.TypeTrojan
		outboundOptions.Options = &option.TrojanOutboundOptions{
			DialerOptions: dialerOptions,
			ServerOptions: serverOptions,
			Password:      config.Outbound.Credential,
			Network:       option.NetworkList("tcp"),
			OutboundTLSOptionsContainer: option.OutboundTLSOptionsContainer{TLS: &option.OutboundTLSOptions{
				Enabled:    true,
				ServerName: config.Outbound.TLSServerName,
				MinVersion: "1.2",
			}},
		}
	case ProtocolHysteria2:
		outboundOptions.Type = constant.TypeHysteria2
		outboundOptions.Options = &option.Hysteria2OutboundOptions{
			DialerOptions: dialerOptions,
			ServerOptions: serverOptions,
			Password:      config.Outbound.Credential,
			Network:       option.NetworkList("tcp"),
			OutboundTLSOptionsContainer: option.OutboundTLSOptionsContainer{TLS: &option.OutboundTLSOptions{
				Enabled:    true,
				ServerName: config.Outbound.TLSServerName,
				MinVersion: "1.2",
			}},
		}
	default:
		return box.Options{}, invalidConfig("outbound protocol")
	}

	return box.Options{
		Context: registryContext(ctx),
		Options: option.Options{
			Log:       &option.LogOptions{Disabled: true},
			Inbounds:  nil,
			Outbounds: []option.Outbound{outboundOptions},
			Route:     &option.RouteOptions{Final: controlPlaneTag},
		},
	}, nil
}

func splitAddress(address string) (string, uint16, error) {
	host, portText, err := net.SplitHostPort(address)
	if err != nil {
		return "", 0, err
	}
	port, err := net.LookupPort("tcp", portText)
	if err != nil || port < 1 || port > 65535 {
		return "", 0, invalidConfig("dial destination")
	}
	return strings.ToLower(host), uint16(port), nil
}

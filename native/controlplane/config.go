package controlplane

import (
	"context"
	"encoding/base64"
	"net"
	"net/netip"
	"strconv"
	"strings"
	"time"

	box "github.com/sagernet/sing-box"
	"github.com/sagernet/sing-box/adapter/endpoint"
	"github.com/sagernet/sing-box/adapter/inbound"
	"github.com/sagernet/sing-box/adapter/outbound"
	boxservice "github.com/sagernet/sing-box/adapter/service"
	"github.com/sagernet/sing-box/constant"
	"github.com/sagernet/sing-box/dns"
	dnsTransport "github.com/sagernet/sing-box/dns/transport"
	"github.com/sagernet/sing-box/dns/transport/local"
	"github.com/sagernet/sing-box/option"
	"github.com/sagernet/sing-box/protocol/hysteria2"
	"github.com/sagernet/sing-box/protocol/shadowsocks"
	"github.com/sagernet/sing-box/protocol/trojan"
	"github.com/sagernet/sing-box/protocol/vless"
	"github.com/sagernet/sing/common/json/badoption"
)

const (
	controlPlaneTag     = "orange-control-plane"
	startupDNSTagPrefix = "orange-startup-dns-"
	startupDNSSystemTag = "orange-startup-dns-system"
)

var shadowsocksMethods = map[string]struct{}{
	"2022-blake3-aes-128-gcm": {},
	"2022-blake3-aes-256-gcm": {},
	"aes-128-gcm":             {},
	"aes-256-gcm":             {},
	"chacha20-ietf-poly1305":  {},
}

func validateConfig(config Config) (Config, error) {
	config.Outbound.Server = strings.ToLower(config.Outbound.Server)
	config.Outbound.TLSServerName = strings.ToLower(config.Outbound.TLSServerName)
	if !validHost(config.Outbound.Server) {
		return Config{}, invalidConfig("outbound server")
	}
	if config.Outbound.Port == 0 || config.Outbound.Credential == "" || len(config.Outbound.Credential) > 512 {
		return Config{}, invalidConfig("outbound credentials")
	}

	switch config.Outbound.Protocol {
	case ProtocolShadowsocks:
		if _, found := shadowsocksMethods[config.Outbound.ShadowsocksMethod]; !found || config.Outbound.TLSServerName != "" || hasVLESSOptions(config.Outbound) {
			return Config{}, invalidConfig("shadowsocks options")
		}
	case ProtocolTrojan, ProtocolHysteria2:
		if !validHost(config.Outbound.TLSServerName) || config.Outbound.ShadowsocksMethod != "" || hasVLESSOptions(config.Outbound) {
			return Config{}, invalidConfig("TLS outbound options")
		}
	case ProtocolVLESS:
		if !validHost(config.Outbound.TLSServerName) || config.Outbound.ShadowsocksMethod != "" ||
			!validUUID(config.Outbound.Credential) || !validRealityPublicKey(config.Outbound.RealityPublicKey) ||
			!validRealityShortID(config.Outbound.RealityShortID) || config.Outbound.ClientFingerprint != "chrome" ||
			config.Outbound.VLESSFlow != "xtls-rprx-vision" {
			return Config{}, invalidConfig("VLESS Reality options")
		}
	default:
		return Config{}, invalidConfig("outbound protocol")
	}

	if len(config.StartupDNS) == 0 || len(config.StartupDNS) > 4 {
		return Config{}, invalidConfig("startup DNS")
	}
	for index := range config.StartupDNS {
		server := &config.StartupDNS[index]
		server.Server = strings.ToLower(server.Server)
		server.TLSServerName = strings.ToLower(server.TLSServerName)
		if !validHost(server.Server) || server.Port == 0 {
			return Config{}, invalidConfig("startup DNS")
		}
		switch server.Protocol {
		case DNSProtocolTLS:
			if !validHost(server.TLSServerName) {
				return Config{}, invalidConfig("startup DNS TLS options")
			}
		case DNSProtocolUDP, DNSProtocolTCP:
			if server.TLSServerName != "" {
				return Config{}, invalidConfig("startup DNS options")
			}
		default:
			return Config{}, invalidConfig("startup DNS protocol")
		}
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

func hasVLESSOptions(config OutboundConfig) bool {
	return config.RealityPublicKey != "" || config.RealityShortID != "" ||
		config.ClientFingerprint != "" || config.VLESSFlow != ""
}

func validUUID(value string) bool {
	if len(value) != 36 {
		return false
	}
	for index, char := range []byte(value) {
		if index == 8 || index == 13 || index == 18 || index == 23 {
			if char != '-' {
				return false
			}
			continue
		}
		if !((char >= '0' && char <= '9') || (char >= 'a' && char <= 'f') || (char >= 'A' && char <= 'F')) {
			return false
		}
	}
	return true
}

func validRealityPublicKey(value string) bool {
	decoded, err := base64.RawURLEncoding.Strict().DecodeString(value)
	return err == nil && len(decoded) == 32
}

func validRealityShortID(value string) bool {
	if value == "" {
		return true
	}
	if len(value) < 2 || len(value) > 16 || len(value)%2 != 0 {
		return false
	}
	for _, char := range []byte(value) {
		if !((char >= '0' && char <= '9') || (char >= 'a' && char <= 'f') || (char >= 'A' && char <= 'F')) {
			return false
		}
	}
	return true
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
	vless.RegisterOutbound(outboundRegistry)
	dnsRegistry := dns.NewTransportRegistry()
	local.RegisterTransport(dnsRegistry)
	dnsTransport.RegisterUDP(dnsRegistry)
	dnsTransport.RegisterTCP(dnsRegistry)
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

func buildBoxOptions(ctx context.Context, config Config) (box.Options, error) {
	dialerOptions := option.DialerOptions{
		ConnectTimeout: badoption.Duration(config.Limits.ConnectTimeout),
		DomainResolver: &option.DomainResolveOptions{
			Server: startupDNSTag(0),
		},
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
	case ProtocolVLESS:
		outboundOptions.Type = constant.TypeVLESS
		outboundOptions.Options = &option.VLESSOutboundOptions{
			DialerOptions: dialerOptions,
			ServerOptions: serverOptions,
			UUID:          config.Outbound.Credential,
			Flow:          config.Outbound.VLESSFlow,
			Network:       option.NetworkList("tcp"),
			OutboundTLSOptionsContainer: option.OutboundTLSOptionsContainer{TLS: &option.OutboundTLSOptions{
				Enabled:    true,
				ServerName: config.Outbound.TLSServerName,
				MinVersion: "1.2",
				Insecure:   false,
				UTLS: &option.OutboundUTLSOptions{
					Enabled:     true,
					Fingerprint: config.Outbound.ClientFingerprint,
				},
				Reality: &option.OutboundRealityOptions{
					Enabled:   true,
					PublicKey: config.Outbound.RealityPublicKey,
					ShortID:   config.Outbound.RealityShortID,
				},
			}},
		}
	default:
		return box.Options{}, invalidConfig("outbound protocol")
	}

	return box.Options{
		Context: registryContext(ctx),
		Options: option.Options{
			Log:       &option.LogOptions{Disabled: true},
			DNS:       buildDNSOptions(config),
			Inbounds:  nil,
			Outbounds: []option.Outbound{outboundOptions},
			Route:     &option.RouteOptions{Final: controlPlaneTag},
		},
	}, nil
}

func startupDNSTag(index int) string {
	return startupDNSTagPrefix + strconv.Itoa(index)
}

func buildDNSOptions(config Config) *option.DNSOptions {
	resolverTag := ""
	for index, server := range config.StartupDNS {
		if _, err := netip.ParseAddr(server.Server); err == nil {
			resolverTag = startupDNSTag(index)
			break
		}
	}

	servers := make([]option.DNSServerOptions, 0, len(config.StartupDNS)+1)
	if resolverTag == "" {
		resolverTag = startupDNSSystemTag
		servers = append(servers, option.DNSServerOptions{
			Type: constant.DNSTypeLocal,
			Tag:  startupDNSSystemTag,
			Options: &option.LocalDNSServerOptions{
				PreferGo: true,
			},
		})
	}

	for index, server := range config.StartupDNS {
		remote := option.RemoteDNSServerOptions{
			RawLocalDNSServerOptions: option.RawLocalDNSServerOptions{
				DialerOptions: option.DialerOptions{
					ConnectTimeout: badoption.Duration(config.Limits.ConnectTimeout),
				},
			},
			DNSServerAddressOptions: option.DNSServerAddressOptions{
				Server:     server.Server,
				ServerPort: server.Port,
			},
		}
		if _, err := netip.ParseAddr(server.Server); err != nil {
			remote.DomainResolver = &option.DomainResolveOptions{Server: resolverTag}
		}

		entry := option.DNSServerOptions{Tag: startupDNSTag(index)}
		switch server.Protocol {
		case DNSProtocolUDP:
			entry.Type = constant.DNSTypeUDP
			entry.Options = &remote
		case DNSProtocolTCP:
			entry.Type = constant.DNSTypeTCP
			entry.Options = &remote
		case DNSProtocolTLS:
			entry.Type = constant.DNSTypeTLS
			entry.Options = &option.RemoteTLSDNSServerOptions{
				RemoteDNSServerOptions: remote,
				OutboundTLSOptionsContainer: option.OutboundTLSOptionsContainer{
					TLS: &option.OutboundTLSOptions{
						Enabled:    true,
						ServerName: server.TLSServerName,
						MinVersion: "1.2",
					},
				},
			}
		}
		servers = append(servers, entry)
	}

	return &option.DNSOptions{RawDNSOptions: option.RawDNSOptions{
		Servers: servers,
		Final:   startupDNSTag(0),
	}}
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

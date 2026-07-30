package controlplane

import (
	"context"
	"os"
	"path/filepath"
	"testing"

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
	"github.com/sagernet/sing-box/protocol/group"
	"github.com/sagernet/sing-box/protocol/hysteria2"
	"github.com/sagernet/sing-box/protocol/shadowsocks"
	"github.com/sagernet/sing-box/protocol/trojan"
	"github.com/sagernet/sing-box/protocol/tun"
	json "github.com/sagernet/sing/common/json"
)

func dataPlaneRegistryContext() context.Context {
	inboundRegistry := inbound.NewRegistry()
	tun.RegisterInbound(inboundRegistry)
	outboundRegistry := outbound.NewRegistry()
	shadowsocks.RegisterOutbound(outboundRegistry)
	trojan.RegisterOutbound(outboundRegistry)
	hysteria2.RegisterOutbound(outboundRegistry)
	group.RegisterSelector(outboundRegistry)
	dnsRegistry := dns.NewTransportRegistry()
	local.RegisterTransport(dnsRegistry)
	dnsTransport.RegisterTLS(dnsRegistry)
	return box.Context(
		context.Background(),
		inboundRegistry,
		outboundRegistry,
		endpoint.NewRegistry(),
		dnsRegistry,
		boxservice.NewRegistry(),
	)
}

func TestSanitizedDataPlaneFixtureMatchesPinnedSingBox(t *testing.T) {
	fixturePath := filepath.Join("..", "..", "contracts", "data-plane", "fixtures", "sanitized-sing-box.v1.json")
	content, err := os.ReadFile(fixturePath)
	if err != nil {
		t.Fatal(err)
	}
	var options option.Options
	if err = json.UnmarshalContextDisallowUnknownFields(dataPlaneRegistryContext(), content, &options); err != nil {
		t.Fatalf("sanitized fixture is not valid sing-box configuration: %v", err)
	}

	if options.Log == nil || !options.Log.Disabled {
		t.Fatal("data plane logging is not fixed off")
	}
	if len(options.Inbounds) != 1 || options.Inbounds[0].Type != constant.TypeTun {
		t.Fatalf("unexpected data plane inbound set: %#v", options.Inbounds)
	}
	tunOptions := options.Inbounds[0].Options.(*option.TunInboundOptions)
	if tunOptions.InterfaceName != "orange-tun" || !tunOptions.AutoRoute || !tunOptions.StrictRoute || len(tunOptions.Address) != 2 {
		t.Fatalf("unexpected fixed TUN options: %#v", tunOptions)
	}

	expectedOutboundTypes := []string{
		constant.TypeShadowsocks,
		constant.TypeTrojan,
		constant.TypeHysteria2,
		constant.TypeSelector,
	}
	if len(options.Outbounds) != len(expectedOutboundTypes) {
		t.Fatalf("unexpected outbound count: %d", len(options.Outbounds))
	}
	for index, expectedType := range expectedOutboundTypes {
		if options.Outbounds[index].Type != expectedType {
			t.Fatalf("outbound %d type is %q, expected %q", index, options.Outbounds[index].Type, expectedType)
		}
	}
	if options.DNS == nil || options.DNS.Final != "orange-dot-dns" || len(options.DNS.Servers) != 1 || options.DNS.Servers[0].Type != constant.DNSTypeTLS {
		t.Fatalf("unexpected fixed DNS options: %#v", options.DNS)
	}
	dnsOptions := options.DNS.Servers[0].Options.(*option.RemoteTLSDNSServerOptions)
	if dnsOptions.Server != "223.5.5.5" || dnsOptions.ServerPort != 853 || dnsOptions.TLS == nil || !dnsOptions.TLS.Enabled || dnsOptions.TLS.ServerName != "dns.alidns.com" || dnsOptions.TLS.Insecure || dnsOptions.TLS.MinVersion != "1.2" {
		t.Fatalf("unexpected fixed DNS transport: %#v", dnsOptions)
	}
	if options.Route == nil || options.Route.Final != "proxy" || len(options.Route.Rules) != 5 {
		t.Fatalf("unexpected fixed route options: %#v", options.Route)
	}
	expectedRouteActions := []struct {
		action   string
		outbound string
	}{
		{action: "sniff"},
		{action: "hijack-dns"},
		{action: "route", outbound: "proxy"},
		{action: "route", outbound: "node-hk"},
		{action: "route", outbound: "proxy"},
	}
	for index, expected := range expectedRouteActions {
		action := options.Route.Rules[index].DefaultOptions.RuleAction
		if action.Action != expected.action || action.RouteOptions.Outbound != expected.outbound {
			t.Fatalf("route %d action is %#v, expected action=%q outbound=%q", index, action, expected.action, expected.outbound)
		}
	}
}

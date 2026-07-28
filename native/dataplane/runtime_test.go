package dataplane

import (
	"bytes"
	"context"
	"os"
	"path/filepath"
	"testing"

	box "github.com/sagernet/sing-box"
	"github.com/sagernet/sing-box/option"
	json "github.com/sagernet/sing/common/json"
)

func TestSanitizedFixtureUsesOnlyRegisteredCapabilities(t *testing.T) {
	fixture := filepath.Join("..", "..", "contracts", "data-plane", "fixtures", "sanitized-sing-box.v1.json")
	content, err := os.ReadFile(fixture)
	if err != nil {
		t.Fatal(err)
	}
	content = bytes.ReplaceAll(content, []byte("<redacted:ss-password>"), []byte("MDEyMzQ1Njc4OWFiY2RlZjAxMjM0NTY3ODlhYmNkZWY="))
	content = bytes.ReplaceAll(content, []byte("<redacted:trojan-password>"), []byte("test-password"))
	content = bytes.ReplaceAll(content, []byte("<redacted:hysteria2-password>"), []byte("test-password"))
	path := filepath.Join(t.TempDir(), "sanitized.json")
	if err := os.WriteFile(path, content, 0o600); err != nil {
		t.Fatal(err)
	}
	options, err := json.UnmarshalExtendedContext[option.Options](registryContext(context.Background()), content)
	if err != nil {
		t.Fatalf("sanitized fixture parse failed: %v", err)
	}
	if !allowedOptions(options) {
		t.Fatal("sanitized fixture exceeds the Orange capability policy")
	}
	instance, err := box.New(box.Options{Context: registryContext(context.Background()), Options: options})
	if err != nil {
		t.Fatalf("sanitized fixture box construction failed: %v", err)
	}
	_ = instance.Close()
	if err := Check(path); err != nil {
		t.Fatalf("sanitized fixture failed: %v", err)
	}
}

func TestReviewedVLESSRealityOptionsUseTheRegisteredRuntime(t *testing.T) {
	content := []byte(`{
		"log":{"disabled":true},
		"inbounds":[{"type":"mixed","listen":"127.0.0.1","listen_port":19090}],
		"outbounds":[
			{"type":"vless","tag":"node-a","server":"8.8.8.8","server_port":443,"uuid":"01234567-89ab-cdef-0123-456789abcdef","flow":"xtls-rprx-vision","network":"tcp","tls":{"enabled":true,"server_name":"www.example.com","insecure":false,"min_version":"1.2","utls":{"enabled":true,"fingerprint":"chrome"},"reality":{"enabled":true,"public_key":"BwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwc"}}},
			{"type":"selector","tag":"proxy","outbounds":["node-a"],"default":"node-a"}
		],
		"route":{"final":"proxy","auto_detect_interface":true}
	}`)
	options, err := json.UnmarshalExtendedContext[option.Options](registryContext(context.Background()), content)
	if err != nil {
		t.Fatalf("reviewed VLESS Reality config parse failed: %v", err)
	}
	if !allowedOptions(options) {
		t.Fatal("reviewed VLESS Reality config exceeded the Orange capability policy")
	}
	instance, err := box.New(box.Options{Context: registryContext(context.Background()), Options: options})
	if err != nil {
		t.Fatalf("reviewed VLESS Reality box construction failed: %v", err)
	}
	_ = instance.Close()
}

func TestConfigurationPolicyRejectsExperimentalAndUnregisteredCapabilities(t *testing.T) {
	for _, content := range []string{
		`{"log":{"disabled":true},"experimental":{"clash_api":{"external_controller":"127.0.0.1:9090"}},"inbounds":[{"type":"mixed","listen":"127.0.0.1","listen_port":19090}],"outbounds":[{"type":"direct","tag":"node-a"},{"type":"selector","tag":"proxy","outbounds":["node-a"]}],"route":{"final":"proxy"}}`,
		`{"log":{"disabled":true},"inbounds":[{"type":"socks","listen":"127.0.0.1","listen_port":19090}],"outbounds":[{"type":"direct","tag":"node-a"},{"type":"selector","tag":"proxy","outbounds":["node-a"]}],"route":{"final":"proxy"}}`,
	} {
		path := filepath.Join(t.TempDir(), "config.json")
		if err := os.WriteFile(path, []byte(content), 0o600); err != nil {
			t.Fatal(err)
		}
		if err := Check(path); err == nil {
			t.Fatal("unsafe configuration was accepted")
		}
	}
}

func TestMixedInboundPolicyIsLoopbackOnly(t *testing.T) {
	base := `,"outbounds":[{"type":"direct","tag":"node-a"},{"type":"selector","tag":"proxy","outbounds":["node-a"]}],"route":{"final":"proxy"}}`
	for _, inbound := range []string{
		`{"type":"mixed","listen":"127.0.0.1","listen_port":19090}`,
		`{"type":"mixed","listen":"::1","listen_port":19090}`,
	} {
		content := []byte(`{"log":{"disabled":true},"inbounds":[` + inbound + `]` + base)
		options, err := json.UnmarshalExtendedContext[option.Options](registryContext(context.Background()), content)
		if err != nil || !allowedOptions(options) {
			t.Fatalf("safe mixed inbound was rejected: %s (%v)", inbound, err)
		}
	}
	for _, inbound := range []string{
		`{"type":"mixed","listen":"0.0.0.0","listen_port":19090}`,
		`{"type":"mixed","listen":"127.0.0.2","listen_port":19090}`,
		`{"type":"mixed","listen":"127.0.0.1"}`,
		`{"type":"mixed","listen":"127.0.0.1","listen_port":19090,"users":[{"username":"operator","password":"secret"}]}`,
		`{"type":"mixed","listen":"127.0.0.1","listen_port":19090,"set_system_proxy":true}`,
		`{"type":"mixed","listen":"127.0.0.1","listen_port":19090,"tls":{"enabled":true}}`,
		`{"type":"mixed","listen":"127.0.0.1","listen_port":19090,"bind_interface":"Ethernet"}`,
	} {
		content := []byte(`{"log":{"disabled":true},"inbounds":[` + inbound + `]` + base)
		options, err := json.UnmarshalExtendedContext[option.Options](registryContext(context.Background()), content)
		if err == nil && allowedOptions(options) {
			t.Fatalf("unsafe mixed inbound was accepted: %s", inbound)
		}
	}
}

func TestPublicIdentifiersMatchRustBoundary(t *testing.T) {
	for _, value := range []string{"proxy", "node-a", "A_1.example"} {
		if !validPublicID(value) {
			t.Fatalf("valid identifier rejected: %s", value)
		}
	}
	for _, value := range []string{"", "orange-private", "node/a", string(make([]byte, 65))} {
		if validPublicID(value) {
			t.Fatalf("invalid identifier accepted: %q", value)
		}
	}
}

package controlplane

import (
	"context"
	"fmt"
	"net/http"
	"os"
	"os/exec"
	"reflect"
	"sort"
	"strings"
	"testing"
	"time"
)

func processListeners(pid int) ([]string, error) {
	command := fmt.Sprintf(
		"$tcp = Get-NetTCPConnection -OwningProcess %d -State Listen -ErrorAction SilentlyContinue | ForEach-Object { 'tcp|{0}|{1}' -f $_.LocalAddress,$_.LocalPort }; "+
			"$udp = Get-NetUDPEndpoint -OwningProcess %d -ErrorAction SilentlyContinue | ForEach-Object { 'udp|{0}|{1}' -f $_.LocalAddress,$_.LocalPort }; "+
			"@($tcp) + @($udp)",
		pid,
		pid,
	)
	output, err := exec.Command("powershell.exe", "-NoProfile", "-NonInteractive", "-Command", command).Output()
	if err != nil {
		return nil, err
	}
	var listeners []string
	for _, line := range strings.Split(strings.TrimSpace(string(output)), "\n") {
		if value := strings.TrimSpace(line); value != "" {
			listeners = append(listeners, value)
		}
	}
	sort.Strings(listeners)
	return listeners, nil
}

func TestControlPlaneAddsNoTCPOrUDPListener(t *testing.T) {
	_, proxyPort := startTestProxy(t)
	api := startTestAPI(t, http.HandlerFunc(func(writer http.ResponseWriter, _ *http.Request) {
		writer.WriteHeader(http.StatusOK)
	}))
	before, err := processListeners(os.Getpid())
	if err != nil {
		t.Skipf("Windows listener audit is unavailable: %v", err)
	}
	bridge := startTestBridge(t, proxyPort, api, testLimits())
	if _, err = bridge.Execute(context.Background(), Request{Method: http.MethodGet, Host: api.host, Path: "/"}); err != nil {
		t.Fatal(err)
	}
	time.Sleep(100 * time.Millisecond)
	after, err := processListeners(os.Getpid())
	if err != nil {
		t.Fatal(err)
	}
	if !reflect.DeepEqual(before, after) {
		t.Fatalf("Control Plane listener set changed: before=%v after=%v", before, after)
	}
}

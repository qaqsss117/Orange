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

func lsofEndpoints(pid int, arguments ...string) ([]string, error) {
	base := []string{"-nP", "-a", "-p", fmt.Sprint(pid)}
	output, err := exec.Command("lsof", append(base, arguments...)...).Output()
	if exitError, isExitError := err.(*exec.ExitError); isExitError && len(exitError.Stderr) == 0 {
		return nil, nil
	}
	if err != nil {
		return nil, err
	}
	var endpoints []string
	for _, line := range strings.Split(string(output), "\n") {
		if strings.HasPrefix(line, "n") {
			endpoints = append(endpoints, strings.TrimSpace(line[1:]))
		}
	}
	return endpoints, nil
}

func processListeners(pid int) ([]string, error) {
	tcp, err := lsofEndpoints(pid, "-iTCP", "-sTCP:LISTEN", "-Fn")
	if err != nil {
		return nil, err
	}
	udp, err := lsofEndpoints(pid, "-iUDP", "-Fn")
	if err != nil {
		return nil, err
	}
	listeners := append(tcp, udp...)
	sort.Strings(listeners)
	return listeners, nil
}

func TestControlPlaneAddsNoTCPOrUDPListener(t *testing.T) {
	if _, err := exec.LookPath("lsof"); err != nil {
		t.Skip("lsof is required for the macOS listener audit")
	}
	_, proxyPort := startTestProxy(t)
	api := startTestAPI(t, http.HandlerFunc(func(writer http.ResponseWriter, _ *http.Request) {
		writer.WriteHeader(http.StatusOK)
	}))
	before, err := processListeners(os.Getpid())
	if err != nil {
		t.Skipf("macOS listener audit is unavailable: %v", err)
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

package controlplane

import (
	"bufio"
	"context"
	"fmt"
	"net/http"
	"os"
	"path/filepath"
	"reflect"
	"sort"
	"strings"
	"testing"
	"time"
)

func processSocketInodes(pid int) (map[string]struct{}, error) {
	entries, err := os.ReadDir(fmt.Sprintf("/proc/%d/fd", pid))
	if err != nil {
		return nil, err
	}
	inodes := make(map[string]struct{})
	for _, entry := range entries {
		target, readErr := os.Readlink(filepath.Join(fmt.Sprintf("/proc/%d/fd", pid), entry.Name()))
		if readErr != nil || !strings.HasPrefix(target, "socket:[") || !strings.HasSuffix(target, "]") {
			continue
		}
		inodes[strings.TrimSuffix(strings.TrimPrefix(target, "socket:["), "]")] = struct{}{}
	}
	return inodes, nil
}

func processListeners(pid int) ([]string, error) {
	inodes, err := processSocketInodes(pid)
	if err != nil {
		return nil, err
	}
	var listeners []string
	for _, network := range []struct {
		name string
		path string
		tcp  bool
	}{
		{name: "tcp4", path: "/proc/net/tcp", tcp: true},
		{name: "tcp6", path: "/proc/net/tcp6", tcp: true},
		{name: "udp4", path: "/proc/net/udp"},
		{name: "udp6", path: "/proc/net/udp6"},
	} {
		file, openErr := os.Open(network.path)
		if openErr != nil {
			return nil, openErr
		}
		scanner := bufio.NewScanner(file)
		for scanner.Scan() {
			fields := strings.Fields(scanner.Text())
			if len(fields) < 10 || fields[0] == "sl" {
				continue
			}
			if network.tcp && fields[3] != "0A" {
				continue
			}
			if _, owned := inodes[fields[9]]; owned {
				listeners = append(listeners, network.name+"|"+fields[1])
			}
		}
		scanErr := scanner.Err()
		closeErr := file.Close()
		if scanErr != nil {
			return nil, scanErr
		}
		if closeErr != nil {
			return nil, closeErr
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
		t.Skipf("Linux listener audit is unavailable: %v", err)
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

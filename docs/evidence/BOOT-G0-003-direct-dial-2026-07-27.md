# BOOT-G0-003 No-Listener Direct-Dial Evidence

- Date: 2026-07-27
- Host: Windows 11 `10.0.26200`, amd64
- Slice status: `in_progress`
- sing-box: `github.com/sagernet/sing-box v1.13.14`

## Implemented Boundary

- `native/controlplane` creates exactly one Shadowsocks, Trojan, or Hysteria2 outbound and fixes `route.final` to that tag. The Control Plane option set contains no inbound, TUN, mixed, HTTP, SOCKS, redirect, system proxy, or direct fallback.
- `http.Transport.DialContext` calls the selected sing-box outbound directly. The HTTP boundary accepts only structured GET/POST fields, allowlisted API hosts, path, content type, and bounded body; it never accepts a full URL, Authorization header, file path, or shell value.
- HTTPS remains fixed to port 443 in the production constructor, verifies the system trust store, requires TLS 1.2 or newer, disables redirects, and bounds connect/total timeout, request/response size, response headers, and concurrency.
- Bootstrap `startupDns` records now cross the stdio `init` boundary and become strict sing-box UDP, TCP, or DNS-over-TLS transports. The first record resolves proxy hostnames; an integration fixture proves a named proxy is reached only after querying the configured DNS server.
- DNS transports dial directly to avoid proxy bootstrap recursion. A DNS server hostname uses the first IP-addressed startup DNS record for its own bootstrap; only an all-hostname DNS list permits the system resolver, and then solely for DNS server hostnames.
- `cmd/orange-control-plane` exposes a versioned, 2 MiB length-prefixed stdio protocol with only `init`, `request`, and `cancel` input frames. Unknown fields, duplicate active IDs, invalid IDs, short frames, oversized frames, and post-close requests are rejected with redacted stable error codes.
- Closing the bridge cancels and waits for active requests before releasing the sing-box instance. Credential and request/response byte buffers owned by the stdio boundary are cleared after handoff/use; process exit releases the native sing-box copy.

## Direct-Dial And Fail-Closed Tests

The deterministic integration tests start a real sing-box Shadowsocks test server and a local TLS API fixture. The client Control Plane has no inbound and sends GET/POST through the encrypted outbound.

```text
go test -count=1 ./...
18 top-level tests discovered; 17 passed and one live test skipped by default
```

Covered paths include:

- HTTPS GET query and JSON POST through the real Shadowsocks outbound;
- strict startup DNS protocol/TLS validation and a real UDP DNS lookup for a domain-based proxy endpoint;
- proxy port blocked while the API remains reachable, producing `bootstrap-unavailable` with zero API hits;
- valid TLS plus unknown-authority rejection, DNS failure, request timeout, caller cancellation, response cap, request cap, and two-request concurrency cap;
- close/request synchronization, strict request metadata, stdio framing, short writes, short reads, and redacted protocol errors.

An explicitly enabled live PoC reached the overseas `postman-echo.com:443` test API through the same Shadowsocks outbound. GET and JSON POST both returned HTTP 200 and echoed the non-sensitive probe value.

```text
ORANGE_RUN_LIVE_CONTROL_PLANE=1 go test -count=1 -run ^TestLiveDirectDialGETAndPOST$ -v .
PASS
```

## Listener Audit

`TestControlPlaneAddsNoTCPOrUDPListener` snapshots process-owned listeners before creating the client and after a successful proxied HTTPS request.

- Windows uses `Get-NetTCPConnection` and `Get-NetUDPEndpoint`; before/after sets were identical.
- Linux reads process-owned socket inodes and `/proc/net/{tcp,tcp6,udp,udp6}`.
- macOS uses `lsof` TCP LISTEN and UDP snapshots.
- Linux and macOS test binaries cross-compiled successfully from the Windows host; their runtime audits still require native CI runners.

## Artifact And Supply Chain

`python scripts/ci/check_control_plane.py` builds the sidecar with `-trimpath`, verifies its embedded module metadata, runs the route/listener checks, and scans for test-only bootstrap data.

| Artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `artifacts/controlplane/orange-control-plane.exe` | 21,833,216 | `dd1f468346aeab0aeadbd73b0816fcc20ed88e5246ee55f7e82d9a282e991f05` |

- Embedded sing-box module/version: `github.com/sagernet/sing-box v1.13.14`.
- Test password, `postman-echo.com`, and `.invalid` API host tokens were absent from the production executable.
- Go checks now require `go mod verify`, no `replace` directives, pinned sing-box, `gofmt`, `go vet`, and tests.
- The SBOM contains 36 Go modules used by compiled packages. Go module hashes and detected licenses are included; sing-box and eight SagerNet components are recorded as `GPL-3.0-or-later`.

## Full Gates

`python scripts/ci/run.py quality` passed all 16 steps:

- source isolation over 224 files and 28 security unit tests;
- Prettier, ESLint, 6 Vitest tests, TypeScript, and Vite build;
- Rust formatting, Clippy with warnings denied, 24 workspace tests, and workspace build;
- bootstrap crypto, memory leak, and Control Plane direct-dial audits;
- Go verify/vet/tests;
- 726-component CycloneDX SBOM, 53 resources, license validation, and 7-ecosystem supply-chain validation.

## Remaining Acceptance Work

The slice remains `in_progress`; the following claims are not yet made:

- `pktmon` capture could not start because the current Windows process lacks elevated capture access. No pcap/ETL evidence is registered yet.
- Linux/macOS native listener audits and Android/iOS socket audits have not run on their target systems.
- A real approved bootstrap proxy and production API host have not been tested; production nodes and credentials still wait for Gitee secret injection.
- The Rust desktop/mobile host does not yet spawn or embed the stdio sidecar, so decrypted `SecretBuffer` handoff and native-process lifetime still need an end-to-end host test.

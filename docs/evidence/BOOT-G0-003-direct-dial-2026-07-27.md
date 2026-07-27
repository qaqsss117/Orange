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
- `orange-control-plane-host` implements the desktop process boundary. It accepts only absolute, canonicalizable sidecar paths, launches without a shell or inherited environment, hides the Windows console, and does not expose production sidecar arguments. The bundled constructor resolves only the fixed application-sibling filename and rejects a SHA-256 mismatch before spawn. Android/iOS builds do not compile this desktop host.
- Tauri holds at most one `Arc<ControlPlaneHost>` in managed state and exposes start, execute, status, and stop operations without placing bootstrap plaintext in WebView state. The expected sidecar SHA-256 is instantiated in managed state so it remains present in the final application binary.

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

## Rust Host Lifecycle

Seven real child-process tests plus a production-sidecar handoff audit cover:

- `ready` handshake success, initialization rejection, missing sidecar, startup timeout, and exit after readiness;
- concurrent request-ID dispatch, explicit cancellation, request timeout cancellation, and cancellation when a pending request is dropped;
- pending-request failure broadcast when the host closes and preservation of the first stable protocol/exit error code;
- EOF graceful shutdown plus timeout-bounded kill and wait for a stuck child;
- production Go sidecar `SecretBuffer -> init -> ready -> EOF` handoff, with the Rust secret observably cleared immediately after frame construction.

Production builds cannot supply an arbitrary sidecar path or arguments; the arbitrary-path constructor and argument builder exist only behind the crate's `test-helper` feature.

## Desktop External Binary Registration

Windows, Linux, and macOS platform configs register exactly `../artifacts/tauri-sidecars/orange-control-plane` through Tauri `externalBin`; their development and build hooks generate only the current target-triple artifact from `native/controlplane` in the isolated build-artifact tree. The base/mobile configuration contains no external binary, so Android/iOS remain on their embedded-native path.

`python scripts/ci/check_control_plane_bundle.py` runs a real `pnpm tauri build --debug --no-bundle` and proves that Tauri strips the target suffix, places the sidecar beside `orange-app`, copies it byte-for-byte, and embeds the same SHA-256 in the application. The copied executable is also recorded through the standard build-artifact manifest with source, version, platform, GPL-3.0-or-later license, hash, and signature state.

```text
target: x86_64-pc-windows-msvc
source: artifacts/tauri-sidecars/orange-control-plane-x86_64-pc-windows-msvc.exe
runtime: target/debug/orange-control-plane.exe
bytes: 21833216
sha256: dd1f468346aeab0aeadbd73b0816fcc20ed88e5246ee55f7e82d9a282e991f05
integrity hash embedded: true
signature: unsigned-debug
release allowed: false
```

Target-aware Go preparation also cross-built and audited `x86_64-unknown-linux-gnu` (22,666,331 bytes, SHA-256 `864d44fa56e6595bd30758390f97a6f0c4a2dfb63dd219a454b1f55fdd113330`) and `aarch64-apple-darwin` (20,786,930 bytes, SHA-256 `ef5daeb7a5e9f6d98fcda9d8f5c45a6142d406246647d8d4048d3708c039d5bf`). These are cross-build results, not native runtime claims.

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

`python scripts/ci/run.py quality` passed all 19 steps:

- source isolation over 241 files (71 text files) and 28 security unit tests;
- Prettier, ESLint, 6 Vitest tests, TypeScript, and Vite build;
- target-aware desktop sidecar preparation, Rust formatting, Clippy with warnings denied, 28 default-feature workspace tests, 7 real host process tests, and workspace build;
- bootstrap crypto, memory leak, Control Plane direct-dial, Rust host, and Tauri bundle/integrity audits;
- Go verify/vet/tests;
- 727-component CycloneDX SBOM, 53 resources, license validation, and 7-ecosystem supply-chain validation.

## Remaining Acceptance Work

The slice remains `in_progress`; the following claims are not yet made:

- `pktmon` capture could not start because the current Windows process lacks elevated capture access. No pcap/ETL evidence is registered yet.
- Linux/macOS native listener audits and Android/iOS socket audits have not run on their target systems.
- A real approved bootstrap proxy and production API host have not been tested; production nodes and credentials still wait for Gitee secret injection.
- A signed Windows/macOS/Linux installer has not been produced or audited. Debug sidecars remain explicitly `unsigned-debug` and non-releaseable until product identifiers and platform signing identities are approved.
- Android/iOS still require their embedded native host implementation and on-device socket/lifecycle audits.

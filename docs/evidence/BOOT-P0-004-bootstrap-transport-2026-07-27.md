# BOOT-P0-004 BootstrapTransport Forced-Routing Evidence

- Date: 2026-07-27
- Hosts: Windows 11 amd64, Ubuntu 24.04.4 under WSL2, and Android 16 / API 36
- Slice status: `in_progress`

## Qualification Scope

This increment establishes a shared Rust business-command client and connects
the desktop implementation to the existing no-listener Control Plane. It does
not expose a business command to the WebView and does not claim that a
production API or mobile Control Plane transport exists.

No dependency, Tauri invoke command, WebView capability, direct network client,
runtime log sink, or platform permission was added. The existing invoke handler
remains exactly `get_plane_state` and `get_runtime_info`.

## Fixed Route Contract

`orange-platform` defines exactly ten `BusinessCommand` values. Each command
owns its method, development host, path, authentication mode, and content type;
the caller cannot supply or override any of those fields.

| Command | Method | Path | Authentication |
| --- | --- | --- | --- |
| `login` | POST | `/v1/development/auth/login` | none |
| `register` | POST | `/v1/development/auth/register` | none |
| `config` | GET | `/v1/development/config` | none |
| `subscription` | GET | `/v1/development/subscription` | Rust token |
| `account` | GET | `/v1/development/account` | Rust token |
| `plans` | GET | `/v1/development/plans` | none |
| `orders` | POST | `/v1/development/orders` | Rust token |
| `invite` | GET | `/v1/development/invite` | Rust token |
| `tickets` | POST | `/v1/development/tickets` | Rust token |
| `update` | GET | `/v1/development/update` | none |

All routes use fixed host `api.orange.invalid`, HTTPS port 443, and schema
version 1. That host is deliberately non-resolving, marked
`production_hosts_configured: false`, and cannot be released. The route fixture
under `contracts/control-plane/fixtures`, `security/control-endpoints.yml`, and
the Rust catalog are compared field by field in tests. This prevents an endpoint
policy edit from silently diverging from executable routing.

The transport policy denies redirects, fixes request attempts to one, and caps
request and response bodies at 1 MiB. The client invokes its one injected
`BootstrapTransport` exactly once and converts every 3xx response into
`business-redirect-denied`; there is no redirect hop or direct fallback.

## Rust Client And Secret Boundary

`BusinessCommandRequest` constructs only a bodyless GET or serialized JSON POST
that matches the fixed route. Request, transport-response, and business-response
bodies zeroize on drop. Their `Debug` implementations report only command,
route, status, authentication presence, and byte counts; body and token bytes
are never formatted.

For the five authenticated routes, `BusinessCommandClient` loads only
`SecretKey::AccessToken` from the injected platform `SecretStoreBackend` inside
Rust. A missing token returns `business-authentication-required` before the
transport is called. The borrowed token exists only for the transport call and
remains owned by the existing zeroizing `SecretValue`.

Response construction rejects invalid status codes, control characters,
content types above 256 bytes, and bodies above 1 MiB. HTTP and transport
failures map only to the stable redacted codes recorded in
`bootstrap-transport-errors.v1.json`.

## Desktop And Native Handoff

The desktop Tauri shell creates one `Arc<ManagedControlPlane>` and uses that
same instance for both managed Control Plane state and the single managed
`BusinessCommandClient<Arc<ManagedControlPlane>, DesktopSecretStore>`. The
adapter alone converts a fixed `BusinessRoute` into `ControlPlaneRequest`; raw
request construction elsewhere in production code is rejected by the security
gate.

An authenticated `ControlPlaneRequest` accepts one optional byte token. Rust
rejects empty, non-Bearer-safe, control-character, or greater-than-16-KiB values
before writing the frame. `Debug` exposes only `authenticated: true/false`.
Version 1 stdio request frames serialize the token as the narrow optional
Base64 `accessToken` field, not an arbitrary header map.

The Go bridge validates the same character and size limits, constructs
`Authorization: Bearer <token>` only inside the native HTTP boundary, and
clears both the request token and stdio frame token buffers after use. Tests
prove correct header injection, CR/LF rejection, caller-buffer clearing, and an
authenticated request through the real Rust process-test sidecar. The protocol
still cannot accept a full URL, caller-supplied Authorization header, local
path, shell command, listener, or direct route.

The desktop adapter has an explicit test for all 18 `HostErrorCode` values and
all three local managed-state errors. Invalid requests, invalid responses,
timeouts, cancellation, DNS, TLS, and response-size failures preserve their
stable transport category; all remaining lifecycle failures collapse to
`transport-unavailable` without leaking native details.

## Static And Focused Verification

```text
cargo test --package orange-platform --package orange-control-plane-host \
  --package orange-app --features orange-control-plane-host/test-helper
orange-platform: 46 passed
orange-control-plane-host unit: 3 passed
orange-control-plane-host process: 7 passed
orange-app: 5 passed

cargo clippy --package orange-platform --package orange-control-plane-host \
  --package orange-app --all-targets \
  --features orange-control-plane-host/test-helper -- -D warnings
passed

go test ./...
passed for the bridge and stdio helper packages

python -m unittest scripts.security.tests.test_control_egress -v
10 passed

python scripts/security/check_control_egress.py
10 commands; 10 business routes; 39 production/runtime sources;
0 runtime log sinks
```

The egress gate now cross-checks the route fixture against the endpoint policy,
requires the fixed ten-command catalog and Rust secure-store load, verifies the
versioned token handoff and Go-only Bearer construction, checks Rust/Go token
clearing, and requires exactly one managed desktop business client. It rejects
WebView URL/host/route/token/Authorization fields, a second HTTP client, and raw
`ControlPlaneRequest::get/post` construction outside the managed adapter.

## Windows And Linux Gates

Windows `python scripts/ci/run.py quality` passed all 21 steps with 298 source
files and 104 text files scanned, 45 security tests, 10 frontend tests, 81 Rust
workspace tests, 7 separate Control Plane host process tests, a 784-component
SBOM, and 53 registered resources. The Control Plane audit covered six
direct-dial tests; the bundle audit verified the 21,835,776-byte sidecar with
SHA-256
`86e1f2e62d0bc3ca9aac8dfdbc8654f24d63715b16bba813de3a442b281c5878`.

The four-step Windows desktop-shell task passed. `target/debug/orange-app.exe`
was 12,735,488 bytes with SHA-256
`87b8f08e937d9d39a4bf6f3d7e27e26eafaba511970ea864aced997a849550af`,
remained alive for an eight-second native startup window, and left no new
Control Plane sidecar after shutdown.

An isolated Ubuntu 24.04.4 WSL2 copy at
`/tmp/orange-boot-p0-004.bcPTgD` excluded `.git`, `.ci-tools`, `artifacts`,
`dist`, `node_modules`, `target`, and `src-tauri/gen`. With explicit pinned
Node, Go, Rust, Python, and system-tool paths, its quality task passed all 21
steps with 282 source files and 104 text files scanned, 45 security tests, 10
frontend tests, 81 passing Rust workspace tests, one explicitly isolated native
secret-store test ignored, a 790-component SBOM, and 53 resources.

The Linux Control Plane bundle audit verified its 22,666,517-byte sidecar with
SHA-256
`dd2a6d3954b59d8477e59f83f873ff5eb0ac5359c62eaea4c9d344d7525662e0`.
The quality-built application was 203,377,576 bytes. The four-step desktop-shell
task and full eight-second Xvfb/D-Bus startup window passed. A full-command-line
process check found no remaining sidecar; the exact isolated workspace was
removed and independently confirmed absent.

## Android Gate

`python scripts/ci/run.py android-shell` regenerated the controlled Android
project and passed all eight steps: four Rust target installations, aarch64
Rust/Tauri build, exact merged-permission audit, Android lint, instrumentation
assembly, and debug-artifact recording. A separate x86_64 build then compiled
the changed shared platform crate for the running emulator.

The final APK permission snapshot contains only `INTERNET`, the app-private
dynamic-receiver permission, the `DUMP`-guarded profile receiver, and implied
faketouch. It has no FileProvider or privacy permission.

Android 16 / API 36 on x86_64 installed and launched the current
124,755,623-byte application APK with SHA-256
`866b51fa000a560dd72823d5bcb69a7563e7967fefa5056a71580153dcce6418`.
The 625,024-byte instrumentation APK had SHA-256
`3d252e98529ca133b77b026bcd7af6dc7215fff181a5583a7847d145ac9790ec`.
The device reported `OK (4 tests)` for the current Rust/Kotlin/Keystore bridge,
and an independent package query confirmed zero matching debug packages after
cleanup.

This does not claim an Android BootstrapTransport. Android and iOS intentionally
do not manage the desktop process host; their embedded Control Plane transport
remains future platform work.

## Remaining Acceptance Work

This slice remains `in_progress`, not `review` or `done`:

- the approved production API host, paths, DTO contract, and desensitized
  fixtures are not configured;
- no typed login, registration, account, subscription, order, ticket, or other
  business Tauri command is exposed yet;
- Android and iOS do not have an embedded Control Plane transport or runtime
  forced-routing proof;
- macOS desktop runtime, privileged packet capture, production proxy/API, and
  signed installer evidence are outstanding; and
- formal dependencies `BOOT-G0-003`, `SEC-G0-003`, and `ARC-G0-002` are not
  complete.

Those gaps are product and platform work, not silently substituted by a direct
HTTP fallback.

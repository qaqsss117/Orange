# VPN-G0-001 Data Plane Configuration Evidence

- Date: 2026-07-28
- Hosts: Windows 11 amd64, Ubuntu 24.04.4 under WSL2, and Android 16 / API 36
- sing-box: `github.com/sagernet/sing-box v1.13.14`
- Slice status: `in_progress`

2026-07-30 follow-up: the sanitized configuration was exercised by the
installed Windows development package. The current DNS and route controls
below supersede the original local-resolver-only fixture, and the live result
is recorded in `WIN-P1-005-windows-development-acceptance-2026-07-30.md`.

## Qualification Scope

This increment establishes a bounded development input and sanitizer for the
future user Data Plane. It does not start a Data Plane instance, fetch a
subscription, change a route or DNS setting, or claim compatibility with an
unseen production subscription. The only fixtures use reserved `.invalid`
hosts and explicit `<redacted:...>` credentials.

No Clash YAML parser, Clash API client, mihomo component, WebView command,
network client, listener, platform permission, or local-file capability was
added.

## Closed Input Contract

`sing-box-subscription.schema.v1.json` is the only accepted v1 input. It is
pinned to sing-box `1.13.14`, closes every object with
`additionalProperties: false`, and permits only:

- Shadowsocks with five allowlisted methods;
- Trojan and Hysteria2 with required verified TLS;
- selector with a bounded node list and explicit default; and
- bounded domain-suffix, CIDR, and DNS/HTTP/TLS/QUIC route matches.

The contract cannot express inbounds, DNS transports, logs, listeners,
control APIs, services, executable hooks, paths, remote rule sets, arbitrary
route actions, or client-reserved `orange-*` tags. Nodes, selectors, rules,
match values, tag bytes, credential bytes, and total input bytes have fixed
limits.

The source fixture covers all four allowed outbound types and all three match
classes. The sanitized fixture is a separately regenerated sing-box document,
not a copy of the subscription.

## Rust Sanitizer Boundary

`orange-platform` accepts the source bytes as `Zeroizing<Vec<u8>>`. Strict,
path-aware serde decoding produces a closed wire DTO, which is consumed into a
separate normalized model. Validation normalizes hostnames, TLS names,
protocols, and CIDR networks; rejects local/special IP servers, bad methods,
unsafe TLS, duplicate/reserved tags, dangling selector or route references,
cross-protocol fields, empty rules, and resource-limit violations.

The rendered document always supplies the client-owned controls:

| Control | Fixed value |
| --- | --- |
| Logging | disabled |
| Inbound | one `orange-tun` TUN with fixed IPv4/IPv6 addresses |
| DNS | one fixed-IP `orange-dot-dns` DoT resolver with verified TLS identity |
| TLS | enabled, verified, minimum `1.2` |
| Selector | interrupt existing connections on selection change |
| Route | fixed sniff, DNS hijack, closed subscription routes, fixed final reference, auto interface detection |

Wire credentials, normalized credentials, and rendered JSON use zeroizing
owners. The result's `Debug` output reports only byte and object counts and the
consumer can explicitly clear the JSON buffer. All errors expose only a stable
code and structural field path; malformed input tests prove the credential is
absent from `Debug` and `Display` output.

Nine focused Rust tests cover exact regeneration, forbidden top-level
capabilities, local servers, methods, TLS, selector/route closure, resource
limits, precise field paths, normalization, redacted debug output, explicit
clearing, duplicate tags, and reserved tags.

## Pinned sing-box Compatibility

`native/controlplane/data_plane_config_test.go` constructs registries for TUN,
local/TLS DNS, Shadowsocks, Trojan, Hysteria2, and selector. The test passes the
sanitized fixture to sing-box `1.13.14` through
`UnmarshalContextDisallowUnknownFields`, then verifies the decoded inbound,
DNS, outbound sequence, and route. `go mod verify`, `gofmt`, `go vet`, and the
complete Go test suite passed on Windows and Linux.

This is a parse/contract compatibility proof. It intentionally does not open a
TUN interface or dial the non-routable fixture nodes.

## Static And Artifact Audits

`check_data_plane_config.py` validates the schema closure, protocol registry,
source fixture, normalized fixture relationship, Rust safety markers,
toolchain/Cargo/Go pins, and absence of Clash/mihomo dependency markers. Five
fault-injection tests cover open schema objects, protocol drift, unsafe fixture
fields, fixed-template drift, label-only leak reporting, and Clash/mihomo
artifact markers.

The build phase derives 18 forbidden application tokens from the fixture and
core denylist. It scans the final desktop `orange-app` without writing any raw
token into the report. Both final Windows and Linux application scans reported
zero leaks. The standard SBOM and supply-chain gates also passed.

## Windows Gate

`python scripts/ci/run.py quality` passed all 24 steps: 330 source files and
123 production text files were scanned, 58 security tests and 20 frontend tests
passed, all 118 Rust workspace tests passed, Go verification passed, and the
785-component/53-resource SBOM passed.

The four-step desktop-shell task passed. The final artifacts were:

| Artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `target/debug/orange-app.exe` | 16,719,872 | `0ed1f892120e92e1d2ae6ff76f4feecb4567e6d28d04f54b8dc59dd0468ea2f9` |
| Windows Control Plane sidecar | 21,835,776 | `86e1f2e62d0bc3ca9aac8dfdbc8654f24d63715b16bba813de3a442b281c5878` |

The application remained alive for an eight-second native startup window.
After stopping that exact process, no new Control Plane sidecar remained.

## Linux Gate

The final source was copied without `.git`, `.ci-tools`, `artifacts`, `dist`,
`node_modules`, `target`, or `src-tauri/gen` into the isolated Ubuntu workspace
`/home/dev/orange-linux-smoke-20260728.cXa3iq`. Its 24-step quality task passed
with 335 source files, 123 production text files, 58 security tests, 20
frontend tests, 118 passing Rust tests, and one explicitly unavailable native
secret-store test ignored. Go and SBOM/supply-chain checks also passed.

The four-step desktop-shell task passed. The final artifacts were:

| Artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `target/debug/orange-app` | 215,202,296 | `a8d1a0dc324e3421f3cfb379e7d1142ecaa4a682b84ec8f7e37d37e0028e52fb` |
| Linux Control Plane sidecar | 22,666,517 | `dd2a6d3954b59d847e59f83f873ff5eb0ac5359c62eaea4c9d344d7525662e0` |

The desktop command stayed alive for the full eight-second Xvfb/D-Bus window.
No application or sidecar remained afterward. The exact isolated workspace
was removed and independently confirmed absent.

## Android Gate

`python scripts/ci/run.py android-shell` passed all eight steps, including
controlled project regeneration, four Rust target installations, current
aarch64 Rust/Tauri compilation, merged-permission audit, Android lint,
instrumentation assembly, and artifact recording. A subsequent current-source
x86_64 build recompiled `serde_path_to_error`, `orange-platform`, and
`orange-app` for the connected emulator.

The merged APK retained only `INTERNET`, the app-private dynamic-receiver
permission, the `DUMP`-guarded profile receiver, and implied faketouch. It has
no FileProvider or privacy permission. Final device artifacts were:

| Artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| x86_64 application APK | 126,259,247 | `a53a498f363331413efad80e5227fa80b6c543810b8a8c55acf662176e673d97` |
| instrumentation APK | 625,024 | `3d252e98529ca133b77b026bcd7af6dc7215fff181a5583a7847d145ac9790ec` |

The only connected device reported Android 16, API 36, and x86_64. It
installed and launched the current application, completed the real
Rust/Kotlin/Keystore bridge receipt, and reported `OK (4 tests)`. Both debug
packages were removed, and an independent package query returned no match.

## Remaining Acceptance Work

The slice remains `in_progress`, not `review` or `done`:

- no approved production sing-box subscription or desensitized backend sample
  has been reconciled with the bounded v1 contract;
- the installed Windows development package has exercised the sanitizer output
  in mixed and TUN modes, but signed release and other desktop/mobile platform
  lifecycle evidence remain outstanding;
- macOS and iOS build/runtime evidence is unavailable; and
- formal dependency `ARC-G0-002` has not reached its required final state.

The development fixture and strict parse proof do not substitute for those
inputs. Any production field expansion must update the closed schema, internal
model, sanitizer tests, pinned sing-box compatibility test, and artifact audit
before it can be accepted.

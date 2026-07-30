# WIN-G0-001 Windows Data Plane Core Evidence (2026-07-28)

## Scope and Result

This increment fixes the Windows hosting decision to one production path:
an Authenticode-signed official `sing-box.exe` sidecar supervised by
`orange-service.exe`. The alternative of compiling sing-box into the service
is rejected and is not retained as a second implementation.

The local PoC and three-platform regression passed. `WIN-G0-001` remains
`in_progress`: the artifact is intentionally unsigned, no production signing
certificate or approved signer thumbprint exists, the native service-side
`WinVerifyTrust` handshake is not wired, and a formal Windows 10 22H2 plus
Windows 11 compatibility matrix is unavailable.

## Decision and Build Boundary

`docs/adr/0001-windows-data-plane-sidecar.md` records the decision, rejected
alternatives, privilege boundary, pre-spawn handshake, and signing order.

`native/dataplane` is an independent artifact-build module. It pins:

- package: `github.com/sagernet/sing-box/cmd/sing-box`;
- module/version: `github.com/sagernet/sing-box v1.13.14`;
- target: `windows/amd64`;
- exact Go toolchain: `1.25.5`;
- CGO: disabled;
- only feature tag: `with_quic`.

`with_quic` is required by the approved Hysteria2 outbound. The policy rejects
the upstream default tags for Clash API, Tailscale, WireGuard, ACME, DHCP,
gVisor, uTLS, CCM, OCM, naive outbound, and V2Ray API. The binary metadata
contained 48 compiled dependency modules and none of the explicitly forbidden
Anthropic/OpenAI, Cronet, Tailscale, wireguard-go, or WireGuard Windows modules.

The separate 320-line `native/dataplane/go.sum` is included alongside the
Control Plane lock in the supply-chain serial. SBOM generation executes the Go
dependency query with the policy's `GOOS=windows`, `GOARCH=amd64`,
`CGO_ENABLED=0`, and `with_quic` values, so the report contains the actual
target build graph rather than every optional module present in the upstream
checksum graph. The final SBOM contained 798 components. Supply-chain
validation covered 913 locked dependency names across seven ecosystems and
reported no error.

## Artifact and Handshake

`python scripts/ci/run.py windows-data-plane` performed two clean output builds
with `-mod=readonly`, `-trimpath`, no VCS metadata, an empty Go build ID, the
official version link value, and the pinned feature tag. Both outputs were
byte-identical:

| Artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| Windows `sing-box.exe` | 23,680,512 | `a1a1a1a4d96ee234f3ebe4f21c7a1f3c3920536e0bb231c95a6d4e8bf4d3ec3d` |

The executable reported:

- `sing-box version 1.13.14`;
- `windows/amd64`;
- `Tags: with_quic`;
- `CGO: disabled`.

The audit validated the executable's Go build metadata, SHA-256, and
Authenticode state before executing its fixed `version` command, then rechecked
the digest before running the smoke test. The standard
build-artifact manifest was written to
`artifacts/security/windows-data-plane-artifacts.json` and independently
validated against `security/build-artifact-manifest.schema.json`.

`Get-AuthenticodeSignature` reported `NotSigned`, which is the only expected
development state. The manifest therefore records `unsigned-debug` and
`release_allowed: false`. Release mode refuses to overwrite an existing signed
artifact and accepts only `Valid` plus an exact allowlisted certificate SHA-1
thumbprint. Invalid status, unknown signer, malformed digest, digest mismatch,
version mismatch, target/tag drift, or enabled CGO all fail closed.

Production does not use PowerShell. The ADR requires the future service adapter
to verify the fixed sibling path, signed-build manifest digest, native
`WinVerifyTrust` chain, allowlisted signer thumbprint, and exact version
handshake before spawning only `run -c` with its service-owned configuration.

## Offline Mixed Smoke

The audit started a temporary HTTP server on `127.0.0.1`, removed inherited
proxy environment variables, and generated a configuration containing only:

- one mixed inbound on a dynamically reserved `127.0.0.1` port;
- one direct outbound;
- a final route to that outbound;
- no DNS server, remote endpoint, rule-set, API, or service.

It reached the same loopback HTTP fixture once through mixed HTTP proxy syntax
and once through SOCKS5 negotiation. Both responses matched the fixed body.
The child remained active throughout the smoke test, terminated within the
normal five-second cleanup bound without a forced kill, was reaped, and the
same listener address could be rebound. A post-run process audit found no
`sing-box`, `orange-app`, or Control Plane process.

The six new fault tests cover unsigned-development classification, digest
mismatch, version mismatch, invalid Authenticode state, unsigned release
rejection, and exact allowlisted-signer release eligibility.

## Privilege Boundary

- `orange.exe` remains an unelevated user process and does not install services,
  load drivers, or accept a Data Plane executable/argument from the WebView.
- An explicitly elevated installer owns service, sidecar, and any later Wintun
  installation. Runtime download or replacement of executable code is denied.
- `orange-service.exe` is the sole privileged coordination boundary. The
  service now uses the fixed service SID/Named Pipe DTO boundary and fixed
  sidecar/revision paths; installer-enforced token privileges and filesystem
  ACL evidence remain outstanding.
- The service may only resolve the installed sibling `sing-box.exe`, clear
  unnecessary environment, and use a service-owned fixed configuration path.
  It cannot accept shell, arbitrary paths, URLs, registry paths, raw sing-box
  commands, or raw upstream configuration over IPC.
- Mixed listeners are loopback-only and must be registered in runtime state.
  TUN/route/DNS privileges and restoration remain owned by the Windows adapter,
  never by an elevated UI.

The subsequent `WIN-P0-002` increment wired an embedded strict runtime
manifest, native `WinVerifyTrust`, signer-certificate SHA-1 extraction,
SHA-256/version/config rechecks, fixed `run -c`, and kill-on-close Job Object
into the SCM service. See
`docs/evidence/WIN-P0-002-windows-service-ipc-2026-07-28.md`. The empty signer
allowlist and unsigned development sidecar still fail release preflight.

## Regression Evidence

### Windows

The final host API reported `Windows 10 Pro`, version `2009`, build `26200`,
64-bit. Because that product label/build combination does not constitute a
clean release matrix, it is recorded only as the development host and is not
used to claim either required Windows 10 22H2 or current Windows 11 coverage.

`python scripts/ci/run.py quality` passed all 26 steps, including 70 security
tests, 20 frontend tests, formatting/lint, Rust workspace lint/tests/build,
Control Plane audits, both Go modules, 798-component SBOM/license validation,
supply-chain validation, and the final Windows Data Plane audit.

`python scripts/ci/run.py desktop-shell` passed all four steps. The debug app
remained active for eight seconds and stopped without leaving its Control Plane
sidecar.

| Artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| Windows app | 16,719,872 | `5ff52ac4218e709a4697b7accc6158e45b87f608c7ca60c5313a1a5c1efa6135` |
| Windows Control Plane sidecar | 21,835,776 | `86e1f2e62d0bc3ca9aac8dfdbc8654f24d63715b16bba813de3a442b281c5878` |
| Windows Data Plane sidecar | 23,680,512 | `a1a1a1a4d96ee234f3ebe4f21c7a1f3c3920536e0bb231c95a6d4e8bf4d3ec3d` |

### Ubuntu 24.04.4 WSL2

Source was copied without `.git`, `.ci-tools`, `artifacts`, `dist`,
`node_modules`, `target`, generated Android output, or Python bytecode.
`python3 scripts/ci/run.py quality` passed all 25 Linux steps. The static and
unit gates validated the Windows policy, both Go locks, and target-specific
SBOM without attempting to execute a Windows PE file.

The four-step desktop-shell job then passed in an independent persistent
isolated copy. `dbus-run-session` plus Xvfb kept the app alive for eight
seconds; timeout returned the expected `124`. EGL/PipeWire/portal warnings were
non-fatal in the headless WSL2 session. No app or Control Plane process
remained, and the guarded isolated directory was deleted and confirmed absent.

| Artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| Linux app | 215,204,984 | `b398cd38c46ac7124ac59b92cc3f951be1ae0079d27a651a4a9ca614a47e59a0` |
| Linux Control Plane sidecar | 22,666,517 | `dd2a6d3954b59d8477e59f83f873ff5eb0ac5359c62eaea4c9d344d7525662e0` |

### Android 16 / API 36

`python scripts/ci/run.py android-shell` passed all eight steps: controlled
generation, four Rust targets, aarch64 Tauri build, merged permission audit,
Android lint, instrumentation assembly, and artifact recording. No new mobile
service, permission, native sidecar, or Data Plane capability was introduced.

The connected x86_64 emulator then received a current-source x86_64 build. The
application launched with the debug-only bridge test request, produced the real
Rust-to-Kotlin receipt, and AndroidJUnitRunner reported `OK (4 tests)` with
`INSTRUMENTATION_CODE: -1`. Secure and bridge preferences were both empty
`<map />` values afterward. Both debug packages were uninstalled and confirmed
absent.

| Artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| x86_64 application APK | 126,263,575 | `e7a1937fc80745ba57e9fc76537effbbe8abe61cd70beb9e5a814dbaccdce456` |
| instrumentation APK | 625,024 | `3d252e98529ca133b77b026bcd7af6dc7215fff181a5583a7847d145ac9790ec` |

## Remaining Acceptance Work

2026-07-30 follow-up: the fixed-path native service handshake, protected
installation, service SID/token and Named Pipe boundaries, upgrade rollback,
uninstall, route/DNS/proxy restoration, and real mixed/TUN behavior all passed
on Windows 10 22H2. The implementation is complete and `WIN-G0-001` is now
`review`.

It cannot become `done` until acceptance rules 3 and 5 are complete:

- obtain the production code-signing certificate and approve its exact signer
  thumbprint;
- sign the app, service, and sidecar, generate their post-signing digest
  manifests, and prove the release-mode audit; and
- run the signed package on a current stable Windows 11 host (and retain the
  existing Windows 10 result in the signed matrix).

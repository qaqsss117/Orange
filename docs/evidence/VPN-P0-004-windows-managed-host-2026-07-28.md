# VPN-P0-004 Windows Managed Data Plane Host Evidence

Date: 2026-07-28
Host: Windows amd64, Go 1.25.5, sing-box 1.13.14
Status: development baseline passed; slice remains `in_progress`

## Scope

This increment replaces the upstream sing-box CLI host selected by ADR-0001 with the single
Orange-managed host defined by ADR-0002. `orange-data-plane.exe` composes the pinned public
sing-box API; it does not patch or fork the sing-box core and does not enable Clash or V2Ray APIs.

The host registers only TUN/mixed inbounds, direct/Shadowsocks/Trojan/Hysteria2/selector
outbounds, and local DNS. It retains the fixed `check -c`, `run -c`, and `version` service
commands. The running process accepts a 4 KiB length-prefixed stdio protocol with these commands:

- `select_node` with immediate sing-box `Now()` readback;
- `read_selected_node`;
- `probe_delay` against the pinned sing-box default HTTPS 204 target;
- correlated `cancel_probe`; and
- `traffic` with TCP/UDP upload and download totals from a router tracker.

There is no network control listener. Requests cannot contain a URL, configuration path,
credential, raw sing-box object, arbitrary command, or connection metadata. Public IDs match the
Rust 64-byte allowlist, `orange-*` is reserved, request IDs are strictly increasing, duplicate JSON
fields and trailing values fail closed, probes are limited to 8 concurrent operations and
100-60000 ms, and zero traffic totals remain explicit.

## Runtime And Service Boundary

The build policy now names `orange-data-plane.exe`, records the inherited-stdio protocol and the
exact registry, and continues to forbid all previously rejected build tags. The Windows runtime
manifest pins the new filename and post-build SHA-256. Existing canonical path, pre/post handshake
hash, `WinVerifyTrust`, signer allowlist, exact version/platform/tag/CGO, protected revision,
Job Object, TUN readiness, and cleanup checks remain in place.

`orange-service` now keeps the child's stdin open for the complete `run` lifetime so the managed
host does not treat startup as control EOF. The Rust protocol client intentionally remains unwired;
the service policy records `rust_client_wired: false` rather than claiming a production node
backend.

## Tests

- Windows `quality`: 34/34 steps passed in the final run.
- Security mutation/unit suite: 124 tests passed.
- Managed Data Plane Go suite: 11 tests passed with `with_quic`; repository Go verify, vet, and
  tests passed for both native modules.
- Windows service: 29 Rust tests and 8 IPC/security tests passed.
- `orange-platform`: 127 tests passed; full workspace format, Clippy with warnings denied, tests,
  and build passed.
- Frontend: 36 tests passed; production Vite build passed.
- Supply chain: 830 locked dependencies across 7 ecosystems and 76 configured URLs passed; SBOM
  and license validation covered 791 library components.
- Resource manifest: 59 files passed.
- Desktop shell: 4/4 steps passed; the native app remained alive for 8 seconds and forced test
  shutdown left zero Orange app, Control Plane, Data Plane, or service processes.

The Windows Data Plane audit built the executable twice with `-trimpath`, an empty build ID, the
locked toolchain and tags, and obtained identical hashes. Its offline process smoke used only a
loopback HTTP server and mixed proxy, exercised HTTP and SOCKS5, selected `node-b`, read `node-b`
back from sing-box, observed non-zero upload/download totals, closed stdin, and confirmed process
and listener cleanup without forced fallback.

Three discarded full-quality attempts demonstrated fail-closed policy behavior before the final
34-step pass: the reviewed service ACL baseline rejected an unsynchronized stdio-policy field, the
runtime log audit rejected `fmt.Print*`, and the supply-chain URL audit rejected a runtime probe URL
misclassified as a build source. The final code synchronizes the independent ACL baseline, uses no
runtime log sink, and represents the pinned sing-box probe target by a semantic policy identifier.

## Artifacts

| Artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `target/debug/orange-app.exe` | 17,019,904 | `46227271bdfead87f625fa1f696feb83aac428b157f748d2424c2a9e66a12a5d` |
| `artifacts/tauri-sidecars/orange-control-plane-x86_64-pc-windows-msvc.exe` | 21,835,776 | `86e1f2e62d0bc3ca9aac8dfdbc8654f24d63715b16bba813de3a442b281c5878` |
| `artifacts/data-plane/windows-amd64/orange-data-plane.exe` | 17,345,536 | `fd8468392e8b049646cbb07507df3ba230b459d5d4aa511726ad10a336ffb3f1` |

The Data Plane executable is `NotSigned`, classified `unsigned-debug`, and
`release_allowed: false`. The runtime manifest and artifact audit use the same final digest.

## Remaining Work

- implement the bounded Rust stdio client and wire it to `DataPlaneNodeBackend` with revision and
  active-instance checks;
- expose only the required node operations through the existing restricted Named Pipe and shared
  runtime, then connect lifecycle traffic events and Tauri/UI;
- run external delay cancellation and real signed TUN selector-switch packet capture tests;
- complete protected installation, approved signer, Windows 10 22H2/Windows 11, and
  Linux/macOS/iOS evidence.

No Android/iOS build was repeated because this increment changes only the Windows external host,
Windows service policy, and host build module. No mobile source, capability, dependency, or
generated project changed.

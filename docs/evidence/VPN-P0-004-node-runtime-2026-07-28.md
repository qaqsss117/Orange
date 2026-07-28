# VPN-P0-004 Node Runtime Evidence

- Date: 2026-07-28
- Hosts: Windows 11 amd64 and Android 16 / API 36 x86_64
- Slice status: `in_progress`

## Qualification Scope

This evidence began with the platform-independent selector catalog, confirmed
node selection, bounded delay-test scheduler, traffic session, and durable
selection ledger. A later Windows increment now wires the production node
backend to the managed sing-box host. It still does not claim lifecycle event
wiring, Tauri commands, product UI, real signed-TUN packet capture, or
five-platform runtime acceptance.

No network endpoint, executable path, process capability, credential field,
WebView command, Tauri capability, or platform permission was added. The node
runtime cannot access `BootstrapTransport`; a dedicated test keeps the shared
Control Plane state `ready` across Data Plane selection.

## Public Selector Catalog

`SanitizedDataPlaneConfig` now retains a second, non-sensitive projection after
the closed subscription has been normalized. It contains only:

- selector ID;
- explicit default node ID;
- selector-member node IDs; and
- the public protocol family: Shadowsocks, Trojan, or Hysteria2.

Servers, ports, credentials, TLS names, routes, generated `orange-*` objects,
arbitrary sing-box objects, and all Control Plane outbounds are absent. The
catalog is derived from validated internal references rather than by parsing the
generated JSON again.

`contracts/data-plane/node-runtime.schema.v1.json` closes the catalog,
selection, delay result, and traffic display DTOs. The aggregate
`node-runtime.v1.json` fixture is serialized exactly by Rust. The static audit
rejects server, password, credential, URL, host, path, Authorization, or raw
outbound fields in the public schema.

## Confirmed Selection And Recovery

`DataPlaneNodeRuntime` serializes selection mutations with an atomic guard. A
user selection must already belong to the requested selector. The runtime then:

1. reads the current backend selection;
2. applies the requested node through the fixed native backend trait;
3. reads the backend selection again and requires an exact match;
4. reads every selector authoritatively; and
5. persists the confirmed set.

Readback mismatch, invalid backend membership, unavailable backend, rejected
selection, or persistence failure never returns a confirmed result. The runtime
attempts to restore and re-read the previous selection; a failed compensation
is exposed separately as `node-runtime-rollback-failed`.

On restart or a new configuration revision, reconciliation reuses a persisted
node only while it remains a member of the same selector. A removed or missing
node falls back to the selector's explicit sanitized default. Every restored
selection is applied and read back before the new ledger revision is committed.

## Delay Tests

Single-node testing is the one-target form of the same batch contract. Requests
are limited to 64 unique selector/node pairs, concurrency is limited to 8, and
the per-probe timeout is limited to 100 through 60,000 milliseconds. Scoped
workers preserve request order and never exceed the requested concurrency.

The native backend receives both the timeout and shared cancellation token and
must stop the platform probe at either boundary. The runtime checks cancellation
before and after each probe, categorizes a late result as timed out, converts a
backend panic or invalid zero delay to unavailable, and exposes only four
states: available, timed out, cancelled, and unavailable. No probe accepts a
URL, hostname, route, or arbitrary core request.

## Traffic Session

`TrafficSession` consumes only bounded upload/download totals for the currently
active Data Plane instance. It rejects instance, sequence, timestamp, clock,
counter, rate, and JavaScript-safe-integer overflow. Totals may only increase;
rates are computed from monotonic elapsed time with integer arithmetic.

The existing single-pending-sample `TrafficEventThrottler` limits event output
without an unbounded queue. Stopping drops the pending envelope, clears the
instance, and sets both displayed rates to zero while retaining final totals.
Samples received after stop fail as inactive, and a new instance starts from
zero so stale speed or totals cannot cross instance ownership.

## Durable Non-Sensitive State

Settings schema v3 adds a closed `DataPlaneNodeSelectionLedger` containing only
one non-zero configuration revision and at most eight selector-to-node ID pairs.
IDs use the same bounded ASCII grammar and reject reserved `orange-*` names.
The existing private generation store writes the complete ledger under its
write lock, preserves unrelated settings, and skips unchanged writes.

Both v1 and v2 settings migrate to v3 with an empty selection ledger through the
existing fsync/rename path. File tests cover migration, bounded validation,
preference preservation, unchanged detection, reopen durability, and atomic
failure behavior.

## Fault Coverage And Static Gate

Eighteen Rust runtime tests cover DTO redaction and fixture alignment,
confirmed readback, readback mismatch rollback, persistence rollback, explicit
rollback failure, valid cross-revision restore, deleted-node default fallback,
unknown/invalid backend state, request bounds, bounded concurrency, timeout,
cancellation, unavailable results, traffic rate/throttling, counter and clock
regression, stop clearing, and Control Plane isolation.

`scripts/security/check_data_plane_nodes.py` fixes catalog derivation,
select/readback/persist ordering, restore/default ordering, concurrency and
target limits, cancellation/timeout markers, traffic stop clearing, settings v3
persistence, public DTO closure, Tauri isolation, a 15-test floor, and the
required `in_progress` status. Seven mutation tests remove readback, expand
concurrency, add a sensitive DTO field, retain stopped speed, expose the runtime
to Tauri, or claim completion and prove that the gate fails closed.

The generated audit reports:

- `rust_runtime_tests: 18`;
- `maximum_delay_concurrency: 8`;
- `maximum_delay_targets: 64`;
- `selection_requires_backend_readback: true`;
- `production_backend_wired: true`;
- `windows_production_backend_wired: true`; and
- `webview_commands_added: false`.

## Windows Gate

`python scripts/ci/run.py quality` passed all 34 steps after formatting the new
schema. It included 122 security/mutation tests, 36 frontend tests, 127
`orange-platform` tests, workspace formatting and Clippy with warnings denied,
all workspace tests/builds, both Go modules, Control Plane audits, Windows Data
Plane/service audits, 918 locked dependencies, and 59 managed resources. Source
isolation scanned 418 files and 152 production text files.

`python scripts/ci/run.py desktop-shell` passed all four steps. The freshly
built application remained alive for eight seconds; terminating its exact PID
left zero `orange-app` or `orange-control-plane` processes.

| Artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| Windows `orange-app.exe` | 17,019,904 | `58f394dbb95beb1fdeb5521f0a9e1673abeb41a6847c23b8a817a8e14dee42c5` |
| Windows Control Plane sidecar | 21,835,776 | `86e1f2e62d0bc3ca9aac8dfdbc8654f24d63715b16bba813de3a442b281c5878` |

## Android Gate

The eight-step Android shell job passed a fresh aarch64 Rust/Tauri build, merged
APK permission audit, lint, instrumentation build, and artifact recording. The
APK exposes only the existing Internet permission plus Android-generated
non-exported receiver metadata; no new mobile command or capability was added.

The connected Android 16 / API 36 x86_64 emulator installed and launched the
current binary. All four Rust/Kotlin/Keystore instrumentation tests passed,
secure and bridge preferences were empty afterward, and both app/test packages
were uninstalled.

| Artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| Android debug APK | 127,279,570 | `45c0170e87c96496a2527ad1412a24cf3e4aa415557c02b792416ac0a8e15662` |
| Android instrumentation APK | 625,024 | `3d252e98529ca133b77b026bcd7af6dc7215fff181a5583a7847d145ac9790ec` |

## Remaining Acceptance Work

The slice remains `in_progress`:

- the Windows production backend and real Rust/Go process prove exact selector
  selection/readback, while other platform backends remain unwired;
- delay timeout/cancellation is a strict backend contract but has not been
  measured against real sing-box probes;
- lifecycle traffic counters and stop events are not wired to the runtime;
- selection restoration is not connected to production revision activation or
  restart;
- no Tauri command, React node page, or homepage traffic view is intentionally
  exposed yet;
- no packet capture proves that business API traffic remains on the Control
  Plane during a real node switch; and
- Linux, macOS, iOS, production signing, real TUN behavior, and formal
  dependency acceptance remain outstanding.

Mocks and platform-independent contracts do not substitute for real backend and
five-platform evidence.

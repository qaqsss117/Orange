# VPN-P0-004 Node Runtime Evidence

- Date: 2026-07-28
- Hosts: Windows 11 amd64 and Android 16 / API 36 x86_64
- Slice status: `in_progress`

## Qualification Scope

This evidence began with the platform-independent selector catalog, confirmed
node selection, bounded delay-test scheduler, traffic session, and durable
selection ledger. A later Windows increment now wires the production node
backend to the managed sing-box host and restricted Named Pipe. The shared
runtime owner is now implemented, and the Windows application can consume one
fixed installer identity file to create the same native client for lifecycle
and node ownership. No real installer currently writes or protects that file,
and no production subscription source or backend currently drives activation.
The platform transaction now hands the committed revision and only its public
selector catalog to the Windows runtime sink after journal commit. This
evidence still does not claim lifecycle event wiring, Tauri commands, product
UI, real signed-TUN packet capture, or five-platform runtime acceptance.

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

## Shared Runtime Ownership

`SharedDataPlaneNodeRuntime` owns at most one active runtime. Selection,
restoration, delay tests, catalog reads, and traffic reads hold a shared read
lock; install and clear hold the write lock. Reconfiguration therefore waits
for an active bounded operation rather than publishing old and new revisions
concurrently.

Install constructs a candidate under the write lock, reconciles every selector
through backend readback and durable storage, and only then replaces the active
runtime. A failed reconciliation returns its exact public error and preserves
the previous runtime. Clear removes the active revision and every later
operation fails closed as backend unavailable. Generic `Arc` forwarding for
both backend and selection storage lets the application reuse one native client
and one `FileSettingsStore` without copying either owner. The shared runtime
retains only the public selector catalog; it does not retain sanitized JSON.
`install_catalog` provides that narrow boundary and restores persisted choices
before publication.

## Windows Application Ownership

The Windows application reads installation identity only from the fixed
`orange-installation-id.v1` sibling of its own executable. The file must be a
regular non-symlink file confined to that canonical directory and contain
exactly 32 lowercase hexadecimal bytes. Missing files, extra newlines,
uppercase input, invalid bytes, relative directories, and path escape all
leave the application on `UnconfiguredVpnAdapter`; no ID is generated,
enumerated, accepted from WebView, or logged.

A valid identity creates one cloneable `NamedPipeClient`. Its clones share the
same request sequence and are installed into both the desktop lifecycle
coordinator and `WindowsNodeRuntimeHost`. The host retains one shared settings
store and exposes only native Rust install/clear ownership for an already
sanitized active configuration. It is managed as Tauri state but no node,
identity, path, or raw-configuration command was added to the WebView.

`WindowsNodeRuntimeHost` now implements `ActiveDataPlaneNodeRuntime`. The
subscription transaction invokes that sink only after the revision journal is
committed. Installation failure clears stale runtime ownership, and recovery
clears a runtime whose revision no longer matches the backend/journal. The
unconfigured sink remains explicit for platforms and startup paths without a
production owner.

This is an application-side handoff contract, not installer evidence. The
current debug layout has no identity file; the signed installer must later
create the exact file with protected ACLs and configure the service with the
same value. Tauri still does not own a production subscription pipeline,
backend, or approved credential-to-config activation source.

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

## Lifecycle Event Monitor

`DataPlaneEventBridge` gives lifecycle state and traffic a single increasing
sequence per instance. State is emitted before traffic from the same
observation; stop emits `unconfigured` against the retiring instance and drops
pending traffic. Snapshot regression or counters supplied for an inactive
snapshot fail before the stream advances.

`DataPlaneEventHub` defaults to 64 envelopes with a hard maximum of 256 and
retains only the newest entries. Its overflow count is numeric; the background
monitor latches one fixed diagnostic rather than filling the diagnostic ring.
The monitor polls at 500 ms and confirms the lifecycle snapshot after every
traffic read so a concurrent stop/restart cannot misattribute counters. It
registers a cancellable Data/background task, and wakes, joins, and releases
its task lease on shutdown.

`WindowsNodeRuntimeHost` supplies authoritative lifecycle snapshots through the
same fixed-identity `NamedPipeClient` and traffic through the installed shared
runtime. Tauri starts the monitor only for a provisioned host and manages the
native event hub without adding a command, capability, or WebView emitter.

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

Twenty-two Rust runtime tests cover DTO redaction and fixture alignment,
confirmed readback, readback mismatch rollback, persistence rollback, explicit
rollback failure, valid cross-revision restore, deleted-node default fallback,
unknown/invalid backend state, request bounds, bounded concurrency, timeout,
cancellation, unavailable results, traffic rate/throttling, counter and clock
regression, stop clearing, Control Plane isolation, shared owner installation
and clearing, failed candidate preservation, `Arc` forwarding, and active
operation/reconfiguration serialization, including catalog-only installation
that restores persisted selection before publication. Two Windows application
tests cover valid installer identity discovery/private ownership and missing or
malformed identity failure. The Windows service suite also rejects malformed
identity files at its client boundary.

`scripts/security/check_data_plane_nodes.py` fixes catalog derivation,
select/readback/persist ordering, restore/default ordering, concurrency and
target limits, cancellation/timeout markers, traffic stop clearing, settings v3
persistence, shared candidate reconciliation-before-publish ordering, public
DTO closure, Tauri isolation, a 15-test floor, and the required `in_progress`
status. Ten mutation tests remove readback, publish a shared candidate early,
expand concurrency, add a sensitive DTO field, retain stopped speed, expose the
runtime to Tauri, drop the Windows application owner or its runtime sink trait,
or claim completion and prove that the gate fails closed.

Four additional event-source Rust tests cover unified state/traffic sequence,
retiring-instance stop, stale snapshot rejection, bounded hub eviction, real
backend polling, task cancellation, thread exit, and registry cleanup. Six
additional mutations reject capacity expansion, sequence bypass, missing task
registration, removal of the Windows event backend, removal of provisioned
Tauri startup, or a WebView emitter.

The generated audit reports:

- `rust_runtime_tests: 22`;
- `maximum_delay_concurrency: 8`;
- `maximum_delay_targets: 64`;
- `selection_requires_backend_readback: true`;
- `shared_runtime_manager: true`;
- `production_backend_wired: true`;
- `windows_production_backend_wired: true`; and
- `windows_app_runtime_owner_wired: true`;
- `active_node_runtime_handoff_contract: true`;
- `windows_node_runtime_sink_wired: true`;
- `rust_event_source_tests: 4`;
- `native_lifecycle_event_source_wired: true`;
- `windows_traffic_event_monitor_wired: true`;
- `default_event_capacity: 64`;
- `maximum_event_capacity: 256`;
- `event_poll_interval_milliseconds: 500`;
- `production_activation_source_wired: false`; and
- `webview_event_emitter_wired: false`; and
- `webview_commands_added: false`.

## Windows Gate

`python scripts/ci/run.py quality` passed all 34 steps for this increment. It
included 145 security/mutation tests, 36 frontend tests, 141
`orange-platform` tests, workspace formatting and Clippy with warnings denied,
all workspace tests/builds, both Go modules, Control Plane audits, Windows Data
Plane/service audits, 830 locked dependencies, and 59 managed resources. Source
isolation scanned 429 files and 155 production text files.

`python scripts/ci/run.py desktop-shell` passed all four steps. An independent
runtime check then kept the freshly built application alive for eight seconds;
terminating its exact PID left zero new application, Control Plane, Data Plane,
or service processes.

| Artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| Windows `orange-app.exe` | 17,449,984 | `1f7f0f0bba8122cb3be456d5fcc27c9e9c404e3e4bff3c82cda367e4ad188f52` |
| Windows Control Plane sidecar | 21,835,776 | `86e1f2e62d0bc3ca9aac8dfdbc8654f24d63715b16bba813de3a442b281c5878` |

## Android Gate

The eight-step Android shell job passed a fresh aarch64 Rust/Tauri build, merged
APK permission audit, lint, instrumentation build, and artifact recording. The
APK exposes only the existing Internet permission plus Android-generated
non-exported receiver metadata; no new mobile command or capability was added.

The existing connected Android 16 / API 36 x86_64 baseline installed and
launched the application. All four Rust/Kotlin/Keystore instrumentation tests
passed, secure and bridge preferences were empty afterward, and both app/test
packages were uninstalled. Device execution was not repeated for this
increment; the current shared source was covered by the fresh aarch64 shell
build, merged-permission audit, lint, and instrumentation assembly.

| Artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| Android universal debug APK | 247,658,328 | `9cd94a92746e9ef98e7cf7771969a2cea5f82a258d657a7894e7a4337da47aa5` |
| Android instrumentation APK | 625,024 | `3d252e98529ca133b77b026bcd7af6dc7215fff181a5583a7847d145ac9790ec` |

## Remaining Acceptance Work

The slice remains `in_progress`:

- the Windows production backend and real Rust/Go process prove exact selector
  selection/readback, while other platform backends remain unwired;
- delay timeout/cancellation is a strict backend contract but has not been
  measured against real sing-box probes;
- lifecycle and runtime traffic now feed the bounded native Windows event hub,
  but no WebView emitter or UI consumer is exposed;
- committed revision activation now has a catalog-only node runtime handoff and
  restart retry contract, but there is no production pipeline/backend/source
  that invokes it in the application;
- the fixed installer identity file has no signed installer or protected-file
  ACL evidence yet;
- no Tauri command, React node page, or homepage traffic view is intentionally
  exposed yet;
- no packet capture proves that business API traffic remains on the Control
  Plane during a real node switch; and
- Linux, macOS, iOS, production signing, real TUN behavior, and formal
  dependency acceptance remain outstanding.

Mocks and platform-independent contracts do not substitute for real backend and
five-platform evidence.

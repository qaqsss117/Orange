# ARC-P1-005 Event, Task, And Observability Evidence

- Date: 2026-07-27
- Hosts: Windows 11 amd64, Ubuntu 24.04.4 under WSL2, and Android 16 / API 36
- Slice status: `in_progress`

## Native Event Contract

`orange-platform` defines a version 1 `EventEnvelope` with a non-zero instance
ID, increasing sequence number, non-zero Unix millisecond timestamp, and one
typed Control state, Data state, or numeric traffic event. Rust construction
and deserialization reject unknown fields, unsupported versions, zero
identifiers, and values above `9_007_199_254_740_991`. The same JavaScript-safe
integer ceiling is fixed in the JSON Schema and TypeScript parser, including
all four traffic counters.

The checked-in state and traffic fixtures are consumed by Rust and TypeScript.
The TypeScript parser reconstructs a strict object instead of trusting a type
assertion, and rejects unknown fields, enum drift, fractions, and unsafe
integers. Its event cursor accepts only the explicitly selected current
instance and a strictly increasing sequence; old instances, duplicates, and
reordered events are reported without mutating the applied sequence.

No Tauri event emitter is exposed. A desktop-only
`get_data_plane_event_snapshot` command now returns the bounded hub as a closed,
versioned snapshot after validating an empty request. The strict TypeScript
consumer parses that snapshot and advances only the selected stream instance
and increasing sequence; the separate `get_plane_state` response remains the
authoritative connection state.

## Native Data Plane Producer

`DataPlaneEventBridge` combines authoritative adapter snapshots and node
runtime counters into one sequence per Data Plane instance. A state transition
is published before traffic observed in the same poll. A transition to the
zero-instance `unconfigured` adapter snapshot is represented against the
retiring instance, so consumers can clear it without accepting an invalid zero
event ID. Snapshot regression, state mutation without sequence movement, and
traffic attached to an inactive snapshot fail closed without advancing the
stream.

The bridge uses `TrafficSession::observe_with_sequence`, preserving the
existing counter, clock, rate, and JavaScript-safe integer checks while sharing
sequence ownership with lifecycle events. Stop and instance replacement clear
the pending sample before the next stream. `DataPlaneEventHub` is a rolling
native queue with a default capacity of 64 and hard maximum of 256; it drops
only the oldest envelope and records a numeric dropped count.

`DataPlaneEventMonitor` polls every 500 ms, reads lifecycle before traffic, and
publishes only events produced by the bridge. After a traffic read it confirms
the lifecycle snapshot again, discarding the counters if state or instance
changed during the two native calls. It owns a cancellable Data Plane
background task lease. Cancellation or application teardown wakes the worker,
joins its thread, and removes the task record. Snapshot, traffic, and bridge
failures are latched into fixed diagnostics rather than arbitrary text; a full
event hub records one fixed overflow diagnostic instead of flooding the ring.

On Windows, `WindowsNodeRuntimeHost` implements `DataPlaneEventBackend` using
the same fixed-identity `NamedPipeClient` for authoritative lifecycle and the
already installed shared node runtime for traffic. Tauri starts the monitor
only when installer identity provisioning succeeded. The normal debug layout
remains unconfigured and starts no monitor thread.

## Traffic Backpressure

`TrafficEventThrottler` uses caller-supplied monotonic processing time rather
than wall-clock event timestamps. The first sample for an instance is emitted
immediately. Samples inside the configured interval coalesce into one pending
envelope, so queue growth is fixed at zero or one; a flush emits only the most
recent sample after the interval. A new instance clears old pending state.

Wrong event kinds, duplicate or reordered sequences, and a regressing
monotonic clock fail closed. Sequence rejection occurs before the clock is
observed, so a stale sample with a future processing time cannot prevent a
later valid sample from being accepted.

## Task Lifecycle

The native task registry defaults to 64 active tasks and enforces a hard maximum
of 256. Every task has a fixed Control, Data, or Platform category and is either
cancellable, deadline-bound, or a background-only non-cancellable operation
with a fixed reason. A page-owned task cannot be non-cancellable, and a zero
deadline is rejected.

Page close and deadline expiry set a shared cancellation token without blocking
the registry lock on task work. The task observes the token and releases its
RAII lease when it exits; explicit completion or lease drop removes the registry
entry. Dropping an unfinished lease also signals cancellation, which prevents a
detached task handle from leaving an unbounded registry record.

## Local Diagnostics And Debug Bundle

Diagnostics accept only fixed category, severity, code, metric name, metric
unit, and integer value types. They have no arbitrary message, URL, host, node,
domain, path, request body, query, credential, secret, or token fields. The
in-memory ring defaults to 256 entries, enforces a hard maximum of 4096, and
reports how many old entries were dropped. It does not write a file, use a log
sink, or send remote telemetry.

Bundle preparation snapshots the typed diagnostic ring and active task
registry, recursively audits every serialized field and string value a second
time, and rejects output above 512 KiB. It first returns only a typed preview
with counts, categories, byte size, and a confirmation ID. The pending object
releases bytes only when consumed with that exact ID; a mismatch consumes and
drops the pending bytes. Confirmed-bundle debug formatting reports only its
length.

`src-tauri` manages the in-memory `DiagnosticsHub` and `DataPlaneEventHub`; the
Windows producer writes only to those native owners. The only new exposure is
one read-only snapshot command granted to the `main` window by a desktop-only
capability; Android/iOS handlers remain unchanged. No file, dialog, shell,
network, logging, event-emitter, or bundle-export capability was added. No new
Cargo or npm dependency was introduced.

## Focused Verification

```text
cargo test --package orange-platform data_plane_events
4 passed

cargo test --package orange-platform data_plane_nodes
22 passed

cargo test --package orange-app --lib
8 passed

cargo clippy --package orange-platform --package orange-app --all-targets -- -D warnings
passed

python scripts/security/check_data_plane_nodes.py
22 runtime tests and 4 event-source tests audited

python -m unittest scripts.security.tests.test_data_plane_nodes
19 passed
```

The focused event tests cover unified lifecycle/traffic sequencing,
retiring-instance stop, snapshot regression, bounded oldest-only eviction,
backend polling, task cancellation, worker exit, and registry cleanup. The
existing observability, node runtime, and subscription tests continue to cover
safe-integer alignment, cursor rejection, traffic backpressure, task leases,
diagnostic eviction, public DTO closure, and production-boundary isolation.

## Windows And Linux Gates

Windows `python scripts/ci/run.py quality` passed all 35 steps with 438 source
files and 159 production text files scanned, 156 security/mutation tests, 41
frontend tests, 141 `orange-platform` tests, and 59 registered resources. The
four-step desktop-shell task passed. `target/debug/orange-app.exe` was
17,495,040 bytes with SHA-256
`da5069c5557c451eb884712896f347320e21f920cf085af9f13ad4733c2782e0`,
stayed alive for an eight-second native startup window, and stopping its exact
PID left zero new Orange application, Control Plane, Data Plane, service, or
sing-box processes.

An isolated Ubuntu 24.04.4 WSL2 copy excluded `.git`, `.ci-tools`, `artifacts`,
`dist`, `node_modules`, `target`, and `src-tauri/gen`. Its quality task passed
all 21 steps with 278 source files and 101 text files scanned, 43 security
tests, 10 frontend tests, 75 passing Rust tests and one explicitly isolated
native-secret-store test, a 790-component SBOM, and 53 resources. The four-step
desktop-shell task passed; the 203,236,360-byte application had SHA-256
`ae6452b8534a3992ebcd199028c2ed7553ef105d0ed55b733ad71c65a772d3c1`
and stayed alive for the full eight-second Xvfb/D-Bus window. No isolated
sidecar remained, and the exact `/tmp/orange-arc-p1-005.9spEcf` evidence copy
was removed and independently confirmed absent.

## Android Gate

The Android shell task regenerated the controlled project and passed all eight
steps, including the aarch64 Rust build, merged-permission audit, Android lint,
instrumentation assembly, and debug artifact recording. A separate x86_64
native build passed and the resulting APK permission snapshot contained only
`INTERNET`, the app-private dynamic receiver permission, the `DUMP`-guarded
profile receiver, and implied faketouch, with no FileProvider.

The freshly rebuilt aarch64-compatible universal debug application APK was
247,675,464 bytes with SHA-256
`0dcba92e00508e2b2ac0445c1d55de85c71505a8a67367f49a717fac268969e9`.
The 625,024-byte instrumentation APK had SHA-256
`3d252e98529ca133b77b026bcd7af6dc7215fff181a5583a7847d145ac9790ec`.
The existing Android 16 / API 36 x86_64 device baseline reported `OK (4
tests)` for the Rust/Kotlin/Keystore path; device execution was not repeated
for this increment.

## Remaining Acceptance Work

This slice remains `in_progress`, not `review` or `done`. The Windows native
Data Plane producer, its background task, and bounded desktop snapshot consumer
are wired, but Control Plane event production, other platform producers, and
the user-visible debug bundle preview/export workflow are not wired. The future
export path must keep
the exact preview-confirmation boundary and obtain only the minimum
user-selected file access. The formal `ARC-G0-003` dependency also remains
`review`.

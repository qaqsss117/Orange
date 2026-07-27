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

No Tauri event emitter or new WebView command is exposed in this increment.
The contract and consumer establish the boundary for the later production
Control/Data event wiring without claiming that those producers are connected.

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

`src-tauri` manages the in-memory `DiagnosticsHub` for future native producers,
but its invoke handler remains exactly `get_plane_state` and
`get_runtime_info`. The main-window capability list is unchanged, and no file,
dialog, shell, network, logging, or bundle-export capability was added. No new
Cargo or npm dependency was introduced.

## Focused Verification

```text
cargo test --package orange-platform --package orange-app
orange-platform: 41 passed
orange-app: 4 passed

cargo clippy --package orange-platform --package orange-app --all-targets -- -D warnings
passed

pnpm check
format, lint, supply-chain, TypeScript build, and production build passed
3 frontend files / 10 tests passed

python scripts/security/check_control_egress.py
38 production/runtime sources scanned; 0 runtime log sinks
```

The focused Rust tests cover cross-language fixtures, schema and safe-integer
alignment, cursor rejection, bounded traffic coalescing, stale-event clock
isolation, bounded task registration, page-close/deadline cancellation,
background-only non-cancellable reasons, lease cleanup, diagnostic eviction,
secondary sensitive-data audit, and preview-confirmation byte release.

## Windows And Linux Gates

Windows `python scripts/ci/run.py quality` passed all 21 steps with 293 source
files and 101 text files scanned, 43 security tests, 10 frontend tests, 75 Rust
workspace tests, a 784-component SBOM, and 53 registered resources. The
four-step desktop-shell task passed. `target/debug/orange-app.exe` was
12,730,880 bytes with SHA-256
`1b903913525eead753bcf23c16addae4c776fe855998abf18d3bef31dffccec4`,
stayed alive for an eight-second native startup window, and left no new
Control Plane sidecar after shutdown.

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

Android 16 / API 36 on x86_64 installed and launched the current
124,750,175-byte application APK with SHA-256
`5a4c9933603bc3a19044a48440037ff7f7d044986e869698f0e012c4ac87551c`.
The 625,024-byte instrumentation APK had SHA-256
`3d252e98529ca133b77b026bcd7af6dc7215fff181a5583a7847d145ac9790ec`.
The device reported `OK (4 tests)` for the current Rust/Kotlin/Keystore
artifacts, and both debug packages were confirmed absent after the runner.

## Remaining Acceptance Work

This slice remains `in_progress`, not `review` or `done`. Real Control/Data
event producers, production long-running tasks, and the user-visible debug
bundle preview/export workflow are not wired. The future export path must keep
the exact preview-confirmation boundary and obtain only the minimum
user-selected file access. The formal `ARC-G0-003` dependency also remains
`review`.

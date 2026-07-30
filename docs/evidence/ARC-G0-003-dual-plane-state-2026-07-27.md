# ARC-G0-003 Dual-Plane State And Adapter Evidence

- Date: 2026-07-27
- Host: Windows 11 amd64
- Slice status: `done` after acceptance review on 2026-07-30

## State Ownership

`crates/orange-domain/src/state.rs` defines the complete Control Plane and Data
Plane state vocabularies plus explicit transition matrices. Repeating the same
state is an idempotent no-op; an invalid edge returns a typed error without
mutating the current state.

`SharedControlPlaneState` is the single state source shared by the coordinator
and the existing desktop `ManagedControlPlane`. Sidecar start moves through
decrypting/starting to ready, host status refreshes the authoritative state,
start failures become failed, and stop returns to cold. Data Plane operations
never hold or mutate this Control Plane state.

## Platform Adapter Boundary

`PlatformVpnAdapter` exposes only four typed operations:

```text
snapshot()
start(configuration revision)
stop(instance ID)
restart(instance ID, configuration revision)
```

The interface cannot carry a URL, file path, shell string, arbitrary JSON map,
credential, or sing-box internal object. `ConfigurationRevision` rejects zero.
The production placeholder adapter reports only `unconfigured` and fails start
or restart closed until a platform slice installs a real adapter.

`VpnController` enforces these invariants:

- repeating start, stop, or an in-flight restart does not call the adapter a
  second time;
- a different configuration revision uses restart and a new instance;
- permission rejection enters `permission_required`, while timeout, crash,
  unavailability, or protocol failure enters `failed`;
- a failed restart retains the active old-instance path, so retry uses restart
  and stop can still clean it up; a failed fresh start remains inactive and
  retries with start;
- a synchronous command response must advance the controller; a stale,
  duplicate, or out-of-order operation snapshot is a protocol violation rather
  than a successful no-op;
- an event from an older instance or a non-increasing sequence is discarded;
- a newly constructed consumer restores the adapter's authoritative snapshot;
  and
- a Data Plane crash leaves a ready Control Plane unchanged.

## WebView Query Boundary

`get_plane_state` is registered in the canonical JSON schema, Rust command
registry, Tauri build manifest, generated command ACL, main-window capability,
and TypeScript invoke wrapper. Its strict request carries only `schemaVersion`;
its forward-compatible response carries only `controlPlane` and `dataPlane`.
Unknown request fields and unknown state enum values fail closed. This allows a
rebuilt WebView to query native truth without exposing lifecycle mutation or
platform configuration to the frontend.

The capability now grants exactly:

```text
allow-get-plane-state
allow-get-runtime-info
```

The cross-platform permission policy and its failure fixture were updated to
the same exact set. No filesystem, dialog, shell, network, or secret capability
was added.

## Focused Verification

```text
cargo test --package orange-domain --package orange-platform
orange-domain: 13 passed
orange-platform: 21 passed

cargo clippy --package orange-domain --package orange-platform --all-targets -- -D warnings
passed

cargo test --package orange-app
4 passed

pnpm test
2 files, 6 tests passed

python -m unittest scripts.security.tests.test_platform_permissions -v
7 passed
```

The mock adapter tests cover success, repeated commands, explicit restart,
timeout, permission denial, adapter crash, active-instance-aware retry, stale
synchronous command responses, a newer sequence arriving before an older
sequence, an old-instance event arriving after restart, and state restore by a
rebuilt consumer.

## Full Gates

The Windows `python scripts/ci/run.py quality` gate passed all 21 steps. It
scanned 281 source files and 90 text files, ran 43 security tests, 6 frontend
tests, and 55 Rust tests, and validated the Go bridge, 784-component SBOM, 53
resources, licenses, and supply-chain policy. The four-step Windows desktop
shell task also passed. `target/debug/orange-app.exe` was 12,503,040 bytes with
SHA-256
`97265728af6d00e6d2b382af4e6831e6edbec133d9627aa706a3af9cdd948eff` and
stayed alive for an eight-second native startup window.

An isolated WSL2 copy at `/home/dev/orange-arc-g0-003-final-20260727` excluded
`.git`, generated mobile projects, dependency directories, artifacts, and Rust
targets. Its Ubuntu 24.04.4 quality gate passed all 21 steps with 43 security
tests, 6 frontend tests, 54 passing Rust tests and one explicitly isolated
native-secret-store test, a 790-component SBOM, and the same 53 resources. The
four-step Linux desktop shell task passed; the 200,843,288-byte application had
SHA-256
`acbace4663426e9214bb3f9ddecf26940bd9c615c9343d6fa765264c7c3c7f06`
and stayed alive for the full eight-second Xvfb/D-Bus window. The evidence
workspace was deleted and confirmed absent afterward.

The final Android shell task passed all eight steps without Rust warnings:
controlled project regeneration, aarch64 Rust/Tauri compilation, exact merged
permission audit, lint, instrumentation build, and debug artifact recording.
The generated manifest still requested only `INTERNET`; the merged APK added
only AndroidX's private receiver permission, the DUMP-guarded profile receiver,
and implied faketouch feature, with no FileProvider. A separate explicit x86_64
rebuild produced a 121,978,455-byte APK with SHA-256
`d6655ea798000934d9b6f91afbb6099cf31ce3eed71a5eeabcd224e5b444117b`.
On Android 16 / API 36 the current x86_64 binary installed and launched, the
Rust/Kotlin/Keystore bridge and native storage suite reported `OK (4 tests)`,
and the runner removed both application packages.

## Acceptance Outcome

All six state-machine and adapter rules have direct tests and registered
evidence, and `ARC-G0-002` has passed its own acceptance. Concrete Windows,
Linux, Android, and Apple VPN adapters remain assigned to their platform
slices; implementing TUN is explicitly outside `ARC-G0-003`. The unavailable
Apple runner and remote-CI execution continue to block `ARC-G0-001`, not this
platform-neutral state contract. The slice is therefore `done`.

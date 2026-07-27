# VPN-P0-002 Data Plane Lifecycle Evidence

- Date: 2026-07-28
- Slice status: `in_progress`
- Production adapter: not wired

## Qualification Scope

This increment establishes the reusable native supervision and cleanup layer
behind `PlatformVpnAdapter`. It does not claim that a packaged sing-box core,
desktop helper, mobile VPN service, real TUN, route, DNS, proxy, or listener was
started. Tauri intentionally remains on `UnconfiguredVpnAdapter` until a
platform `G0` implementation can satisfy the backend contract.

The production supervisor receives only `ConfigurationRevision` and a
monotonic instance ID. It does not accept an executable path, arguments,
environment, shell command, URL, or sing-box object from the WebView boundary.

## Native Supervisor Contract

`SupervisedVpnAdapter` owns the lifecycle independently of React and any one
`VpnController` consumer. A platform backend must provide:

- a side-effect-free preflight that validates configuration, permission, and
  the approved core/helper;
- a process handle with stable PID, nonblocking exit checks, authoritative
  readiness, graceful stop request, and forced termination plus reaping; and
- idempotent cleanup that attempts all process, port, proxy, route, and DNS
  ownership associated with an instance.

The monitor runs on a named native thread through a weak reference. Policy
construction rejects a zero polling interval or any interval above two
seconds. Starting processes become online only after backend readiness.
Startup timeout, readiness failure, process-query failure, and unexpected exit
all remove the active-process flag, enter failed, wake state consumers, and run
the same cleanup path.

Stop first requests graceful shutdown and polls until its deadline. It then
forces and reaps the process if needed, always invokes cleanup, and records
whether the result was graceful or forced. Cleanup failure remains failed and
can be retried by a later stop. Dropping the final native owner also forces and
cleans any remaining process.

## Authoritative State Recovery

`AdapterSnapshot` now carries activity separately from lifecycle state. A
failed logical attempt can therefore remain visible without claiming that a
core/helper is still alive. `VpnController` rereads the adapter after a failed
operation before applying the public error state, preserving whether an old
instance actually survived a restart failure.

Snapshot-change waiting uses a condvar owned by the native adapter. A rebuilt
consumer reads the current process-backed snapshot and does not depend on
frontend memory or a React component lifecycle. Control Plane state is stored
and tested independently.

## Fault And Process Tests

Thirteen focused supervisor Rust tests cover:

- invalid configuration, permission denial, unavailable core/spawn, and zero
  residual ownership;
- 20 repeated duplicate start/stop cycles with a maximum of one resource
  owner;
- startup timeout and forced cleanup;
- unexpected exit detection without UI polling;
- stop timeout, forced termination, and disposition reporting;
- failed forced termination retaining the live process handle for a later
  successful stop retry;
- cleanup failure remaining failed followed by successful recovery;
- restart replacing the instance without overlapping ownership;
- consumer reconstruction and Control Plane readiness isolation; and
- controller reconciliation of an inactive authoritative failure snapshot;
- a real child copy of the Rust test binary becoming ready, surviving consumer
  reconstruction, crashing on an external marker, and being detected and
  cleaned by the background monitor.

The static lifecycle audit fixes the two-second bound, version-only backend
surface, weak monitor, forced stop, drop cleanup, active-instance snapshots,
minimum Rust fault coverage, and `in_progress` status. It rejects production
use of arbitrary process commands, executable paths, arguments, or shell
markers. The report explicitly records that no production adapter is wired.

## Verification

### Windows

`python scripts/ci/run.py quality` passed all 25 steps. The run scanned 337
source files and 124 production text files, passed 62 security tests and 20
frontend tests, and passed 125 workspace Rust tests. The separate Control
Plane host audit passed seven native process tests. Go verification, the
six-test direct-dial audit, the 785-component/53-resource SBOM, supply-chain
checks, and the final 18-token Data Plane application scan also passed.

The four-step desktop-shell task passed. `orange-app.exe` remained alive for
an eight-second native startup window; after stopping that exact application,
no new Control Plane sidecar remained.

| Artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| Windows `orange-app.exe` | 16,719,872 | `8fe572b6df8c047311ccf2a809709115947b05f9d76fb3762c500611639561b6` |
| Windows Control Plane sidecar | 21,835,776 | `86e1f2e62d0bc3ca9aac8dfdbc8654f24d63715b16bba813de3a442b281c5878` |

### Linux

The source was copied without `.git`, `.ci-tools`, `artifacts`, `dist`,
`node_modules`, `target`, or `src-tauri/gen` into the isolated WSL2 directory
`/home/dev/orange-linux-smoke-20260728.MVOw2k`. The first run exposed that the
login environment did not include Go. The complete 25-step run was repeated
from the start with the installed fixed Go 1.25.5 path and passed; this was not
recorded as a source success until the repeat completed.

The Linux run passed the same 62 security and 20 frontend tests, 125 workspace
Rust tests, seven separate Control Plane host process tests, Go, SBOM,
supply-chain, and final application scans. One native desktop secret-store
test was explicitly ignored because the isolated session had no available
unlocked graphical secret store.

The four-step desktop-shell task passed. The application remained in the
Xvfb/D-Bus session for the eight-second timeout and neither `orange-app` nor
the Control Plane sidecar remained afterward.

| Artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| Linux `orange-app` | 215,204,328 | `9a0b65b42fd0e1d39d12915b91afa42b6056d284b261030fd8fdb45d04b170f3` |
| Linux Control Plane sidecar | 22,666,517 | `dd2a6d3954b59d8477e59f83f873ff5eb0ac5359c62eaea4c9d344d7525662e0` |

The exact isolated directory was then deleted and independently confirmed
absent.

### Android

`python scripts/ci/run.py android-shell` passed all eight steps, including
controlled project regeneration, four Rust targets, current aarch64
Rust/Tauri compilation, merged-permission audit, Android lint,
instrumentation assembly, and artifact recording. The merged application
retains only `INTERNET`, its private dynamic-receiver permission, the
`DUMP`-guarded profile receiver, and implied faketouch; it has no FileProvider
or privacy permission.

The current source was then rebuilt for the only connected device, which
reported Android 16, API 36, and x86_64. The application installed and
launched, produced the real Rust-to-Kotlin bridge receipt, and passed all four
Keystore instrumentation tests with `INSTRUMENTATION_CODE: -1`. Secure and
bridge preferences were empty afterward, both debug packages were removed,
and no production Data Plane capability was added.

| Artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| x86_64 application APK | 126,262,951 | `d1b212df5f9c5bcdc1999c23bc9843e1c7c9305e136473d035705d88a7df59d8` |
| instrumentation APK | 625,024 | `3d252e98529ca133b77b026bcd7af6dc7215fff181a5583a7847d145ac9790ec` |

## Remaining Acceptance Work

`VPN-P0-002` remains `in_progress`, not `review` or `done`:

- desktop platforms still need fixed, integrity-checked sing-box/helper
  backends and privileged process/session ownership;
- Android needs the approved libbox plugin plus `VpnService`, and Apple needs
  the Network Extension/helper implementation;
- sanitized configuration revisions are not yet installed into a protected
  runtime location or handed to a real core;
- real TUN permission denial, route/DNS/port acquisition, crash rollback, and
  restoration require platform integration and system-level tests;
- a Tauri/native event bridge and public start/stop/restart command boundary
  are not registered; and
- macOS/iOS build and runtime evidence is unavailable in this environment.

The mock and child-process evidence qualifies the reusable supervisor, not the
platform VPN implementations or the overall slice.

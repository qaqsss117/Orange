# P0 Production Slice Acceptance (2026-07-31)

## Scope

This review maps the production Windows 10 development run and current
automated gates back to the acceptance rules of four slices. It does not turn
the unsigned artifacts into release evidence. Formal Windows signing,
Windows 11, a real machine restart, and non-Windows platform implementations
remain owned by their platform and release slices.

The installed runtime candidate used by the network, IPC, lifecycle, crash,
upgrade, and uninstall phases had SHA-256
`338942109f592db3b5991313f6fc9204f614264f19b0a9774229039a2b3d109c`.
The combined ignored result is `passed`; its current SHA-256 is
`723c7d0fd6e9bb376a2f7f71be82535e582246317572880f6bf4c9920e289e68`.
Reports contain only bounded public state, hashes, counts, and boolean results.

## `WIN-P0-002` Service, Named Pipe, And Dual-Plane Host

| Rule | Accepted evidence |
| ---: | --- |
| 1 | The installed pipe rejected independent different-user and Low Mandatory Level processes; the service remained running. |
| 2 | The 4 KiB strict protocol, fixed 20-command catalog, unknown-field rejection, and path/URL/shell bans pass Rust and mutation tests. |
| 3 | `crash-ui` killed the installed UI while the native service retained authoritative state; the restarted app re-read the same service-owned lifecycle. |
| 4 | `crash-service` detected service loss and restored proxy, route, DNS, TUN, process, and listener ownership to a safe state. |
| 5 | Control Plane listener audits stayed empty; the Data Plane exposed only the registered loopback mixed listener while active. |
| 6 | Install, upgrade, failure rollback, both uninstall paths, and final clean-state proved fixed SCM lifecycle, protected binaries, and no orphan service/process. |

The signer allowlist and release flag intentionally remain closed. Those are
`WIN-G0-001` and `REL-P1-005` conditions, not additional rules of this IPC
slice. `WIN-P0-002` is `done`.

## `VPN-P0-002` Data Plane Lifecycle

| Rule | Accepted evidence |
| ---: | --- |
| 1 | Invalid configuration, permission, spawn, and readiness paths leave no ownership in focused tests; installed clean-state covers process, listener, route, DNS, proxy, and TUN residue. |
| 2 | The native supervisor test performs 20 repeated start/stop cycles with at most one owner. |
| 3 | Native ownership is independent of React; consumer reconstruction tests and installed `crash-ui` recovery read authoritative service state. |
| 4 | The two-second detector is fixed by policy and tests; installed Data Plane and service crash phases both reached the expected failed/cleanup path. |
| 5 | Graceful timeout, forced reap, retryable cleanup failure, and installed stop/crash cleanup all pass. |
| 6 | The production Bootstrap Control Plane completed login/subscription and remained independently available throughout Data Plane mixed/TUN and crash phases. |

Linux, macOS, Android, and iOS backends remain in their corresponding platform
slices. They do not reopen this shared supervisor contract. `VPN-P0-002` is
`done`.

## `API-P0-003` Account And Subscription

| Rule | Accepted evidence |
| ---: | --- |
| 1 | The live account and subscription routes returned strict accepted envelopes; native mapping covers email, balance, plan, expiry, used, and total traffic without recording private values. |
| 2 | Safe-integer overflow, null/zero total, saturation, time, expired, exhausted, and unknown-status tests pass. |
| 3 | The sensitive subscription URL and body remain native, travel only through the allowlisted Control Plane, enter the sanitizer and Windows revision pipeline, and never enter a WebView response. |
| 4 | Expired/exhausted refresh clears the native subscription credential. Current policy also denies a new start even when an old revision remains; an already-online instance remains explicitly stoppable. |
| 5 | Manual refresh renders loading/error/success, rejects concurrent refresh, and retains the last safe snapshot on failure. |
| 6 | Logout verifies Data Plane stop before deleting all three credentials and cached identity; the same service then logs in again through the still-running Control Plane. |

Mobile command handlers and non-Windows production runs remain platform work;
they do not change this fixed shared account contract. `API-P0-003` is `done`.

## `UI-P0-004` Connection Home

| Rule | Accepted evidence |
| ---: | --- |
| 1 | Status and mutations use only native authoritative readback; there is no optimistic online state. |
| 2 | Eight Data Plane states plus explicit expired/exhausted subscription presentation are covered. Online expiry explains that the current connection remains stoppable but cannot be restarted. |
| 3 | React and native atomic guards reject repeated transition actions; fixed safe retry copy is used after failure. |
| 4 | Only online authoritative traffic is displayed; all other states and read failures zero rates with bounded binary-unit formatting. |
| 5 | The banner uses the registered local Orange development asset and performs no remote fetch. |
| 6 | The responsive baseline covers 360x800, 412x915, tablet, 1366x768, and 1440x900 with named tokens, 44 px controls, and no clipped/overlapping text. |

The installed application completed real production subscription activation and
both mixed and TUN start/stop paths. Platform-native screenshots remain owned
by platform UI release matrices. `UI-P0-004` is `done`.

The current responsive screenshots are ignored local acceptance artifacts. The
hashes bind this review to the exact images without adding user-machine output
to the repository:

| Viewport artifact | SHA-256 |
| --- | --- |
| `ui-p0-004-360x800.png` | `8dea01df3e25e0e1dd4faa19496dec2bdc4d2a5d603a75c8598ca8cf3ff83611` |
| `ui-p0-004-412x915.png` | `18e9ef155b22dd97c64c255a3bf985ffc99aea47838b7e1e1cae0cabc771e418` |
| `ui-p0-004-768x1024.png` | `69b4466551321c01a347f0d3505efa390e62d6d70b9a62fe2f7b03e2a79bbe6c` |
| `ui-p0-004-1366x768.png` | `ae03653f8ecefb7e078d53e64ba976392f5df6cc66ed5fcd611f7e1b94e5f13c` |
| `ui-p0-004-1440x900.png` | `6ca1d04a9ea8f893854d54ffc609c9c2e0f494e04fa16dbebcb1f3ea47882551` |

## Verification Boundary

The final source gate must pass the complete Python mutation suite, frontend
tests/build, Rust formatting/Clippy/tests/build, both Go modules, package and
SBOM checks, and the pinned Windows quality task. Any regression in the four
status rows, subscription start gate, IPC boundary, lifecycle cleanup, or
authoritative UI polling reopens the affected slice.

The pinned Windows quality entry point passed all 35 steps. It included 201
Python security/mutation tests, 54 frontend tests plus formatting, lint, and
build, the complete Rust workspace formatting/Clippy/test/build matrix, both Go
modules, Bootstrap, package/SBOM checks, and the Windows Data Plane artifact
audits.

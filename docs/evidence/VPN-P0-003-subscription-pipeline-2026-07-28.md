# VPN-P0-003 Subscription Pipeline Evidence

- Date: 2026-07-28
- Hosts: Windows 11 amd64 and isolated Ubuntu 24.04 WSL2
- Slice status: `in_progress`

## Qualification Scope

This increment establishes the platform-independent transaction core for
subscription candidate staging, health qualification, atomic activation, and
crash recovery. It does not claim a production subscription download contract,
a protected platform revision writer, a real bypass sing-box instance, or a
production `SubscriptionDataPlaneBackend`. This increment adds the native node
runtime handoff contract and its Windows sink, but does not instantiate a
production pipeline or invent a production activation source.

The pipeline accepts only `SanitizedDataPlaneConfig` produced by the existing
closed Rust sanitizer. It does not accept a URL, Authorization value, arbitrary
process path, shell command, or WebView request. No Tauri command, capability,
network permission, file permission, or frontend DTO was added.

## Transaction And Recovery Contract

`SubscriptionPipeline` serializes apply and recovery with one atomic guard. An
apply records the candidate in the durable revision journal before handing the
sanitized buffer to the backend. The backend must stage without taking system
ownership, start a bypass candidate, and report all three health properties:

- the core is ready;
- the target outbound is reachable; and
- candidate DNS is independent of Bootstrap DNS and cannot create a bootstrap
  loop.

Only a fully healthy candidate may atomically take system ownership. The
pipeline then reads the authoritative active revision before committing the
journal. The sanitized input buffer is cleared immediately after the stage
call returns. Before clearing, the pipeline retains only the non-sensitive
`SelectorCatalog`. After the journal commit succeeds, that catalog and the
committed revision are handed to `ActiveDataPlaneNodeRuntime`; raw sanitized
JSON never enters or survives in the node runtime.

Runtime publication is explicit in the outcome as `Installed`, `Unconfigured`,
or `Unavailable`. An installation failure clears any stale runtime before
returning `Unavailable`; a cleanup failure instead returns
`subscription-recovery-required`. Reapplying the committed revision retries
installation, covering the crash window after journal commit. Recovery keeps a
runtime only when its revision matches the authoritative backend and journal,
and clears it after a revision restore, rollback, or mismatch.

Failure compensation restores the complete previous revision first, removes
the candidate with an idempotent backend operation, and clears the journal
marker last. This ordering makes every interruption recoverable without an
unmarked candidate file. A persistence failure after backend activation uses
the same rollback path.

Startup recovery covers these authoritative states:

- an active, healthy candidate with a pending marker is committed;
- an inactive or unhealthy pending candidate is rejected while current stays
  active;
- a missing current process is restored and health checked;
- an already restored previous revision is committed as a rollback;
- an unhealthy current revision falls back to a healthy previous revision; and
- unexpected ownership on a first install is cleared.

If neither committed revision is healthy, recovery clears system ownership and
returns `subscription-recovery-required` instead of leaving an unhealthy
instance active. After ownership is restored, an active revision absent from
the current, previous, and candidate journal slots is removed idempotently.

`FileSettingsStore` implements atomic journal mutations under its existing
write lock. Each mutation reloads the latest settings, preserves unrelated
preferences, writes a new fsync/rename generation, and retains the previous
valid generation. A failed commit leaves the durable candidate marker for the
next recovery attempt.

## Fault Coverage And Static Gate

Eighteen pipeline Rust tests cover the three health failures, first-install
failure, activation failure, persistence failure after activation, candidate
recovery on both sides of activation, killed-current restoration, an already
restored previous revision, unexpected first-install ownership, unknown active
cleanup, no-healthy-revision fail-closed behavior, idempotency, cleared input,
concurrent apply/recover rejection, commit-before-runtime publication, failed
runtime installation cleanup/retry, cleanup failure, and recovery of a stale
runtime revision. Two additional file journal tests cover preference
preservation, reopen durability, and failed commit recovery.

`scripts/security/check_subscription_pipeline.py` records the three mandatory
health checks and 12-test floor, fixes
journal/stage/health/activation/commit/runtime and restore/discard/reject
ordering, requires stale-runtime cleanup, revision reconciliation, and the
Windows sink implementation, rejects direct HTTP/process/shell capability,
rejects premature production Tauri wiring, and prevents the slice from
claiming completion before production backends are audited. Eight security
unit tests, including seven mutations, prove those gates fail closed. The audit
report recorded `active_node_runtime_handoff_contract: true`,
`windows_node_runtime_sink_wired: true`, `production_backend_wired: false`,
`production_activation_source_wired: false`, and
`webview_commands_added: false`.

## Windows Gate

`python scripts/ci/run.py quality` passed all 34 steps. The run included 139
security/mutation tests, 36 frontend tests, 137 `orange-platform` tests,
workspace formatting and Clippy with warnings denied, all workspace tests and
builds, both Go modules, Control Plane audits, Windows Data Plane/service
audits, 830 locked dependencies, and 59 managed resources. Source isolation
scanned 428 files and 154 production text files. The subscription pipeline and
node runtime audits passed without errors.

`python scripts/ci/run.py desktop-shell` passed all four steps. An independent
runtime check kept the application alive for eight seconds; stopping its exact
process left no new application, Control Plane, Data Plane, or service process.

| Artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| Windows `orange-app.exe` | 17,299,456 | `72dea812ca276ad2132621c5f51fe785d0d033d9f9a2cc19c27f9a0a8b94217a` |
| Windows Control Plane sidecar | 21,835,776 | `86e1f2e62d0bc3ca9aac8dfdbc8654f24d63715b16bba813de3a442b281c5878` |

## Linux Gate

The Linux results below predate this runtime-handoff increment and were not
repeated. They remain historical portability evidence rather than acceptance of
the current source revision.

At that prior checkpoint, source was copied without `.git`, `.ci-tools`, `artifacts`, `dist`,
`node_modules`, `target`, or `src-tauri/gen` to a dedicated Ubuntu 24.04 WSL2
workspace. Node 22.23.1 and Go 1.25.5 archives were downloaded from the pinned
mirrors and verified against the Node distribution checksum list and repository
Go SHA-256 before use. Rust 1.95.0 was already installed.

`portable-quality` passed all 25 steps without engine warnings. It included the
same 85 security and 22 frontend tests, 106 `orange-platform` tests with one
expected unavailable graphical secret-store test ignored, portable Windows
service tests, both Go modules, and an 806-component Linux SBOM. The four-step
Linux desktop-shell job also passed.

The Linux application remained alive for eight seconds under Xvfb and a D-Bus
session. It and the Control Plane sidecar were absent after termination. The
352-line active Linux `orange-platform` dependency tree contained no
`windows-sys`.

| Artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| Linux `orange-app` | 215,362,944 | `0db12ba7ff0fffb8b8a6d20c27c1136b1821e10e54d00de197753bc43267da01` |
| Linux Control Plane sidecar | 22,666,517 | `dd2a6d3954b59d8477e59f83f873ff5eb0ac5359c62eaea4c9d344d7525662e0` |

The dedicated Linux source, Node/Go toolchain, and dependency-tree paths were
validated, deleted, and confirmed absent after evidence collection.

## Remaining Acceptance Work

The slice remains `in_progress`:

- no approved contract defines how the native subscription credential fetches
  the real node configuration;
- no production backend writes sanitized revisions to a protected platform
  location or launches a bypass candidate;
- core/outbound/DNS health values are a strict backend contract but have not
  been measured against a real production candidate;
- no platform backend atomically transfers and restores TUN, proxy, route, or
  DNS ownership;
- the pipeline and Windows node sink have a tested native handoff contract, but
  no production `SubscriptionPipeline` instance or activation source is wired
  into Tauri startup, account refresh, logout, or product UI; and
- real backend, Android, macOS, iOS, production signing, and formal dependency
  acceptance remain unavailable.

Mocks and the generic transaction core do not substitute for these inputs.

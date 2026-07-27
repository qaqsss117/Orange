# API-P0-003 Account And Subscription Evidence

- Date: 2026-07-28
- Hosts: Windows 11 amd64 and isolated Ubuntu 24.04 WSL2
- Slice status: `in_progress`

## Qualification Scope

This increment establishes the native account and public-subscription refresh
baseline. It does not claim a production subscription-to-node-configuration
contract, Data Plane activation, product UI completion, or real-backend and
five-platform acceptance.

The existing fixed `BusinessCommand::Account` and
`BusinessCommand::Subscription` routes remain the only network entry points.
Both use the single `BusinessCommandClient` and authenticated
`BootstrapTransport`; no URL, Authorization value, proxy configuration, or
raw request is accepted from the WebView.

## Native Policy And Secret Boundary

`BusinessApiService` requires initialized configuration and an authenticated
native session before either refresh. Login, registration, account refresh,
and subscription refresh share one atomic operation guard. A second refresh
during an in-flight operation is rejected before a second transport call.

Account refresh strictly decodes `AccountResponse` and replaces the
authoritative native session user. Subscription refresh strictly decodes the
sensitive wire DTO, removes `subscriptionCredential`, derives an effective
public status, and stores only the public fields in the native cache.

Only effective `trial` and `active` subscriptions retain a credential. The
credential is atomically replaced in the platform secret backend; a partial
storage failure restores the previous value. Expired, exhausted, none, and
unknown subscriptions remove a stale credential. All input secret buffers are
cleared on success and failure. An authenticated 401 removes access, refresh,
and subscription secrets and clears both the native session and subscription
cache.

The usage policy is bounded by the JavaScript-safe integer maximum. Checked
addition fails above `2^53 - 1`, remaining bytes use saturating subtraction,
expiry at or before the native clock becomes `expired`, and zero or consumed
totals become `exhausted`. A null total remains valid. Unknown or inactive
states cannot authorize a future Data Plane start.

## Desktop IPC And Permissions

The desktop shell adds `refresh_account` and `refresh_subscription`. Each
request contains only schema version and rejects URL, token,
subscription-credential, and arbitrary extra fields in Rust, JSON Schema, and
TypeScript. Responses contain only the existing public account and
subscription DTOs; frontend parsing rejects injected credential fields.

The `desktop-business` capability grants the two commands only to the main
window on Linux, macOS, and Windows. Android and iOS remain without these
handlers. No browser network, file, shell, storage, logging, or additional
platform permission was added.

## Focused Verification

- `orange-domain`: 22 tests passed, including IPC injection rejection and
  subscription arithmetic/status policy.
- `orange-platform`: 89 tests passed, including fixed routes, native-only
  credential storage, expired/exhausted deletion, 401 cleanup, duplicate
  refresh rejection, input clearing, and rollback after a failed write.
- `orange-app`: 5 tests passed and the desktop Tauri handlers compiled.
- Frontend: 5 files and 22 tests passed; the focused business API/command run
  passed 12 tests.
- Platform-permission audit passed with 10 policy tests and exactly six
  desktop-business permissions.

## Windows Gate

`python scripts/ci/run.py quality` passed all 28 steps. The run included 80
security tests, 22 frontend tests, formatting, workspace Clippy with warnings
denied, all workspace tests and builds, both Go modules, the 799-component
SBOM, Control Plane audits, and Windows Data Plane/service audits. Source
isolation scanned 364 files and 135 production text files; Control Plane
egress scanned 52 production and runtime-log sources with zero log sinks.

`python scripts/ci/run.py desktop-shell` passed all four steps. The application
remained alive for eight seconds and stopping its exact process left no newly
running Control Plane sidecar.

| Artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| Windows `orange-app.exe` | 16,804,352 | `cf7a10ee62e8c3981598a3900612f3d280b500dabf07ea89513ab4dbd3092957` |
| Windows Control Plane sidecar | 21,835,776 | `86e1f2e62d0bc3ca9aac8dfdbc8654f24d63715b16bba813de3a442b281c5878` |

## Linux Gate

The current source was copied without `.git`, `.ci-tools`, `artifacts`,
`dist`, `node_modules`, `target`, or `src-tauri/gen` to
`/home/dev/orange-linux-smoke-20260728-api-p0-003`. Temporary Go 1.25.5 and
Node 22.23.1 distributions were downloaded from the configured mirrors and
verified against the repository-pinned Go digest and the Node distribution
SHA-256 list.

The final `portable-quality` run passed all 24 steps without toolchain engine
warnings. It passed the same 80 security and 22 frontend tests, 13 bootstrap
tests, 22 domain tests, 89 of 90 platform tests with the expected unavailable
graphical-secret-store test ignored, six portable Windows service protocol
tests, both Go modules, and an 806-component Linux SBOM. The four-step Linux
desktop-shell job also passed.

One discarded Node-correct run observed the pre-existing Data Plane background
exit timing test before its cleanup counter advanced. The test then passed 20
consecutive focused runs and the complete final 24-step run; no source was
changed to mask it. An earlier discarded run also confirmed that the isolated
copy must regenerate its intentionally excluded fixed Control Plane sidecar
before the bootstrap memory check.

The final Linux application remained alive for eight seconds under Xvfb and a
D-Bus session. It and the Control Plane sidecar were absent after termination.
The 321-line active Linux dependency tree contained no `windows-sys`.

| Artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| Linux `orange-app` | 215,347,632 | `54def2d41ea31914fdcfcde200c3725308f6bccf430fec6daa306ae103e827a4` |
| Linux Control Plane sidecar | 22,666,517 | `dd2a6d3954b59d8477e59f83f873ff5eb0ac5359c62eaea4c9d344d7525662e0` |

The exact isolated source and toolchain directories were validated, deleted,
and independently confirmed absent.

## Remaining Acceptance Work

The slice remains `in_progress`:

- no approved contract defines how `subscriptionCredential` retrieves or
  carries the real node configuration, so no endpoint or format was invented;
- the subscription is not yet sanitized into a revision, activated in the
  Data Plane, or atomically switched by `VPN-P0-003`;
- logout does not yet enforce stop-Data-Plane-before-secret-deletion ordering;
- product loading, error, success, and already-connected expiry behavior are
  not implemented;
- no production API, real backend E2E, mobile transport, macOS, iOS, or
  production signing evidence is available; and
- formal dependencies remain unfinished.

Development fixtures, mocked transports, and desktop-only commands do not
substitute for those inputs.

# API-P0-002 Dynamic Configuration And Authentication Evidence

- Date: 2026-07-27
- Hosts: Windows 11 amd64, Ubuntu 24.04.4 under WSL2, and Android 16 / API 36
- Slice status: `in_progress`

## Qualification Scope

This increment establishes the native development baseline for dynamic
configuration, login, registration, and authentication-session recovery. It
does not claim that the approved production API, an Android/iOS embedded
Control Plane transport, or real-backend end-to-end coverage exists.

All executable business routes and URL policies remain marked `development`
and `releaseAllowed: false`. The `.invalid` payment, support, and banner hosts
are non-routable test policy, not production configuration.

## Native Service And Commands

`orange-platform` owns one `BusinessApiService` backed by the existing single
`BusinessCommandClient`. Initialization performs these bounded operations:

1. wait up to 15 seconds for the shared Control Plane state to become ready;
2. execute the fixed `config` command through `BootstrapTransport`;
3. strictly decode and validate all wire configuration URLs;
4. inspect native authentication-secret completeness; and
5. validate a complete session through the fixed authenticated `account`
   command.

The desktop shell exposes exactly four additional commands:

| Command | Request | Public response |
| --- | --- | --- |
| `initialize_business` | schema version | config without URLs and session |
| `login` | email and password | authenticated user without credentials |
| `register` | email, password, optional invite | authenticated user without credentials |
| `get_auth_session` | schema version | `signed_out`, `authenticated`, or `unverified` |

The separate `desktop-business` capability grants only those commands to the
main window and only on Linux, macOS, and Windows. The Android and iOS build
manifests retain only `get_plane_state` and `get_runtime_info`; no mobile
handler or capability exposes the new business commands.

Rust and TypeScript independently apply the same bounded ASCII email, UTF-8
password, invite-code, and schema-version rules. One atomic submission guard is
shared by login and registration, so concurrent submissions cannot produce a
second request. Server configuration decides whether an invite is mandatory.

## URL And Bootstrap Boundary

The wire configuration contains API, payment, support, and optional banner
URLs. The public `ConfigResponse`, IPC schema, TypeScript types, and public
fixture contain none of those URLs. Wire fixtures retain only explicit
`<redacted:...>` markers.

All URLs require HTTPS on port 443 and reject credentials, query strings, and
fragments. API and payment origins must use `/`; support and banner URLs may
carry a path. Payment, support, and banner hosts must match their fixed
non-routable development allowlists.

The API hostname is not compiled into `BusinessRoute` or the application.
`BusinessTarget::BootstrapPrimaryApi` asks the already-decrypted native host to
select the first approved bootstrap API host. Dynamic configuration must then
match the live host's decrypted allowlist. The host normalizes the comparison,
requires ready state, and zeroizes both the primary host and allowlist during
shutdown. The application and bootstrap crypto binary passed the five-token
bootstrap plaintext scan without weakening that scan.

## Credential And Failure Semantics

Login and registration wire responses deserialize into automatically zeroized
credential DTOs. Access and refresh tokens are converted directly to native
`SecretValue` buffers and never enter a Tauri response, React state, browser
storage, or runtime log. IPC request `Debug` output reports only byte counts.

Authentication replacement first snapshots all three user secrets, writes
access and refresh, and removes the old subscription credential. Any partial
failure restores access, refresh, and subscription values; if restoration also
fails, the operation returns the stable storage failure. Caller credential
buffers are cleared on every path.

An authenticated route returning 401 invokes the existing native logout
boundary, removing access, refresh, and subscription credentials while leaving
non-user settings intact. Partial stored authentication is also cleared. A
complete session that cannot be checked because bootstrap, DNS, TLS, timeout,
or cancellation is unavailable becomes `unverified` and retains credentials;
it is not reported as authenticated. Expired login responses never replace
stored authentication.

Frontend tests prove that failed invoke calls leave caller-owned email,
password, and invite input unchanged. The production wrapper contains no
storage or console sink and no access-token, refresh-token, or Authorization
field. Static security checks enforce those boundaries.

## Focused Verification

The focused Rust suites passed with 20 `orange-domain` tests, 60
`orange-platform` tests, 5 `orange-app` tests, and the Control Plane host unit
and process suites. Coverage includes new install, valid stored credentials,
authenticated 401 cleanup, unavailable bootstrap, offline account validation,
successful authentication, expired authentication, server-required invite,
duplicate submission, credential rollback, strict URL rejection, and strict
content-type/schema parsing.

The frontend suite passed 5 files and 20 tests. The new command tests cover
strict response parsing, local validation before invoke, caller-input
preservation, and absence of persistence/logging sinks. The full security unit
suite passed 53 tests. The Control Plane audit scanned 44 production sources
and 44 runtime-log sources with zero runtime log sinks.

## Windows Gate

`python scripts/ci/run.py quality` passed all 22 steps, including source
isolation, platform/contract/egress audits, formatting, clippy with warnings as
errors, Rust and frontend tests, bootstrap plaintext scanning, Control Plane
process tests, Go tests, the 784-component SBOM, and all 53 registered
resources.

The four-step desktop-shell job passed. The final debug artifacts were:

| Artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `target/debug/orange-app.exe` | 16,691,200 | `2553c5f639d0024ae16c0c3528e3142e38e742bd45fc782c94d83c6e428f5783` |
| Windows Control Plane sidecar | 21,835,776 | `86e1f2e62d0bc3ca9aac8dfdbc8654f24d63715b16bba813de3a442b281c5878` |

The application remained alive for an eight-second native startup window.
After stopping that exact application process, an executable-path query found
no newly remaining Control Plane sidecar.

## Linux Gate

The final source was copied without `.git`, `artifacts`, `dist`,
`node_modules`, `src-tauri/gen`, or `target` into an isolated Ubuntu 24.04.4
WSL2 workspace. Its 22-step `quality` task passed with 317 scanned files, 119
production text files, 53 security tests, 20 frontend tests, all Rust/Go tests,
and 53 registered resources.

The final Linux artifacts were:

| Artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `target/debug/orange-app` | 213,895,080 | `cd77e611bf5677c213ddfb7a04b566e11b71641304f4f9e7cad13db10c9eb6d0` |
| `target/debug/orange-control-plane` | 22,666,517 | `dd2a6d3954b59d8477e59f83f873ff5eb0ac5359c62eaea4c9d344d7525662e0` |

The application stayed alive for the full eight-second Xvfb/D-Bus window. The
exact evidence workspace `/home/dev/orange-linux-smoke-20260728003946` was
validated against the timestamped workspace boundary, removed, and confirmed
absent.

## Android Gate

`python scripts/ci/run.py android-shell` passed all eight steps: controlled
project regeneration, four Rust target installations, aarch64 Rust/Tauri
build, exact merged-permission audit, Android lint, instrumentation assembly,
and artifact recording. A subsequent current-source x86_64 build completed
without Rust warnings.

The x86_64 merged APK snapshot contains only `INTERNET`, the app-private
dynamic-receiver permission, the `DUMP`-guarded profile receiver, and implied
faketouch. It has no FileProvider or privacy permission. The final device-test
artifacts were:

| Artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| application APK | 124,865,703 | `59a90784a84ee541e889a3e807824fce1ee115cdb5a2f7259851feaf1539d72b` |
| instrumentation APK | 625,024 | `3d252e98529ca133b77b026bcd7af6dc7215fff181a5583a7847d145ac9790ec` |

The only connected device reported Android 16, API 36, and x86_64. It installed
and launched the current application, completed the real Rust/Kotlin/Keystore
bridge receipt, and reported `OK (4 tests)`. Both debug packages were removed
by the runner, and an independent package query returned zero matches.

## Remaining Acceptance Work

The slice remains `in_progress`, not `review` or `done`:

- the approved production API host, paths, DTO contract, and desensitized
  backend fixtures are unavailable;
- Android and iOS still lack their embedded Control Plane transport and the
  four business handlers intentionally fail closed there;
- no real backend was used for login, registration, expiry, offline recovery,
  or fresh-install product-level end-to-end workflows;
- macOS desktop and iOS runtime evidence is unavailable; and
- formal dependencies `API-G0-001`, `BOOT-P0-004`, and `ARC-P1-004` have not
  reached their required final state.

Development fixtures, mocked transport scenarios, and desktop-only commands do
not substitute for those inputs.

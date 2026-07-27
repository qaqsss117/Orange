# SEC-G0-003 Control Egress And Secret Boundary Evidence

- Date: 2026-07-27
- Hosts: Windows 11 amd64 and Ubuntu 24.04.4 under WSL2
- Slice status: `in_progress`

## Endpoint Policy

`security/control-endpoints.yml` is a JSON-compatible YAML policy parsed with a
strict JSON parser, so the build does not add an unpinned YAML dependency. It
defines the ten required business command categories: login, register, config,
subscription, account, plans, orders, invite, tickets, and update.

The current policy is deliberately development-only:

- `release_allowed` and `production_hosts_configured` are both false;
- the sole host is the non-routable `api.orange.invalid`, and the audit requires
  it to match the encrypted development bootstrap fixture exactly;
- the scheme and port are fixed to HTTPS/443;
- redirect following is denied;
- connect and request timeouts match the encrypted bootstrap failover policy;
- concurrency is capped at 16, request/response bodies at 1 MiB, and request
  attempts at one.

The development paths are fixed fixtures, not a claim about the unapproved
production API contract. Production hosts and paths must replace this policy
through an explicit security review before `release_allowed` can change.

## Automated Egress Audit

`python scripts/security/check_control_egress.py` is a fail-closed quality step.
The passing report recorded:

```text
commands: 10
hosts: 1
production sources scanned for network clients: 31
production sources scanned for runtime log sinks: 31
runtime log sinks: 0
approved network implementation: native/controlplane/bridge.go
release allowed: false
```

The audit verifies:

- direct frontend HTTP dependencies and `fetch`, XHR, WebSocket, or EventSource
  construction are absent;
- direct Rust HTTP dependencies and socket client construction are absent;
- Go `net/http` construction exists only in the audited sing-box direct-dial
  bridge;
- the WebView CSP permits only self and Tauri IPC in `connect-src`;
- the public IPC schema exposes no URL, host, Authorization, token, bootstrap,
  route, or file-path request field;
- the Go bridge constructs only HTTPS targets, fixes the production port to
  443, disables proxy-environment discovery and redirects, requires TLS 1.2 or
  newer, and enforces the bootstrap host allowlist;
- production React, Rust, and Go runtime sources contain no unapproved print,
  console, log, or tracing sink that could receive a body, token, node, or local
  path.

Six focused security tests prove that policy, host, transport, dependency,
source, CSP, IPC, and log violations fail the audit. A real Go integration test
returns an HTTPS 302 response without following it and confirms the redirect
target receives zero requests.

## Secret Storage Contract

`orange-platform` now exposes only fixed `AccessToken` and `RefreshToken` keys,
a bounded non-cloneable `SecretValue`, stable redacted errors, a platform
backend trait, a shared `SecretStorage` wrapper, and a production
`DesktopSecretStore`.

- `SecretValue` zeroizes on drop and its `Debug` output contains only
  `<redacted>`.
- Shared storage clears the caller's value after every store attempt, including
  permission failures.
- Logout attempts deletion of both token keys even if the first deletion fails.
- Loads return the controlled value type and are not exposed through a Tauri
  command or WebView DTO.
- The production service name is fixed to `com.orange.vpn`; callers cannot
  inject an arbitrary service or key name.
- Native backend errors are reduced to four stable application categories.
  Third-party diagnostic text never crosses the adapter, and byte buffers
  attached to malformed-data errors are zeroized before returning.

The desktop adapter uses exactly pinned `keyring 4.1.5` with default features
disabled and the maintained `v1` native-store selection: Windows Credential
Manager, macOS Keychain, and Linux Secret Service. The dependency is MIT or
Apache-2.0 licensed and is target-gated out of Android and iOS builds. Mobile
secure-store adapters therefore remain explicit future work rather than
silently falling back to a desktop or plaintext implementation.

Four portable Rust tests cover bounds, redacted debug output, successful
storage/load, success/error clearing, logout deletion, and partial-failure
cleanup. The test backend exists only inside the Rust unit-test module; it is
not a production plaintext store. A fifth cross-desktop test exercises the
native backend: it stores and overwrites the access token, stores the refresh
token, confirms all caller buffers are cleared, loads only the current token,
logs out, and confirms both credentials are absent. A drop guard repeats
cleanup after the test. The test runs normally against Windows Credential
Manager and is ignored by default on Linux and macOS because those platforms
require an available, unlocked native store.

`cargo check -p orange-platform --target aarch64-linux-android` passed and its
dependency tree contains no desktop `keyring` backend. The Linux quality gate
compiled, linted, tested, and linked the Secret Service path.

The Ubuntu 24.04.4 WSL2 host then installed `gnome-keyring 46.1-2ubuntu0.2` and
`libsecret-tools 0.21.4-1build3` from its configured Aliyun Ubuntu mirror.
`scripts/dev/run-linux-secret-store-tests.sh` created private HOME/XDG paths,
started an isolated `dbus-run-session`, unlocked a temporary GNOME Keyring,
and ran the ignored native test through the production `DesktopSecretStore`.
The test passed twice and proved store, overwrite, load, caller-buffer
clearing, logout, and absence of both token keys. An independent
`secret-tool search` found no service record after logout. The runner shut down
the test daemon and left no `/tmp/orange-secret-store.*` directory, real-user
keyring, or test daemon behind. This is real Linux Secret Service lifecycle
evidence in an isolated native keyring; packaged graphical-session integration
and macOS Keychain runtime validation remain outstanding.

### Android Keystore

The managed Kotlin source under `native/android` implements the Android
secure-storage primitive and its internal Tauri mobile plugin without a new
Android dependency or permission:

- a fixed, non-exportable 256-bit AES-GCM key is generated by the
  `AndroidKeyStore` provider under `com.orange.vpn.secret-storage.v1`;
- only versioned IV/ciphertext payloads are committed synchronously to the
  app-private `orange.secure-secrets.v1` SharedPreferences file;
- fresh randomized IVs are required, and GCM additional authenticated data
  binds every ciphertext to its fixed access-token or refresh-token key so the
  records cannot be exchanged;
- plaintext input accepts 1 through 16 KiB, is never converted to a `String`,
  and is zeroized after every successful or failed store attempt;
- intermediate byte arrays are cleared, logout attempts both token removals
  and destroys the token encryption key, and platform exceptions collapse to
  the same four stable redacted errors as the Rust contract.

`src-tauri/src/android_secret_store.rs` implements the shared
`SecretStoreBackend` over a Tauri `PluginHandle`. The plugin registers only on
Android, has no Rust invoke handler, adds no capability permission, and is not
reachable through the WebView command surface. Protocol version 1 accepts only
the two fixed Rust storage names, five fixed handshake/store/load/delete/logout
operations, and a debuggable-build-only completion receipt used by the device
runner. Values cross the in-process Tauri mobile transport as
canonical Base64 rather than token text; Rust-owned encoded/decoded buffers and
Kotlin decoded byte arrays are cleared, and unknown native errors fail closed
to `secret-store-failure`. Tauri's internal JSON/JNI string copies are owned by
the framework and cannot be explicitly zeroized, so they remain an in-process
transport limitation rather than being described as fully controlled memory.
The shared backend contract now supports a platform-specific logout override,
allowing Android logout to destroy its Keystore key while desktop backends keep
the existing delete-both-records default.

The Tauri Android generator copies both managed production sources and the test
source into the ignored generated project, fixes the instrumentation runner,
and verifies byte-for-byte equality. The seven-step `android-shell` job passed
from a regenerated project: source isolation covered 262 files and 78 text
files, the arm64 Rust/Tauri shell built, Android lint and the instrumentation
APK completed, and the debug APK was registered as non-releasable. That APK
was 121,458,242 bytes with SHA-256
`450c4d424419aa96d65dfca88104e872bc2a4112207beba6ec1d185d6af04c33`.

The local `Medium_Phone_API_36.1` AVD ran Android 16 / API 36 on x86_64. The
device-run application APK was 121,574,575 bytes with SHA-256
`954061009cd9cc208293737257836dc791956697f274606335d626ee2f1c5ba6`.
The 623,707-byte test APK had SHA-256
`289be8f6e9211e5a1e349c86769df552a7e6d111af4e864031bdb7af7d96a1e6`.
All four tests passed:

- a debug-only intent requested an App startup self-test; Rust called the real
  Kotlin plugin to handshake, logout, store, load, compare, and logout again,
  then wrote a non-sensitive completion receipt only after the full round trip;
- store, overwrite, load, caller-buffer clearing, logout, absence of both token
  records, and removal of the fixed Android Keystore alias;
- oversize rejection with stable `secret-invalid-value` and input clearing;
- ciphertext exchange between token keys failing authentication with stable
  `secret-store-failure`.

`scripts/dev/run-android-secret-store-tests.py` makes the device proof
repeatable. It launches the real application first, waits for the Rust receipt,
then starts a fresh instrumentation process so Tauri Activity teardown cannot
invalidate the remaining provider tests. The runner reported `OK (4 tests)`,
verified the secure preferences file was `<map />`, and removed both debug
packages. Android lint completed with zero errors. This proves the internal
Rust/Kotlin storage bridge and native primitive, not future typed login-command
wiring or the physical-device/supported-API matrix.

### iOS Keychain

`crates/orange-ios-secret-store` is an internal Tauri plugin carrier. Its build
script links the checked-in `native/apple/secret-store` Swift Package against
the local Tauri iOS API generated from the exact Cargo dependency graph. The
Rust side registers only on iOS, manages the shared `SecretStorage`, and uses
the same protocol version, fixed token keys, canonical Base64 transport, and
stable error mapping already exercised by Android. It defines no Tauri command
handler or capability permission, so the WebView cannot call the native
handshake/store/load/delete/logout operations.

The Swift backend uses Security.framework generic-password records with:

- fixed service `com.orange.vpn.secret-storage.v1` and the same two fixed
  access/refresh account names as Rust;
- `kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly` and explicit
  `kSecAttrSynchronizable = false`, with no Keychain access group or new
  entitlement;
- update-before-add overwrite semantics, exact-one lookup, idempotent delete,
  and logout that still attempts the second token after the first error;
- 1 through 16 KiB canonical Base64 validation, mutable `Data` clearing after
  store/load use, and four stable redacted errors rather than OSStatus details.

The mobile egress gate now checks both Rust/native fixed command sets, the
binding symbol/package link, all required Keychain controls, absence of
UserDefaults/iCloud persistence, and absence of secret-store capability
permissions. Swift production files are also included in the direct-network
and runtime-log scans. The shared protocol's three Rust tests moved into
`orange-platform`; its nine focused tests passed, including the real Windows
Credential Manager lifecycle. `cargo check -p orange-ios-secret-store` and 36
security tests passed on Windows.

`cargo check --workspace --target aarch64-apple-ios` was attempted after
installing the exact Rust target, but the dependency build stopped at
`objc2-exception-helper` because this Windows host has no `xcrun`, Apple clang,
or iPhoneOS SDK. No Swift compile, iOS package link, simulator lifecycle, or
device lifecycle is claimed by this increment; those remain mandatory Apple
host evidence.

## Full Gates

The final Windows `python scripts/ci/run.py quality` passed all 20 steps after
the Linux lifecycle increment:

- source isolation over 268 files and 84 text files;
- 36 security tests, 6 frontend tests, and 36 Rust workspace tests;
- Control Plane, seven-process Rust host, Tauri bundle/integrity, Go,
  784-component SBOM, 53-resource, license, and supply-chain audits.

Because the mobile protocol moved from the Tauri application crate into the
shared platform crate, the seven-step `android-shell` gate was also rerun from
a clean generated project. Source isolation and all 53 resources passed, the
arm64 Rust/Tauri shell built, Android lint completed, and the instrumentation
APK was assembled. The single-ABI debug application APK was 121,483,026 bytes
with SHA-256
`df7e65d618f2c02c9fc69ab2b7f34ebaa3e6a633a65c5bb8e6837336596a6d64`;
the 620,670-byte test APK had SHA-256
`4686e635ab443c287a4aa780db1c4ae95078bfbe4daf709b4a2704b4b25ade3b`.
Both are debug-only and not release eligible. The previously recorded API 36
device lifecycle remains valid because protocol bytes and native commands did
not change; this iOS-focused increment did not rerun the emulator lifecycle.

The clean Linux runner copied the final tree without `.git` to
`/home/dev/orange-linux-smoke-20260727184102` and passed all 20 quality steps:

- source isolation over 266 files and 84 text files;
- 36 security tests, 6 frontend tests, and 35 passing Linux Rust workspace
  tests, with only the explicitly isolated native-store test ignored;
- Linux Control Plane and Tauri bundle audits, including sidecar SHA-256
  `864d44fa56e6595bd30758390f97a6f0c4a2dfb63dd219a454b1f55fdd113330`;
- desktop shell SHA-256
  `933569e28f12de9684531699c85f1abc200715c0c2a17e6ed74fd2bcfa6cc920`
  and a passing eight-second Xvfb/D-Bus startup window;
- 790-component Linux SBOM and the same 53 resources.

The isolated Secret Service runner then enabled and passed the ignored native
test against GNOME Keyring. The temporary keyring and the clean evidence
workspace were removed after their results were recorded.

## Remaining Acceptance Work

This slice remains `in_progress` because the following evidence is still
missing:

- approved production API hosts, paths, and typed business command wiring
  through a single BootstrapTransport;
- wiring the internal Android secret backend into future typed authentication
  commands, plus physical-device and supported-API lifecycle coverage;
- iOS Swift/package compilation plus Keychain lifecycle tests on a simulator
  and physical device;
- macOS Keychain runtime lifecycle tests, plus packaged-application integration
  in a supported graphical Linux session with an available system store;
- privileged packet captures proving runtime Control Plane destinations match
  the approved allowlist and remain distinct from user tunnel traffic;
- completion of the formal `ARC-G0-002` and `BOOT-G0-003` dependencies.

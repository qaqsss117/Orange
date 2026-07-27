# ARC-P1-004 Persistence, Migration, And Rollback Evidence

- Date: 2026-07-27
- Hosts: Windows 11 amd64, Ubuntu 24.04.4 under WSL2, and Android 16 / API 36
- Slice status: `in_progress`

## Storage Boundary

`orange-platform` defines a strongly typed `AppSettings` schema for ordinary
application preferences and Data Plane revision identifiers. Version 2 has the
fixed fields `schemaVersion`, `locale`, `launchOnStartup`, `theme`,
`reducedMotion`, and `dataPlane`; unknown fields are rejected. The revision
ledger accepts only non-zero `currentRevision`, `previousRevision`, and
`candidateRevision` identifiers.

The settings type cannot accept an arbitrary JSON map, URL, host, file path,
bootstrap, node, token, or subscription payload. Access token, refresh token,
and subscription credential remain exclusively in the platform secure-store
contract. Logout attempts removal of all three user credentials even after an
earlier deletion failure, while ordinary application settings are retained.

`src-tauri` resolves the platform-standard application data directory during
startup, loads or migrates `FileSettingsStore`, and manages the native store for
later Rust use. No setting or secret command, WebView handler, capability, or
filesystem permission was added.

## Schema Migration

The checked-in v1 and v2 JSON Schemas and fixtures under `contracts/settings`
define a deterministic v1-to-v2 migration. Locale and launch-on-startup are
preserved; new theme, reduced-motion, and revision fields receive explicit
defaults. The v1 input remains strict, so a migration cannot silently carry an
unknown or sensitive field into v2.

Loading a migrated document writes a new generation only after validation. A
pre-rename failure leaves the original v1 bytes intact and a later reopen can
retry the migration. Storage or settings schema versions newer than this
binary's supported versions return stable unsupported-version errors; `save`
also refuses to overwrite such data.

## Atomic Generation Protocol

Each store uses the fixed `state-v1` directory below an absolute app-data path.
A save validates the typed model, serializes a bounded document, creates a
unique same-directory temporary file, writes all bytes, calls `sync_all`, and
renames it to a monotonically increasing immutable generation. Unix then syncs
the containing directory and enforces `0700` on the directory and `0600` on
generation files. Production file operations use only the Rust standard
library.

The store retains the committed generation and the last validated generation.
It ignores stale `.tmp` files, never treats a symlink or non-file generation as
data, and bounds each document to 64 KiB. If the newest committed generation is
truncated or oversized, the loader validates the prior generation, promotes it
as a new generation, and keeps the recovered source as the fallback. A
simulated interruption before rename preserves the previous generation.

`tempfile = 3.27.0` is pinned only as a test dependency for isolated filesystem
fixtures. It was already present transitively in the lockfile; no production
storage dependency or runtime permission was added.

## Data Plane Rollback Policy

The revision ledger stages a candidate without replacing the active current
revision. Candidate rejection clears only the candidate and returns the current
revision. Once a candidate reports online, the former current revision moves to
previous. An active-version failure selects previous, and a committed rollback
swaps current and previous so the failed revision remains available for
diagnosis or a controlled retry.

Only revision identifiers are persisted. Sanitized sing-box configuration
content remains owned by future `VPN-G0-001`; this slice does not persist raw
server configuration or claim that an identifier alone makes configuration
safe.

## Focused Verification

```text
cargo test --package orange-platform --package orange-app
orange-platform: 30 passed on Windows
orange-app: 4 passed

cargo clippy --package orange-platform --package orange-app --all-targets -- -D warnings
passed

python -m unittest scripts.security.tests.test_control_egress -v
8 passed

python scripts/security/check_control_egress.py
passed; 35 production/runtime sources scanned; 0 runtime log sinks
```

The persistence tests cover exact fixture migration, revision transition and
reopen, interrupted commit cleanup, private Unix modes, corrupt-latest recovery,
migration commit failure, future-version read/write blocking, logout retention,
and rejection of sensitive or arbitrary settings fields. Windows excludes only
the Unix permission-mode test.

## Full Platform Gates

Windows `python scripts/ci/run.py quality` passed all 21 steps with 287 source
files and 95 text files scanned, 43 security tests, 6 frontend tests, 64 Rust
workspace tests, a 784-component SBOM, and 53 registered resources. The
four-step desktop-shell task passed. `target/debug/orange-app.exe` was
12,710,400 bytes with SHA-256
`bf92e9d2243448da71c60f63169efe89ee66f14fcd7339ec7fb2da24b0e6b50a`
and stayed alive for an eight-second native startup window.

An isolated Ubuntu 24.04.4 WSL2 copy excluded `.git`, `.ci-tools`, artifacts,
dependency directories, generated mobile projects, and Rust targets. Its
quality task passed all 21 steps with 43 security tests, 6 frontend tests, 64
passing Rust tests and one explicitly isolated native-secret-store test, a
790-component SBOM, and the same 53 resources. The Unix-only persistence test
confirmed `0700` store-directory and `0600` generation-file modes. The
four-step desktop-shell task passed; the 201,917,648-byte application had
SHA-256
`fca5e9ab78a3d44fa2f55a493ede68bc9558e0b4dfeabc9afd56251b56a7eea5`
and stayed alive for the full eight-second Xvfb/D-Bus window. The isolated
GNOME Keyring runner then passed the real three-credential lifecycle test and
left no temporary keyring. The evidence copy and two interrupted nested smoke
copies created during validation were removed and confirmed absent.

The Android shell task passed all eight steps from a regenerated controlled
project. A separate x86_64 build and exact permission audit passed; the merged
APK requested only `INTERNET`, the app-private receiver permission, the
`DUMP`-guarded profile receiver, and implied faketouch, with no FileProvider.
The 123,195,423-byte application APK had SHA-256
`7f5e40f411d6e21d4fd7cb66e398abb3127cc5bfaf199ecbcb87548931670c18`;
the 625,024-byte instrumentation APK had SHA-256
`3d252e98529ca133b77b026bcd7af6dc7215fff181a5583a7847d145ac9790ec`.
Android 16 / API 36 on x86_64 reported `OK (4 tests)` for the current
Rust/Kotlin/Keystore artifacts, and both debug packages were independently
confirmed absent after the runner completed.

## Remaining Acceptance Work

This slice remains `in_progress`, not `review` or `done`. Standard app-data and
secure-store deletion must still be proven through the signed installer,
upgrade, and uninstall lifecycle on Windows, Linux, Android, macOS, and iOS.
Those platform installers do not exist yet, so this increment does not claim
that uninstall leaves no settings or credential residue. The formal
`ARC-G0-002` dependency also remains `review`.

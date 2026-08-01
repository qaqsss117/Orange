# SEC-G0-002 Platform Permission Baseline Evidence

- Date: 2026-07-27
- Hosts: Windows 11 amd64, Ubuntu 24.04.4 under WSL2, and Android 16 / API 36
- Slice status: `in_progress`
- Current package evidence: GitHub Actions `package #34`, commit `22f84b1`,
  2026-08-01
- Current Android evidence: GitHub Actions `package #43`, commit `c6897c0`,
  2026-08-01
- Current Tauri evidence: GitHub Actions `package #61`, commit `6f5164e`,
  2026-08-01

Except for the explicitly dated current evidence below, this document
records the pre-`97ff13a` baseline. Commit `97ff13a` removed the general policy,
checker, tests, and security workflow on 2026-07-31. Historical results remain
evidence of what passed then, not a claim about current CI capabilities.

## Historical Fail-Closed Policy

Before `97ff13a`, `security/platform-permissions.yml` was JSON-compatible YAML
parsed with the standard JSON parser. It fixed the development shell at that
commit rather than claiming release approval. The policy recorded:

- the exact `main-window` Tauri capability and its two read-only
  `allow-get-plane-state` / `allow-get-runtime-info` permissions;
- the Android application ID, generated manifest and APK paths, source and
  merged-artifact permission sets, component permission guards, and features;
- all registered Apple Info.plist and entitlement inputs;
- all registered Windows capability manifests and service ACL inputs;
- all registered Linux polkit and systemd inputs plus the network-only
  capability ceiling; and
- whether file import exists and whether directory or persistent scope is
  permitted.

The recorded privileged-helper and file-import lists were deliberately empty.
Adding a declaration file without updating the audited policy failed. Windows
service or Linux helper permissions could not be enabled by changing data
alone: the checker required a dedicated implementation and threat-model review
first.

## Historical Automated Audit

Before its removal, `scripts/security/check_platform_permissions.py` used
structured parsers for JSON, Android XML, Apple plist, Cargo TOML, and Windows
XML manifests. Generated platform output was ignored by the portable source
audit so stale ignored files could not make it depend on an Android or Apple
SDK. Platform jobs opted in to their generated evidence explicitly:

```text
python scripts/security/check_platform_permissions.py
python scripts/security/check_platform_permissions.py --require-android-artifact
python scripts/security/check_platform_permissions.py --require-apple-project
```

The Android artifact path was inspected with the exactly pinned `aapt 36.0.0`.
The audit compared exact sets rather than searching only for known-dangerous
names. It also had non-configurable denials for photo/media storage, camera,
microphone, contacts, SMS, phone state, location, and Apple screen-capture
declarations. Tauri `fs:`, `dialog:`, and `shell:` permissions were rejected even
if a policy edit attempted to allow them.

Seven focused tests proved that the baseline succeeded while Tauri file access,
an Android camera permission paired with a weakened policy, an Apple camera
usage description paired with a weakened policy, an unconfigured Android
directory-scoped FileProvider, and an unregistered Linux systemd unit failed
closed. The `aapt` snapshot parsers also had an exact contract test.
The complete provider-neutral security task passed 43 tests and produced a
passing machine-readable permission report.

## Platform Evidence

### Tauri Current Source Snapshot

GitHub Actions
[`package #61`](https://github.com/qaqsss117/Orange/actions/runs/30701586200)
completed the workspace-quality job and all five platform jobs for clean commit
`6f5164e` in 8 minutes 35 seconds. It produced six artifacts and emitted zero
workflow annotations. The workspace-quality job ran
`scripts/ci/check_tauri_capabilities.py` before the other checker contract
tests and uploaded its report even when the gate would fail.

The current `security/tauri-capabilities.json` policy fixes the exact inventory
and privilege-relevant fields of all five capability JSON files: identifier,
window labels, platform restriction, and sorted permission set. The checker
rejects duplicate JSON keys, unknown fields, unregistered capability files,
and any baseline drift. The `dialog:`, `fs:`, and `shell:` prefixes are also
hard-coded denials, so editing the policy cannot weaken them.

The same gate parses all 17 generated permission definition files with
`tomllib`. Each custom `allow-*` grant must have exactly one registered TOML
file containing the matching single-command allow entry and paired deny entry;
unregistered files, unused definitions, duplicate paths, and command mapping
drift fail closed. Six focused tests cover the clean repository, inventory
expansion, privilege drift, policy weakening, TOML mapping drift, duplicate
JSON keys, nonzero exit, and failure-report retention. The complete CI script
suite now contains 34 tests.

The downloaded `orange-tauri-capabilities` ZIP matched GitHub's artifact digest
`sha256:027d86f99815c8c7c352be3fe3d4f11ae1eea8bc8d67222d752fa3a780f766ec`.
Its only file was `report.json`, SHA-256
`94f170e041f7272830cd1b7baf6b31a47823f22eadc54751c4870d25fcc14d60`,
which records `passed=true`, five capabilities, 17 permission definitions,
relative paths, and per-file SHA-256 values. This closes the current Tauri
source-capability baseline gap. It does not provide Windows or Linux package
permission snapshots or a five-platform approval gate.

### Android Current Package Snapshot

GitHub Actions [`package #43`](https://github.com/qaqsss117/Orange/actions/runs/30693779455)
completed the workspace-quality job and all five platform jobs for commit
`c6897c0` in 13 minutes. After the signed release APK and AAB were built, the
Android job required exactly one of each and passed them separately to
`scripts/ci/audit_android_package.py`. The APK path used the SDK
`apkanalyzer manifest print` command; the AAB path used a Java source bridge
and the same SDK's pinned `apkanalyzer` classpath to decode the protobuf base
Manifest as XML. Both paths had a 120-second limit, parsed the result with
`ElementTree`, matched the package ID against `tauri.conf.json`, and compared
exact permission, defined-permission, component-guard, and explicit feature
sets.

The passing release APK and AAB snapshots both contain exactly:

```text
requested: android.permission.INTERNET
requested: com.orange.vpn.dev.DYNAMIC_RECEIVER_NOT_EXPORTED_PERMISSION
defined (signature): com.orange.vpn.dev.DYNAMIC_RECEIVER_NOT_EXPORTED_PERMISSION
component guard: android.permission.DUMP
explicit hardware features: none
shared user ID: none
```

The separate `android.json` and `android-aab.json` reports retain each package's
format, SHA-256, and declaration names but not the dynamic permission's
protection-level value. Both reports were generated before the configured
paths were uploaded in `orange-android`, whose artifact digest is
`sha256:53933faed67071865faca50b6f44ea78df90974ac928ed27405fa839c51105a7`.

This closes the independent current release-APK and release-AAB evidence gaps
for acceptance rule 1 and the corresponding Android subset of rule 6.
VpnService declarations and the supported release-target matrix remain future
acceptance work.

### Android Historical Development Snapshot

The generated source manifest requested only:

```text
android.permission.INTERNET
```

The 121,483,026-byte arm64 development APK had SHA-256
`9d7e7442aaccd1689a4c5306bbf24c93c709377dca0c5450a338b5c20ed4ad31`.
Its merged snapshot contained exactly:

```text
uses-permission: android.permission.INTERNET
uses-permission: com.orange.vpn.dev.DYNAMIC_RECEIVER_NOT_EXPORTED_PERMISSION
defined permission: com.orange.vpn.dev.DYNAMIC_RECEIVER_NOT_EXPORTED_PERMISSION
component guard: android.permission.DUMP
feature: android.hardware.faketouch
```

The package-specific signature permission is generated by AndroidX to protect
non-exported dynamic receivers. `android.permission.DUMP` guards AndroidX's
profile installer receiver; Orange does not request DUMP. `faketouch` is the
platform's implied default feature. The APK contains no photo/media, storage,
camera, microphone, contacts, SMS, phone, or location permission. The clean
eight-step Android shell job regenerated the project, removed the unconfigured
FileProvider, built the arm64 Rust/Tauri shell, passed the merged permission
audit and Android lint, built the instrumentation APK, and recorded the debug
artifact as non-release-eligible.

A separate x86_64 device build produced a 121,755,631-byte APK with SHA-256
`59d152b65f539d6d8ce659c492d2d84ecde0bf6c7b1ac0363f8611c936b1b776`.
It had the same passing merged permission snapshot and no FileProvider. On the
Android 16 / API 36 `Medium_Phone_API_36.1` emulator, the application installed
and launched successfully, the real Rust/Kotlin/Keystore bridge and native
storage tests reported `OK (4 tests)`, and the runner removed both packages.
This is a startup regression check for the manifest reduction, not completion
of the physical-device or supported-API matrix.

### Apple

GitHub Actions [`package #34`](https://github.com/qaqsss117/Orange/actions/runs/30689536851)
completed all five jobs for commit `22f84b1` in 10 minutes 16 seconds. The
current `scripts/ci/audit_apple_package.py` runs after macOS signing/PKG creation
and after iOS IPA creation, before either package is uploaded to App Store
Connect. It parses every packaged `Info.plist` with `plistlib`, inspects every
Mach-O entitlement dictionary with `codesign`, checks the configured bundle ID,
and fails on photo, camera, microphone, contacts, location, or screen-recording
declarations.

The JSON reports contain package SHA-256, bundle-relative paths, and sorted
declaration key names only; entitlement values are not retained. Both audit
steps passed and wrote their reports. The smoke steps separately required their
reports and screenshots before the configured paths were uploaded:

- `orange-macos`, digest
  `sha256:424a27e0171157fddf4f31027dee1ecf897b68b2c3b8349eab4759a055df2d50`,
  was configured to include the signed PKG,
  `target/apple-permissions/macos.json`, and the macOS smoke report;
- `orange-ios`, digest
  `sha256:a40b22d4c4eae23134a57e5c83ce0501ce33d7a1d8fdd013d08ffd769bda51b1`,
  was configured to include the IPA, `target/apple-permissions/ios.json`, the
  iOS smoke report, and its startup screenshot.

This closes the current-package evidence gap for acceptance rule 2 and the
Apple subset of rule 6. It does not restore or approve a cross-platform
permission baseline.

### Windows And Linux Historical Snapshot

The classic Windows development shell captured here had no AppX capability
manifest and no privileged service. Consequently this snapshot claimed no
service ACL completion. The Linux development shell captured here had no
helper, polkit policy, or systemd unit; its Secret Service adapter used the user
session D-Bus and required no privilege.
At the time of this snapshot, a new declaration file under either native
boundary failed the source audit. That checker is no longer present.

### File Import

File import remains unimplemented. At the recorded baseline, the dependency
graph contained no Tauri dialog or filesystem plugin, the WebView capability
granted neither, and policy denied directory and persistent scope. The
controlled Android generation step removed Tauri's unused default FileProvider
because its external-path root would have been broader than the baseline
permitted. Source and merged-artifact audits blocked its return. A later
single-file import implementation must add explicit
temporary-scope behavior and cancellation tests before this acceptance item
can close.

## Historical CI Integration

Before `97ff13a`, the provider-neutral `security` task ran the permission audit
before its unit tests. Desktop, Android, and iOS shell tasks wrote the same
permission snapshot; Android and iOS additionally required their generated
platform evidence. GitHub and Gitee adapters retained the report under
`artifacts/security`.

The current workflow now runs the focused Tauri source-capability gate and the
Android and signed Apple package audits described above. It does not restore
the deleted provider-neutral workflow or claim current Windows, Linux, or
file-import permission coverage.

## Historical Full Gates

The final Windows `python scripts/ci/run.py quality` passed all 21 steps:

- source isolation over 281 files and 90 text files;
- the portable platform permission baseline and Control Plane egress audits;
- 43 security tests, 6 frontend tests, and 55 Rust workspace tests; and
- Control Plane, host, Tauri bundle, Go, 784-component SBOM, 53-resource,
  license, and supply-chain audits.

The isolated Linux runner copied the final tree without `.git`, generated mobile
projects, dependency caches, artifacts, or Rust targets. All 21 quality steps
passed, including 43 security tests, 6 frontend tests, 54 passing Rust tests
and one explicitly isolated native-store test, a 790-component Linux SBOM, and
the same 53 resources. The 200,843,288-byte Linux desktop shell had SHA-256
`acbace4663426e9214bb3f9ddecf26940bd9c615c9343d6fa765264c7c3c7f06`;
the fixed Control Plane sidecar had SHA-256
`864d44fa56e6595bd30758390f97a6f0c4a2dfb63dd219a454b1f55fdd113330`.
The shell stayed alive for the eight-second Xvfb/D-Bus window. The temporary
evidence workspace was removed after recording these results.

The final eight-step Android shell task passed source isolation, project
generation, controlled configuration, arm64 Rust/Tauri build, exact merged
permission audit, Android lint, instrumentation build, and debug artifact
recording without Rust warnings. A separate current-source x86_64 build passed
the same permission audit and produced a 121,978,455-byte APK with SHA-256
`d6655ea798000934d9b6f91afbb6099cf31ce3eed71a5eeabcd224e5b444117b`;
Android 16 / API 36 startup and all four device tests passed, after which both
packages were removed. No Apple build or permission result is inferred from
these Windows, Linux, and Android gates.

## Remaining Acceptance Work

This slice remains `in_progress` until evidence exists for:

- current machine-readable policies and blocking package snapshots for Windows
  and Linux after their general checker was removed;
- a signed Windows package/Win11 declaration snapshot and current service ACL
  evidence at the package boundary;
- the future Linux helper's exact polkit/systemd sandbox, capability set, no
  Home access, and absence of arbitrary privileged commands;
- a single-file, temporary user import grant with cancellation and no
  directory-level persistence; and
- refreshed Android APK/AAB snapshots when VpnService is introduced and for
  every supported release target, and a cross-platform
  permission-diff and approval gate satisfying acceptance rule 6.

The installed Windows 10 development package subsequently passed independent
other-user and Low Mandatory Level process rejection while the service remained
running; the temporary account and profile were removed. See
`docs/evidence/WIN-P1-005-windows-development-acceptance-2026-07-30.md`.

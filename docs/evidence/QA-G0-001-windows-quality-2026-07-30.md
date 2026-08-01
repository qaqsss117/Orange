# QA-G0-001 Windows Local Quality Evidence (2026-07-30)

## Scope and Result

The complete local Windows quality entry point passed on Windows 10 Pro 22H2,
build 19045. This section remains local development evidence; current remote
CI and non-Windows runner evidence is recorded separately below. `QA-G0-001`
remains in `review` because not all acceptance gates are present.

The acceptance worktree was based on Git revision
`14ca810e0edeed8a8e7222a280a45b02383a8d66` and was intentionally dirty. The
Windows acceptance reports record tracked-diff and untracked-path SHA-256
provenance instead of representing it as a clean revision.

## Current Remote CI Evidence (2026-08-01)

GitHub Actions [`package #45`](https://github.com/qaqsss117/Orange/actions/runs/30694373794)
completed the workspace-quality job and all five platform jobs for clean commit
`b9c1078` in 10 minutes 50 seconds. The Ubuntu `workspace-quality` job passed:

- the workspace toolchain preflight;
- 17 focused Python contract tests for the Android and Apple package auditors,
  resource-manifest checker, and toolchain checker;
- frontend formatting, ESLint, Vitest, TypeScript, and production build;
- Rust workspace formatting, strict Clippy, and tests; and
- formatting, vet, and tests for both Go modules.

The Windows, Linux, macOS, Android, and iOS package jobs also succeeded. The
same run retained the resource-manifest gates, independent Android APK/AAB and
Apple package permission audits, Apple startup smoke checks, and five platform
artifacts. This closes the previously recorded remote-run and non-Windows
runner evidence gaps, but does not imply that the remaining acceptance gates
below exist.

Follow-up GitHub Actions
[`package #48`](https://github.com/qaqsss117/Orange/actions/runs/30695820406)
completed all six jobs for clean commit `b25b05a` in 11 minutes 9 seconds and
produced all five artifacts with zero workflow annotations. The workflow moved
checkout, pnpm setup, Node setup, Go setup, Java setup, Android SDK setup,
cache, and artifact upload to their released Node 24 Action majors. The iOS
runner removed the unused untrusted `aws/tap` before either Xcode build instead
of disabling Homebrew trust checks or trusting the whole tap. This eliminates
the eight Node 20 deprecation annotations and two Homebrew tap annotations seen
before the two focused changes without weakening the existing build gates.

GitHub Actions
[`package #52`](https://github.com/qaqsss117/Orange/actions/runs/30697855702)
proved that the Android release build can be followed by the dedicated native
quality step: the fixed four Kotlin protocol tests, Android lint, APK/AAB
permission audits, and Android artifact upload all passed. The overall run was
not recorded as a successful baseline because App Store Connect rejected the
otherwise successful iOS build with error `90382` after the application reached
its daily upload limit.

The follow-up
[`package #53`](https://github.com/qaqsss117/Orange/actions/runs/30698397597)
completed the workspace-quality job and all five platform jobs for clean commit
`f0912ab` in 13 minutes. It produced five artifacts and zero workflow
annotations. The Android artifact retains the four-test JUnit XML/HTML reports
and lint report alongside the signed release APK/AAB and permission snapshots.
The workspace-quality job now passes 20 CI checker contract tests. App Store
Connect publication occurs only for version tags or an explicit manual input,
and artifact upload precedes external publication; the ordinary main-branch
iOS job therefore retained its signed package and skipped publication instead
of consuming another daily upload slot.

GitHub Actions
[`package #57`](https://github.com/qaqsss117/Orange/actions/runs/30699334068)
completed all six jobs for clean commit `b8254dd` in 12 minutes 8 seconds,
produced five artifacts, and emitted zero workflow annotations. The macOS job
used Xcode's bundled formatter in strict lint mode with the checked-in project
configuration, compiled the independent protocol core with warnings as errors,
and passed the fixed four XCTest cases. Its artifact retains separate formatter
and test logs. The signed iOS package and unsigned simulator shell both linked
the same core through the production Rust/Tauri carrier; package permission
audit, simulator launch, liveness, and first-frame capture also passed. The
workspace-quality suite then contained 23 CI checker contract tests.

GitHub Actions
[`package #59`](https://github.com/qaqsss117/Orange/actions/runs/30700618759)
completed all six jobs for clean commit `900a1c6` in 12 minutes 51 seconds,
produced five artifacts, and emitted zero workflow annotations. After the
release APK/AAB build, the Android job downloaded the pinned ktlint `1.8.0`
all-in-one JAR from Maven Central, verified SHA-256
`369ad2b789f95a011f807e1fcb690ccef80bd7cd014fd139e73ae82dcc0baeab`,
checked nine project-owned Kotlin files, and confirmed that all four generated
copies exactly matched their managed sources. Tauri/Wry `generated/**` output
was explicitly excluded because the upstream generator rewrites it on every
build. The fixed four Kotlin contracts, Android lint, package permission audit,
and artifact upload then passed. The downloaded `orange-android` archive
matched GitHub's digest
`sha256:11f821209b93ff795d95bf2f4fb3bc6e6ac5685ad521114d60914f4f636168c1`
and contained `target/android-native-quality/ktlint.log` with the pinned
identity, exact scope, and `exit_code=0`. The workspace-quality suite now
contains 28 CI checker contract tests.

GitHub Actions
[`package #61`](https://github.com/qaqsss117/Orange/actions/runs/30701586200)
completed all six jobs for clean commit `6f5164e` in 8 minutes 35 seconds,
produced six artifacts, and emitted zero workflow annotations. Before the
remaining checker tests, workspace-quality passed the focused Tauri capability
gate and uploaded its machine-readable report. Six new negative contracts
raised the CI checker suite to 34 tests. The downloaded
`orange-tauri-capabilities` ZIP matched GitHub's digest
`sha256:027d86f99815c8c7c352be3fe3d4f11ae1eea8bc8d67222d752fa3a780f766ec`;
its sole `report.json` had SHA-256
`94f170e041f7272830cd1b7baf6b31a47823f22eadc54751c4870d25fcc14d60`
and recorded a passing exact inventory of five capabilities and 17 generated
permission definitions. This is a focused current Tauri source gate, not a
restoration of the deleted general security workflow.

## Fixed Toolchain

- Node.js `22.23.1`.
- pnpm `11.9.0`.
- Rust and Cargo `1.95.0`.
- Go `1.25.5` from the pinned per-user SDK.
- Python `3.14.2`.

The active dependency directory was rebuilt from `pnpm-lock.yaml`. The previous
mixed pnpm layout was moved outside the repository because local policy blocked
recursive deletion; no old dependency files were included in repository scans.

## Gate Corrections

- The Go Control Plane fixture test validates all five generated route actions:
  sniff, DNS hijack, selector route, explicit node route, and final selector
  route.
- The Data Plane node checker inspects every desktop Tauri handler, including
  separate Windows and non-Windows handlers, while continuing to reject the
  snapshot command from the mobile handler. A split-handler regression test
  locks this behavior.
- Rust 1.95 Clippy drift was removed without changing IPC: `ConnectionMode`
  derives the same `SystemProxy` default, and the Windows connection-mode command
  uses an internal managed runtime context instead of an argument-count lint
  exception.
- Windows mixed readiness now checks ownership of the fixed listener by the
  expected Data Plane PID. TUN routing now sniffs before DNS hijack and uses the
  fixed TLS-validated DoT resolver, removing the reproduced DNS black hole.
- Acceptance provenance supports dirty candidate builds, and Windows verbatim
  service paths are normalized before fixed-root validation.

## Verification

- `python -m unittest discover scripts/security/tests -v`: 202 passed.
- `pnpm check`: 54 frontend tests passed, followed by formatting, lint,
  supply-chain checks, resource checks, TypeScript, and the Vite production
  build.
- Rust workspace formatting, strict Clippy, tests, build, and Data Plane
  application artifact audit passed. The Windows service suite passed 63 tests;
  two explicitly live tests remained ignored by the normal workspace run.
- Both Go modules passed formatting, module verification, vet, and tests with
  `with_quic,with_utls` where required.
- Bootstrap crypto, memory, direct-dial, host, and Tauri bundle checks passed.
- The SBOM contains 810 components and 59 registered resources. License and
  supply-chain policy checks passed for 836 dependencies across 7 ecosystems.
- The unsigned Windows Data Plane core passed with
  `release_allowed=false` and Authenticode status `NotSigned`.
- `python scripts/ci/run.py quality`: all 35 steps passed.

The ignored real-production chain also passed Bootstrap, login, account,
subscription download, VLESS sanitization, mixed HTTPS, and sensitive-data
log isolation. The installed Windows acceptance then passed system proxy and
TUN connectivity for domestic and overseas HTTPS, DNS and exit checks, four
isolated crash cases, independent cross-user and low-integrity IPC rejection,
injected upgrade rollback, baseline-to-candidate upgrade, uninstall, and final
clean state. The combined acceptance result is documented in
`docs/evidence/WIN-P1-005-windows-development-acceptance-2026-07-30.md`.

The 2026-07-31 focused rerun added six Python contract/mutation tests for the
uninstall data choice and native credential cleanup. Default retention,
candidate reinstall, explicit `/DELETEAPPDATA`, credential empty-state probes,
and final clean state passed on the installed unsigned candidate. A separate
mutation locks update mode to `prepare-upgrade` so it cannot perform full
credential cleanup; the top-level quality job remained 35/35.

## Remaining Evidence

`QA-G0-001` remains in `review` until complete permission-diff, SBOM,
dependency-denylist, and secret-scan gates, plus required branch protection,
are present. Formal
signing, a real Windows restart, and Windows 11 remain tracked by their release
and platform slices rather than as missing remote-CI evidence. Service
termination is network-safe, but the existing UI must be restarted after
manually restarting the Service to rebuild its Named Pipe; in-place Service hot
recovery is not claimed.

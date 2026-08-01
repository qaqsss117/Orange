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

`QA-G0-001` remains in `review` until dedicated Kotlin/Swift formatting, lint,
and unit/contract tests; complete permission-diff, SBOM, dependency-denylist,
and secret-scan gates; and required branch protection are present. Formal
signing, a real Windows restart, and Windows 11 remain tracked by their release
and platform slices rather than as missing remote-CI evidence. Service
termination is network-safe, but the existing UI must be restarted after
manually restarting the Service to rebuild its Named Pipe; in-place Service hot
recovery is not claimed.

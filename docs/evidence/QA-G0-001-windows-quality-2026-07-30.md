# QA-G0-001 Windows Local Quality Evidence (2026-07-30)

## Scope and Result

The complete local Windows quality entry point passed on Windows 10 Pro 22H2,
build 19045. This is local development evidence only. `QA-G0-001` is in
`review` pending a remote CI run URL and non-Windows runner evidence.

The worktree was based on Git revision
`14ca810e0edeed8a8e7222a280a45b02383a8d66` and contained the changes described
in this evidence. It was intentionally not represented as a clean revision.

## Fixed Toolchain

- Node.js `22.23.1`
- pnpm `11.9.0`
- Rust and Cargo `1.95.0`
- Go `1.25.5` from the per-user pinned SDK
- Python `3.14.2`

The active dependency directory was rebuilt from `pnpm-lock.yaml`. The previous
mixed pnpm layout was moved outside the repository because local policy blocked
recursive deletion; no old dependency files were included in repository scans.

## Gate Corrections

- The Go Control Plane fixture test now validates all four canonical route
  actions: DNS hijack, selector route, explicit node route, and final selector
  route.
- The Data Plane node checker now inspects every desktop Tauri handler, including
  the separate Windows and non-Windows handlers, while continuing to reject the
  snapshot command from the mobile handler.
- Rust 1.95 Clippy drift was removed without changing IPC: `ConnectionMode`
  derives the same `SystemProxy` default, and the Windows connection-mode command
  uses an internal managed runtime context instead of an argument-count lint
  exception.

## Verification

- `python -m unittest discover scripts/security/tests -v`: 188 passed.
- `pnpm check`: 53 frontend tests passed, followed by formatting, lint,
  supply-chain checks, resource checks, TypeScript, and the Vite production
  build.
- Rust workspace: fmt, `clippy --workspace --all-targets -D warnings`, tests,
  build, and Data Plane application artifact audit passed. Two explicitly live
  Windows tests remained ignored by the normal workspace run.
- Both Go modules passed format, module verification, vet, and tests with
  `with_quic,with_utls` where required.
- Bootstrap crypto, memory, direct-dial, host, and Tauri bundle checks passed;
  the live probe correctly remained `not_run`.
- The SBOM contains 810 components and 59 registered resources. License and
  supply-chain policy checks passed for 836 dependencies across 7 ecosystems.
- The unsigned Windows Data Plane core passed with
  `release_allowed=false` and Authenticode status `NotSigned`.
- `python scripts/ci/run.py quality`: all 35 steps passed.

## Remaining Evidence

The approved `ORANGE_BOOTSTRAP_*` and `ORANGE_E2E_*` environment was absent, so
the ignored production account/subscription chain was not rerun. Remote CI,
signed packages, Windows 11, other platforms, installed-state fault injection,
cross-user/low-integrity checks, and real reboot evidence remain outstanding.

# WIN-P1-005 Windows Development Acceptance Evidence (2026-07-30)

## Scope and Result

The Windows development acceptance workflow is implemented as
`scripts/acceptance/windows-development.ps1`. It supports resumable
`preflight`, `build`, `install`, `proxy`, `tun`, `crash`, `upgrade`, `uninstall`,
and `verify-clean` phases. System-changing phases require an elevated session
and the explicit `-AllowSystemChanges` guard.

This run proves the workflow contract and the initial/final clean-state check on
Windows 10 Pro 22H2 build 19045. It does not prove package installation or live
VPN operation because the approved environment was not present.

## Security and Release Boundary

- Bootstrap build input and E2E credentials are accepted only through named
  process environment variables. They are not command parameters or report
  fields.
- Reports contain redacted observations and hashes, never account values,
  bootstrap JSON, keys, exit-probe content, or subscription content.
- The build phase uses baseline revision `6b23686` as version `0.0.9` and the
  current worktree as version `0.1.0`.
- Both packages are constrained to `unsigned-test-runtime`; Authenticode must be
  `NotSigned`, the signer allowlist remains empty, and `release_allowed` remains
  false.
- Go `1.25.5` is selected from the pinned per-user SDK before any build phase,
  avoiding drift from the machine-wide Go installation.

## Evidence Contract

Every schema-v1 phase report records:

- OS version/build, architecture, Git revision and dirty-state flag;
- Node, pnpm, Rust, Cargo, and Go versions;
- known baseline/candidate package paths and SHA-256 values;
- service policy, WinINET proxy/recovery state, TUN addresses, hashed DNS and
  route summaries, firewall state, loopback listener state, and Orange process
  postconditions; and
- phase-specific redacted observations.

The install phase additionally checks the fixed Program Files binary location,
automatic LocalSystem service, unrestricted service SID, protected identity and
runtime ACLs without broad write grants, installation identity, exact packaged
files, firewall program binding, and the per-installation Named Pipe. Clean-state
checks reject Orange service/process/install-root, proxy journal/RunOnce,
listener, TUN adapter, TUN DNS/routes, or firewall residue.

Ten focused contract tests and the PowerShell parser check passed. The tests
lock the phase set, environment-only secrets, explicit system-change guard,
unsigned release boundary, exact toolchains, shell restrictions, report context,
SCM/ACL/Named Pipe checks, and DNS/route cleanup checks.

## Executed Phase

`verify-clean` passed from an elevated Windows session. The ignored raw artifacts
are stored under `artifacts/acceptance/windows-development`:

- `phase-verify-clean.json` SHA-256:
  `fe939c7dd4ef1ddb63cdbe0c1842f7d0bde8a5919294a08ff0486a86745ff04e`
- `result.json` SHA-256:
  `743c888c0c6938630aa74ed4379b87b9351c23a8dc891273a3dc0bf5bd604992`

The observation confirms no Orange service, installation root, process, proxy
ownership/recovery value, RunOnce value, TUN adapter, TUN DNS/route, firewall
rule, or port 24836 listener. The report records only a hash of the machine DNS
set and no raw server addresses.

## Blocked Phases

All required `ORANGE_BOOTSTRAP_*`, `ORANGE_E2E_EMAIL`,
`ORANGE_E2E_PASSWORD`, and `ORANGE_E2E_IP_CHECK_URL` variables were absent.
Therefore no baseline/candidate package was built or installed, no credential
was entered, and proxy, TUN, crash, upgrade, uninstall, and real reboot phases
were not run.

`UI-P0-005`, `WIN-P0-003`, `WIN-P1-004`, and `WIN-P1-005` remain
`in_progress`. Formal signing, Windows 11, cross-user/low-integrity, upgrade
failure rollback, remote CI, and other platforms remain explicit blockers.

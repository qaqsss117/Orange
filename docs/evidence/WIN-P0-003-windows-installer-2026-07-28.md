# WIN-P0-003 Windows Test Installer Evidence (2026-07-28)

## Scope and Result

This increment wires a native per-machine NSIS test installer to the Windows
SCM lifecycle. It is an unsigned acceptance artifact, not a release package.
`production_backend_release_eligible` and `release_allowed` remain false.

The installer packages the desktop application, Control Plane host, Windows
service, native installer helper, and Data Plane sidecar into the fixed
`Program Files\\Orange` installation root. The helper accepts exactly
`install`, `prepare-upgrade`, or `uninstall`; it does not execute a shell,
PowerShell, `sc.exe`, an arbitrary executable, or caller-provided paths.

## Provisioning Contract

The helper resolves Program Files with `SHGetKnownFolderPath`, canonicalizes
its own executable, and rejects any root other than `Program Files\\Orange` as
well as missing, non-regular, or reparse-point package files. Installation:

- generates or validates one 32-character lowercase cryptographic installation
  identity;
- protects the identity so SYSTEM and administrators have full access while
  the installation user has read access;
- creates a service-owned `data-plane/revisions` runtime with a protected DACL;
- creates `OrangeDataPlane` with a fixed quoted service binary and only the
  installation ID and installation-user SID arguments;
- selects automatic start and unrestricted service SID mode; and
- starts the service, deleting the newly created SCM entry if setup fails.

The NSIS preinstall hook removes the old service before replacing files, the
postinstall hook provisions and starts the new service, and the preuninstall
hook stops/deletes the service and removes runtime and identity state. Any
helper failure aborts the surrounding installer operation. Because SCM service
deletion is asynchronous, the helper also waits with a fixed deadline until the
old service is actually absent before NSIS replaces files or recreates it.

## Initial Machine Evidence

An unsigned test package was installed once on the Windows 11 development
machine under `C:\\Program Files\\Orange`. Read-only inspection confirmed:

- `OrangeDataPlane` was running with automatic start;
- the service SID type was unrestricted;
- SCM held only the fixed service arguments;
- the installation identity contained exactly 32 lowercase hexadecimal bytes;
- the identity ACL was protected and did not grant broad write access; and
- all six expected packaged files were present.

This initial installation preceded the final canonical Program Files source
hardening and production Bearer-token compatibility change. The subsequent
Windows 10 development acceptance upgraded in place, authenticated through the
app, activated a real sanitized subscription revision and TUN, exercised
rollback after an injected post-payload upgrade failure, and then proved clean
uninstall. See
`docs/evidence/WIN-P1-005-windows-development-acceptance-2026-07-30.md`.

## Automated Verification

The security checkers require the fixed native API calls, files, arguments,
actions, Program Files policy, service SID, ACL protection and three NSIS hook
phases. Mutation tests fail if installer service-SID provisioning or uninstall
hook wiring is removed. The Data Plane lifecycle audit now reports
`installer_provisioned: true` while the release gates remain false.

`WIN-P0-003` is now `review`: its implementation and all non-reboot recovery
paths are complete, while its own acceptance rule 4 still requires a real
Windows restart. `WIN-P0-002` and `VPN-P0-002` remain `in_progress` under their
production/cross-platform gates; release signing and Windows 11 continue in
the Windows G0/P1 matrix.

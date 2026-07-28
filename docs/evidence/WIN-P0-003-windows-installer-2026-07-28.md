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
hardening and production Bearer-token compatibility change. It therefore
proves the basic SCM and ACL workflow but is not the final E2E package. The
final package must still be upgraded in place, authenticate through the app,
activate a real sanitized subscription revision and TUN, then uninstall while
proving service, process, runtime, identity, route, and DNS cleanup.

## Automated Verification

The security checkers require the fixed native API calls, files, arguments,
actions, Program Files policy, service SID, ACL protection and three NSIS hook
phases. Mutation tests fail if installer service-SID provisioning or uninstall
hook wiring is removed. The Data Plane lifecycle audit now reports
`installer_provisioned: true` while the release gates remain false.

`WIN-P0-002`, `VPN-P0-002`, and `WIN-P0-003` remain `in_progress` pending the
final upgrade/TUN/uninstall run, independent low-integrity and cross-user
negative tests, a release signer, and the Windows 10/11 compatibility matrix.

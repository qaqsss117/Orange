# WIN-P0-002 Windows Service IPC Evidence (2026-07-28)

## Scope and Result

This increment starts the Windows service slice with a real SCM entry point,
a native Named Pipe transport, a fixed versioned protocol, and two layers of
client authorization. It does not claim a production VPN service: the binary
deliberately hosts `UnconfiguredVpnAdapter`, SCM installation is not wired,
and all release policy remains false.

`WIN-P0-002` is therefore `in_progress`, not `review` or `done`.

## Fixed Service Contract

The `orange-windows-service` crate builds the `orange-service.exe` binary and
a reusable native client. The SCM image accepts exactly these protected
service-configuration arguments:

- `--service`;
- one 32-character lowercase hexadecimal installation ID; and
- one numeric installation-user SID.

Neither the SCM entry point nor IPC accepts an executable path, argument list,
shell, URL, registry path, raw sing-box command, or raw sing-box configuration.
The binary resolves only the fixed sibling `orange-app.exe` client image.

The v1 wire protocol has a 4 KiB frame ceiling and four commands:

| Command | Request data |
| --- | --- |
| `status` | request ID |
| `start` | request ID, configuration revision |
| `stop` | request ID, instance ID |
| `restart` | request ID, instance ID, configuration revision |

Requests deny unknown fields. Schema version, request ID, instance ID, and
configuration revision are checked before an adapter call. Responses carry a
fixed error enum, echo the request ID, and reconstruct `AdapterSnapshot` only
after its state/activity invariants pass.

## Pipe and Identity Boundary

The pipe name is
`\\.\pipe\Orange.DataPlane.<32-lower-hex-installation-id>.v1`. Creation uses
`FILE_FLAG_FIRST_PIPE_INSTANCE`, one maximum instance, and
`PIPE_REJECT_REMOTE_CLIENTS`.

The SDDL has a protected DACL with only:

- SYSTEM;
- deterministic service SID
  `S-1-5-80-1506274412-2088495018-3667606844-4049117896-1250325128`; and
- the exact installation-user SID.

The object also has `S:(ML;;NW;;;ME)`, so a low-integrity caller cannot write
up to the service pipe. Everyone, Authenticated Users, Builtin Users, and
Anonymous are absent.

ACL access is not the only check. Before reading any frame, the server uses
`GetNamedPipeClientProcessId`, opens that process and its primary token, then
requires the expected user SID, at least medium integrity, and the exact
canonical fixed client image. A same-user process from another image is
disconnected without parsing its payload.

## Authoritative State and Tests

`ServiceCommandHandler` owns the native `PlatformVpnAdapter`; a client owns
only a pipe connection. A native test starts state through one client, drops
it, creates a new client, and reads the still-online authoritative snapshot
from the same server handler. This proves the IPC ownership direction without
claiming the not-yet-wired production process backend.

Twelve focused Rust tests passed, including three real Windows Named Pipe
tests:

- restricted-ACL status round trip;
- client destruction/reconstruction with authoritative service state; and
- same-user but unpinned executable rejection after connection.

The remaining tests cover all four command frames, unknown commands and
capability fields, zero/invalid identifiers, schema drift, truncated/empty/
oversized frames, response correlation, snapshot invariants, pipe-name
validation, broad SID rejection, and current-token SID conversion.

`scripts/security/check_windows_service_ipc.py` independently fixes the SCM,
DTO, frame, pipe, ACL, PID/token/image, unavailable-backend, progress, and
release markers. `scripts/security/check_platform_permissions.py` now parses
the exact reviewed `native/windows/service-ipc-policy.json` and rejects any
broader principal or premature installed-service claim.

## Verification

The complete Windows `python scripts/ci/run.py quality` task passed all 28
steps from the beginning after formatting the new policy. It included:

- 77 security tests and the dedicated Windows service audit;
- 20 frontend tests plus the production frontend build;
- workspace formatting, warning-free Clippy, tests, and build;
- Control Plane host/process, Go, and bundle audits;
- an SBOM with 799 components and 53 registered resources;
- 914 locked dependency names across seven ecosystems; and
- the existing Windows Data Plane reproducibility, Authenticode
  classification, version handshake, and loopback HTTP/SOCKS5 smoke.

The development service artifact was built but is not bundled or releasable:

| Artifact | Bytes | SHA-256 | Authenticode |
| --- | ---: | --- | --- |
| `orange-service.exe` | 660,480 | `559c7c10432d67837d1896b32f2ccd1f400463e6546b27f03211ab9bf9bbceb6` | `NotSigned` |

The current source was also copied without Git metadata, generated output,
artifacts, dependencies, or build output to an isolated Ubuntu 24.04 WSL2
directory. Formatting, warning-free Clippy, build, and all six portable
protocol tests passed. Its dependency tree contained no `windows-sys`. The
exact temporary directory was deleted and independently confirmed absent.

## Remaining Acceptance Work

This increment does not qualify the full service slice. The following remain:

- replace `UnconfiguredVpnAdapter` with the fixed service-owned revision store
  and signed sing-box backend, including native `WinVerifyTrust`, signer,
  digest, version, fixed `run -c`, listener, and cleanup checks;
- install/configure the service SID and minimum token privileges through a
  signed installer, then verify start/stop/upgrade/delete and binary ACLs;
- verify service-process crash detection and explicit proxy/route/DNS repair;
- run unauthorized low-integrity and different-user clients as independent
  OS processes rather than relying only on the enforced descriptor plus
  token checks; and
- execute the signed installation matrix on Windows 10 22H2 and the current
  Windows 11 release.

Until those checks pass, `service_configured`, `production_backend_wired`,
`scm_installation_wired`, and `release_allowed` all remain false.

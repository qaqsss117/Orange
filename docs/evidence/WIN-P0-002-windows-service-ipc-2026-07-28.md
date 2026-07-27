# WIN-P0-002 Windows Service IPC Evidence (2026-07-28)

## Scope and Result

This increment now connects the real SCM entry point and restricted Named Pipe
transport to the shared supervisor and a fixed Windows sing-box sidecar
backend. It does not claim a releasable VPN service: no signer is approved,
the development sidecar is unsigned, sanitized revisions are not installed by
a protected workflow, SCM installation is not wired, and release policy
remains false.

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
The binary resolves only the fixed sibling `orange-app.exe` client image. A
request can name only a positive numeric configuration revision; it cannot
name the sidecar or configuration path.

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

## Fixed Sidecar Boundary

`data-plane-runtime-manifest.json` is embedded into `orange-service.exe`, so a
release-signed service also authenticates the manifest it consumes. The strict
manifest fixes:

- sibling `sing-box.exe` and its exact SHA-256;
- sing-box `1.13.14`, Go `1.25.5`, Windows/amd64, CGO disabled, and only
  `with_quic`;
- mandatory Authenticode plus an uppercase SHA-1 signer allowlist;
- no runtime download; and
- `data-plane/revisions/<positive-u64>.json` with a 1 MiB ceiling.

Preflight canonicalizes the installation, artifact, revision root, and exact
revision file, rejecting a reparse-point escape. It hashes both files, runs
native `WinVerifyTrust`, extracts the leaf signing certificate SHA-1 through
the WinTrust provider state, and compares it exactly with the embedded
allowlist. It then permits only `version` and `check -c <fixed-revision>` with
a cleared environment, bounded output, and a 15-second deadline. Version,
Go/platform, tags, and CGO output must match exactly. Artifact and config
hashes are checked after the handshakes and again immediately before spawn.

The only long-lived command is `run -c <fixed-revision>`, also with a cleared
environment and no shell. The child is assigned to a Windows Job Object with
`JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`; the existing `SupervisedVpnAdapter`
owns process state, timeout, crash detection, forced termination, reaping, and
restart serialization. A 300 ms live-process settle currently supplies
readiness. It does not yet prove the TUN interface or a listener is ready and
therefore is not release qualification.

The embedded signer allowlist is deliberately empty and `release_allowed` is
false. Thus the unsigned development sidecar cannot cross production
preflight even though the production backend code path is wired.

## Authoritative State and Tests

`ServiceCommandHandler` owns the native `PlatformVpnAdapter`; a client owns
only a pipe connection. A native test starts state through one client, drops
it, creates a new client, and reads the still-online authoritative snapshot
from the same server handler. This proves the IPC ownership direction.

Twenty-three focused Rust tests passed. Three are real Windows Named Pipe tests:

- restricted-ACL status round trip;
- client destruction/reconstruction with authoritative service state; and
- same-user but unpinned executable rejection after connection.

Eleven sidecar tests cover strict embedded-manifest parsing, exact version output,
fixed revision selection, empty/wrong signer denial, native WinTrust rejection,
reparse-point escape, config mutation between preflight and spawn, supervised
crash detection, fixed lifecycle handoff, bounded handshake timeout/reap, and
native Job Object force/reap.
The remaining tests cover all four command frames, unknown commands and
capability fields, zero/invalid identifiers, schema drift, truncated/empty/
oversized frames, response correlation, snapshot invariants, pipe-name
validation, broad SID rejection, and current-token SID conversion.

`scripts/security/check_windows_service_ipc.py` independently fixes the SCM,
DTO, frame, pipe, ACL, PID/token/image, fixed backend, manifest, command,
WinTrust, hash, Job Object, progress, and release markers.
`scripts/security/check_platform_permissions.py` parses
the exact reviewed `native/windows/service-ipc-policy.json` and rejects any
broader principal or premature installed-service claim.

## Verification

The complete Windows `python scripts/ci/run.py quality` task passed all 28
steps from the beginning after formatting the new policy. It included:

- 79 security tests and the dedicated Windows service audit;
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
| `orange-service.exe` | 1,144,832 | `d760bc02663939445779f07984100ef868991ed390d7ce96ecf612c998ca0153` | `NotSigned` |

The current source was also copied without Git metadata, generated output,
artifacts, dependencies, or build output to an isolated Ubuntu 24.04 WSL2
directory. Its complete 26-step quality task passed 79 security tests, 20
frontend tests, formatting, warning-free workspace Clippy, tests/build, both
Go modules, a Linux SBOM with 806 components and 53 resources, and the six
portable service protocol tests. The desktop app stayed alive for the full
eight-second Xvfb/D-Bus window. The service dependency tree contained no
`windows-sys`. The exact temporary directory
`/home/dev/orange-linux-smoke-20260728041614` was deleted and independently
confirmed absent.

## Remaining Acceptance Work

This increment does not qualify the full service slice. The following remain:

- approve and package a release-signed sing-box artifact, populate the signer
  allowlist/hash, sign the service, and prove the embedded-manifest chain;
- install sanitized revisions into the service-owned store with protected ACLs
  and atomic immutable-revision semantics;
- replace live-process settling with authoritative TUN/listener readiness and
  verify the real signed sidecar through start/restart/crash/stop;
- install/configure the service SID and minimum token privileges through a
  signed installer, then verify start/stop/upgrade/delete and binary ACLs;
- verify service-process crash detection and explicit proxy/route/DNS repair;
- run unauthorized low-integrity and different-user clients as independent
  OS processes rather than relying only on the enforced descriptor plus
  token checks; and
- execute the signed installation matrix on Windows 10 22H2 and the current
  Windows 11 release.

The backend is now wired, but until those checks pass,
`production_backend_release_eligible`, `service_configured`,
`scm_installation_wired`, and `release_allowed` all remain false.

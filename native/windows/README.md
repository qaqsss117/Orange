# Windows Native Boundary

`WIN-P0-002` now provides the first native service boundary in the
`orange-windows-service` crate:

- `orange-service.exe` has a fixed Windows SCM entry point and accepts only an
  installation ID plus installation-user SID from the protected service
  configuration;
- the local Named Pipe uses a one-instance, remote-rejected ACL for SYSTEM,
  the fixed service SID, and that exact user SID, plus a medium-integrity
  mandatory label;
- the server rechecks the connected process ID, primary token user,
  integrity level, and fixed sibling `orange-app.exe` image before reading a
  request; and
- the v1, 4 KiB protocol exposes only status/start/stop/restart with revision
  and instance identifiers.

The reviewed machine policy is `service-ipc-policy.json`. The service now uses
the shared supervisor with the fixed managed sing-box backend and a bounded
Rust stdio client for node selection, readback, delay cancellation, and traffic.
The client is bound to the current revision, supervisor instance, and process
ID. Policy therefore records `production_backend_wired: true` while retaining
`production_backend_release_eligible: false` and `scm_installation_wired: false`.
The unsigned development artifact and empty signer allowlist still fail closed.
Restricted Named Pipe node DTOs, installer lifecycle, system proxy restoration,
and signed real-TUN evidence remain later Windows work.

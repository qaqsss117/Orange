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

The reviewed machine policy is `service-ipc-policy.json`. It intentionally
records `production_backend_wired: false` and `scm_installation_wired: false`.
The service binary therefore uses `UnconfiguredVpnAdapter` until the fixed,
signed sing-box backend and installer lifecycle pass their own review. System
proxy restoration and optional TUN integration remain later Windows slices.

# Windows Native Boundary

The `orange-windows-service` crate provides the native service boundary:

- `orange-service.exe` has a fixed Windows SCM entry point. Legacy unpackaged
  installs pass an installation ID plus installation-user SID; the MSIX
  packaged-service entry point derives the installation ID from its package
  directory and uses the package-pinned client image;
- the local Named Pipe uses a one-instance, remote-rejected ACL for SYSTEM,
  the fixed service SID, and either the configured user SID or local users for
  the package-pinned client, plus a medium-integrity mandatory label;
- the server rechecks the connected process ID, primary token user,
  integrity level, and fixed sibling `orange-app.exe` image before reading a
  request; and
- the v1, 4 KiB protocol exposes ten fixed lifecycle and node commands. Delay
  probes use begin/poll/cancel across separate connections, with 8 running
  probes, 32 retained records, and five-second result retention.

The reviewed machine policy is `service-ipc-policy.json`. The service now uses
the shared supervisor with the fixed managed sing-box backend and a bounded
Rust stdio client for node selection, readback, delay cancellation, and traffic.
The client is bound to the current revision, supervisor instance, and process
ID. A native helper and the legacy per-machine installer path install, start,
stop, upgrade, and remove the fixed SCM service. MSIX uses the Windows packaged
service registration and initializes the firewall/runtime from the service
entry point. Policy therefore records `production_backend_wired: true`,
`scm_installation_wired: true`, and `service_configured: true`, while retaining
`production_backend_release_eligible: false` and `release_allowed: false`.

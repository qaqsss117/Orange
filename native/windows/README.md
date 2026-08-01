# Windows Native Boundary

The `orange-windows-service` crate provides the native service boundary:

- `orange-service.exe` has a fixed Windows SCM entry point and accepts only an
  installation ID plus installation-user SID from the protected service
  configuration;
- the local Named Pipe uses a one-instance, remote-rejected ACL for SYSTEM,
  the fixed service SID, and that exact user SID, plus a medium-integrity
  mandatory label;
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
ID. A native helper and per-machine NSIS hooks now install, start, stop, upgrade,
and remove the fixed SCM service. They provision the installation identity and
service-owned runtime under the canonical `Program Files\\Orange` root with
protected ACLs. Policy therefore records `production_backend_wired: true`,
`scm_installation_wired: true`, and `service_configured: true`, while retaining
`production_backend_release_eligible: false` and `release_allowed: false`.

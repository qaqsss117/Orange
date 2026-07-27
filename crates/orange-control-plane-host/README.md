# Orange Control Plane Host

This crate owns the desktop lifecycle of the no-listener Go Control Plane sidecar.

- The production API resolves only the fixed `orange-control-plane` sibling of the current application executable and verifies it against the SHA-256 embedded at build time before spawn. Arbitrary path/argument construction exists only behind the `test-helper` feature. Production builds expose no sidecar argument or environment injection API, invoke no shell, clear the inherited environment, and hide the Windows console.
- A `SecretBuffer` is serialized directly into the versioned length-prefixed `init` frame. The source config and serialized JSON/base64 buffers are zeroized immediately after the write, including initialization failures and panic paths.
- The host waits for `ready`, dispatches concurrent responses by generated request ID, sends `cancel` on explicit cancellation/drop/timeout, and broadcasts a stable redacted error if the reader or child exits.
- Closing stdin requests native sing-box release and waits for child exit. A bounded timeout kills and reaps a stuck child; dropping the host performs the same idempotent cleanup.
- The retained API host allowlist is cleared on failure and close. Request, response, frame, credential, and bootstrap buffers use zeroizing cleanup.

The Tauri desktop shell owns one optional host through managed state. Android and iOS do not compile this process host; their platform G0 work will use an embedded native boundary instead.

## Verification

```text
python scripts/ci/check_control_plane_host.py
```

The audit uses a feature-gated Rust test sidecar for response dispatch, cancellation, timeouts, rejection, unexpected exit, graceful EOF, and forced reap paths. It also initializes and closes the production Go sidecar with a real decrypted development bootstrap buffer.

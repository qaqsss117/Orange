# Orange Control Plane Host

This crate owns the desktop lifecycle of the no-listener Go Control Plane sidecar.

- The production API resolves only the fixed `orange-control-plane` sibling of the current application executable and verifies it against the SHA-256 embedded at build time before spawn. It exposes no sidecar argument or environment injection API, invokes no shell, clears the inherited environment, and hides the Windows console.
- A `SecretBuffer` is serialized directly into the versioned length-prefixed `init` frame. The source config and serialized JSON/base64 buffers are zeroized immediately after the write, including initialization failures and panic paths.
- The host waits for `ready`, dispatches concurrent responses by generated request ID, sends `cancel` on explicit cancellation/drop/timeout, and broadcasts a stable redacted error if the reader or child exits.
- A structured request may carry one optional bounded access-token byte buffer. The host rejects empty, oversized, or non-Bearer-safe values before writing a frame, never accepts an arbitrary header map, and exposes only an `authenticated` boolean in `Debug` output.
- Closing stdin requests native sing-box release and waits for child exit. A bounded timeout kills and reaps a stuck child; dropping the host performs the same idempotent cleanup.
- The retained API host allowlist is cleared on failure and close. Request, response, frame, credential, access-token, and bootstrap buffers use zeroizing cleanup.

The Tauri desktop shell owns one optional host through managed state. Android and iOS do not compile this process host; their native integration uses a separate embedded boundary.

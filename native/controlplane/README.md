# Orange Control Plane Bridge

This module is the narrow, no-listener Go boundary for `BOOT-G0-003`.

## Runtime boundary

- sing-box is pinned to `v1.13.14` in `go.mod` and `toolchains.toml`.
- The client creates exactly one Shadowsocks, Trojan, or Hysteria2 outbound. `route.final` points to that tag and no direct fallback is configured.
- The Control Plane has no inbound, system proxy, or TUN configuration.
- HTTPS uses the selected sing-box outbound as `http.Transport.DialContext`; TLS verification remains enabled with a minimum of TLS 1.2.
- Requests are limited to structured GET/POST fields, an API host allowlist, fixed HTTPS port 443, bounded bodies, bounded responses, total timeouts, and bounded concurrency.

## stdio protocol

`cmd/orange-control-plane` exposes the bridge without opening a control socket. Each JSON message is prefixed by a four-byte big-endian length and is capped at 2 MiB.

1. The host sends one `init` frame containing version `1`, the outbound, API hosts, and limits. The Go `credential` byte slice is represented as a base64 JSON string.
2. The helper returns `ready` or a redacted `error` frame.
3. The host sends `request` frames with unique IDs and may send `cancel` for an active ID.
4. The helper returns `response` or `error`; errors contain only a stable `ErrorCode`.

The protocol never accepts a full URL, an Authorization header, a local path, a shell command, or network-listener configuration. EOF cancels active requests and closes the sing-box instance.

## Verification

```text
python scripts/ci/check_go.py
```

The tests run GET and POST through a real local Shadowsocks tunnel, then cover blocked-proxy fail-closed behavior, TLS and DNS failures, timeout, cancellation, concurrency, response limits, strict stdio frames, and a Windows process listener snapshot.

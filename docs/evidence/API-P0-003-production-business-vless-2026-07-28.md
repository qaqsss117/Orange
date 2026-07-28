# API-P0-003 Production Business And VLESS Evidence

Date: 2026-07-28

## Scope

This evidence covers a live Windows desktop path through the encrypted
production Bootstrap, the Rust Control Plane host, and the Go direct-dial
sidecar. Credentials were supplied only as process environment variables. The
probe did not persist or print response bodies, account identifiers, tokens,
subscription URLs, UUIDs, server addresses, Reality keys, SNI values, or node
names.

## Observed Contract

- `GET /api/v1/guest/comm/config`: HTTP 200, strict JSON production envelope.
- `POST /api/v1/passport/auth/login`: HTTP 200, strict JSON production envelope.
- `GET /api/v1/user/info`: HTTP 200 with the accepted native Bearer credential.
- `GET /api/v1/user/getSubscribe`: HTTP 200 with the same credential.
- All four envelopes had the exact top-level keys `data`, `error`, `message`,
  and `status`; `status` was `success`, `error` was null, and the message was
  control-character free.
- The config `app_url` host was already present in the decrypted Bootstrap API
  host allowlist.
- The production registration endpoint was not tested and is not claimed.
  Production config therefore makes registration fail closed without a network
  request.

## Subscription Payload

The sensitive subscription URL passed HTTPS, port 443, credential, fragment,
and Bootstrap host checks before the native Control Plane downloaded it. The
response was HTTP 200, `text/plain; charset=utf-8`, and 7,072 bytes. Strict
Base64 decoding produced 5,304 UTF-8 bytes and 18 VLESS URIs.

All 18 entries used the reviewed closed set:

- Reality security;
- TCP transport;
- `xtls-rprx-vision` flow;
- Chrome client fingerprint;
- `mode=multi` and canonical `spx=/`;
- matching server-name fields;
- a canonical 32-byte Base64URL Reality public key.

No Shadowsocks, Trojan, Hysteria2, JSON, YAML, invalid URI, protocol downgrade,
or unexpected query-key variant was observed.

## Implementation And Tests

`orange-platform` now strictly maps the observed production config, login,
account, and subscription envelopes into the existing public DTOs. Sensitive
credentials remain native and zeroizing. The Base64 VLESS sanitizer rebuilds a
bounded sing-box configuration instead of trusting server-supplied JSON, and
the public selector catalog exposes only stable IDs and the `vless` protocol
family.

The production Rust client now owns the same download boundary used by the
probe. It reloads the URL only from native secure storage, parses it without
exposing it to the WebView, requires HTTPS on port 443 with no userinfo or
fragment, checks the host against the live Bootstrap allowlist, and passes only
the validated host plus path/query to the existing Control Plane. Redirects and
non-success responses retain the fixed business error mapping, and successful
response bytes are returned in a zeroizing buffer. The desktop adapter uses
`ControlPlaneRequest::get`; it does not create a second HTTP client or a direct
fallback.

The focused verification passed:

- 149 `orange-platform` tests, including accepted, rejected, missing,
  redirect, timeout, and redacted-debug subscription download cases;
- 24 `orange-domain` tests;
- 46 `orange-windows-service` tests, with the existing real-artifact audit test
  ignored by design;
- tagged Go data-plane tests and strict config/node policy checks;
- a reproducible Windows data-plane build audit.

The focused native download increment also passed Windows Tauri compilation,
strict Clippy, the control-egress audit, and the subscription-pipeline audit.
The audited data-plane binary was unsigned, so `release_allowed` remained
false. This evidence does not claim application-driven subscription activation,
SCM installation, signed release eligibility, real TUN connectivity, or
cross-platform completion.

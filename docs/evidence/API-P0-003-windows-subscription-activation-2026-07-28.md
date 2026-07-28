# API-P0-003 Windows Subscription Activation Evidence

Date: 2026-07-28

## Scope

This increment connects the already-audited production subscription metadata
and native download client to the Windows revision pipeline. It does not add a
new WebView command or expose subscription content, URLs, node addresses, or
selector configuration to React.

## Native Sequence

For an active or trial subscription, Windows login and explicit subscription
refresh execute this native-only sequence:

1. Refresh the strict public subscription metadata DTO.
2. Load the subscription credential from the desktop secret backend.
3. Download through the existing allowlisted Control Plane transport.
4. Keep the body in a zeroizing buffer and sanitize Base64 VLESS input into the
   closed TUN template.
5. Derive a positive revision greater than both wall-clock milliseconds and
   every persisted ledger revision.
6. Apply the existing journal, candidate-health, activation, commit, and public
   selector-runtime transaction.

Inactive, expired, exhausted, and unknown statuses return their public metadata
without attempting a body download. Every native failure maps to the fixed
public `subscription` error code.

## Verification

- 18 `orange-app` Rust tests passed, including provisioned/unprovisioned runtime
  ownership and monotonic persisted revision tests.
- 149 `orange-platform` Rust tests passed.
- Subscription and node-runtime policy audits report the Windows production
  backend and activation source as wired.
- 27 subscription/node security mutation tests passed.
- No capability, command name, request field, network client, log sink, browser
  storage, or WebView event emitter was added.

Real installed login, network target health, TUN ownership, external
connectivity, stop, and uninstall results remain installer E2E work and are not
claimed here.

# Orange Business API Contract

This directory is a clean-room, development-only equivalent contract for
`API-G0-001`. It is not copied from the untrusted reference application and is
not an approved production OpenAPI document.

## Boundary

- `business-api.schema.v1.json` defines eleven semantic operations: config,
  login, registration, account, subscription, plans, orders, payment, invite,
  tickets, and update.
- Wire objects are strict and reject unknown fields. Unknown status strings map
  to the typed `unknown` state so a future server value cannot be mistaken for a
  known actionable state.
- Timestamps are non-negative Unix milliseconds. Money is a non-negative
  integer in the currency's minor unit. Traffic is bytes. All integers stay at
  or below JavaScript's exact integer maximum.
- Nullable fields are explicit in the schema and fixtures; absence and `null`
  are not interchangeable.

## Sensitive Field Mapping

`field-mapping.v1.json` separates Rust wire DTOs from WebView-safe public DTOs.
Access and refresh tokens go only to the Rust secure store. Subscription
credentials go only to the native Data Plane pipeline. A payment URL must be
validated in Rust against HTTPS and the payment allowlist; only its approved
target host may be projected publicly. None of those raw fields exists in the
TypeScript production DTO module.

The wire fixture retains protocol field names but uses explicit
`<redacted:...>` values. Emails use the reserved `.invalid` domain, order and
invite identifiers are redaction markers, and no fixture contains an HTTP URL.
The public fixture removes raw credentials and payment URLs entirely.

## Compatibility Policy

Structural changes require a schema version and fixture update. Status enums
are the only forward-compatible exception: unknown strings map to `unknown`.
The failure fixture fixes empty 2xx, 4xx, 5xx, non-JSON, timeout, and structural
schema-drift behavior. The production contract and real backend samples must be
reviewed and substituted before release; this development schema is marked
`releaseAllowed: false`.

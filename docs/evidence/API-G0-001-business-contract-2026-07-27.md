# API-G0-001 Business Contract And Redaction Evidence

- Date: 2026-07-27
- Hosts: Windows 11 amd64, Ubuntu 24.04.4 under WSL2, and Android 16 / API 36
- Slice status: `in_progress`

## Qualification Scope

This increment establishes a clean-room, development-only equivalent business
contract. It does not claim access to an approved production OpenAPI document,
production backend samples, or confirmed production error semantics. The
schema is explicitly marked `environment: development` and
`releaseAllowed: false`.

No Tauri command, WebView capability, direct network client, runtime log sink,
or platform permission was added. The TypeScript module parses only native
public projections and cannot initiate a request. Rust owns all sensitive wire
DTOs for the later typed business client.

## Versioned Contract And Fixtures

`business-api.schema.v1.json` registers exactly eleven semantic operations:
config, login, register, account, subscription, plans, orders, payment, invite,
tickets, and update. Every object schema is closed with
`additionalProperties: false`; every nullable property remains required so
missing and `null` are not interchangeable.

Timestamps are non-negative Unix milliseconds, money is a non-negative integer
in minor units, traffic is bytes, and every integer is capped at JavaScript's
exact maximum of `9_007_199_254_740_991`. Currency codes are exactly three
uppercase ASCII letters. Plans and tickets are capped at 256 items.

Account, subscription, order, payment, and ticket statuses have fixed known
registries. An unknown string maps to the typed `Unknown`/`unknown` state;
unknown structural fields still fail closed. This keeps a future status from
being interpreted as an existing actionable state without opening the object
contract.

The wire and public success fixtures cover all eleven operations. Fixture
emails use the reserved `.invalid` domain. Authentication values, subscription
credentials, order IDs, invite codes, and payment targets use exact
`<redacted:...>` markers, and neither fixture contains an HTTP URL. The public
fixture removes raw authentication credentials, subscription credentials, and
the payment URL; payment exposes only the approved `targetHost` projection.

`field-mapping.v1.json` fixes nine native-to-public policies, including secure
store ownership for both login and registration tokens, Data Plane-only
subscription credentials, and allowlisted payment-host projection. The six
failure fixtures fix empty 2xx, 4xx, 5xx, non-JSON, transport timeout, and
structural schema-drift results.

## Rust And TypeScript Boundaries

`orange-domain` implements strict serde DTOs for the complete wire and public
models. Login and registration inputs, credential bundles, subscription wire
responses, and payment wire responses zeroize sensitive strings on explicit
clear and drop. Their custom `Debug` implementations disclose only lengths,
presence, status, and already-public timing fields. Contract tests prove that
wire secrets and fixture email addresses do not appear in formatted output.
All 18 production DTO `schemaVersion` fields reject values other than v1 during
deserialization rather than relying on a later caller check.

The TypeScript production module defines only public response interfaces. Its
parser reconstructs every object after checking the exact key set, schema
version, nullable value, safe integer, currency, host, string, and bounded-array
rules. It contains no password, access/refresh token, subscription credential,
payment URL, direct networking, `any`, exported map, or index-signature DTO.
Tests parse the full public fixture, compare operation and status registries to
the canonical schema, reject extra/missing fields and invalid units, and map
all five future statuses to `unknown`.

## Static And Focused Verification

```text
cargo test -p orange-domain
19 passed

pnpm exec vitest run src/businessApi.test.ts
6 passed

python -m unittest scripts.security.tests.test_business_api_contract -v
6 passed

python scripts/security/check_business_api_contract.py
11 operations; 6 failure cases; 9 field mappings; 0 errors
```

The new CI audit also rejects release-enabling the development schema, missing
or reordered operations, open or implicitly optional objects, unit/status
drift, altered sensitive-field ownership, an incomplete failure matrix, raw
fixture secrets, real email domains, URLs, sensitive public fields, unsafe
TypeScript DTO maps, and direct frontend networking.

## Windows And Linux Gates

Windows `python scripts/ci/run.py quality` passed all 22 steps with 312 source
files and 112 text files scanned, 51 security tests, 16 frontend tests, 87 Rust
workspace tests, seven separate Control Plane host process tests, a
784-component SBOM, and 53 registered resources. The business contract audit
reported 11 operations, 6 failure cases, 9 field mappings, and zero errors.

The four-step Windows desktop-shell task passed. `target/debug/orange-app.exe`
was 12,735,488 bytes with SHA-256
`47290722d79aa0a58fedb3a86ae6bb41fd9efed223c09e08cc20cf884a825dfd`,
remained alive for the eight-second startup window, and left no new Control
Plane sidecar after shutdown.

An isolated Ubuntu 24.04.4 WSL2 copy at
`/home/dev/orange-linux-smoke-20260727232106` excluded `.git`, dependency
directories, artifacts, build outputs, generated mobile files, and Rust
targets. Its quality task passed all 22 steps with 310 source files and 112
text files scanned, 51 security tests, 16 frontend tests, 87 passing Rust tests
and one explicitly unavailable native-secret-store test ignored, a
790-component SBOM, and 53 resources.

The Linux four-step desktop-shell task also passed. The application was
203,515,808 bytes with SHA-256
`a1ab14034d1062b4970bda3b9ff40f2ecc3cdeca4402100ef6ef381a73467037`;
the 22,666,517-byte sidecar had SHA-256
`dd2a6d3954b59d8477e59f83f873ff5eb0ac5359c62eaea4c9d344d7525662e0`.
The application stayed alive for the full eight-second Xvfb/D-Bus window and
left no sidecar. The exact isolated copy was then deleted and confirmed absent.

## Android Gate

`python scripts/ci/run.py android-shell` regenerated the controlled Android
project and passed all eight steps: four Rust target installations, aarch64
Rust/Tauri build, exact merged-permission audit, Android lint, instrumentation
assembly, and debug-artifact recording. A separate x86_64 build compiled the
changed domain and platform crates for the running emulator.

The final APK permission snapshot contains only `INTERNET`, the app-private
dynamic-receiver permission, the `DUMP`-guarded profile receiver, and implied
faketouch. It has no FileProvider or privacy permission.

Android 16 / API 36 on x86_64 installed and launched the current
124,828,831-byte application APK with SHA-256
`f9d92bceea0a7c02dbe90be741807b93cdf14992e2873472d7cca4ed6fce283b`.
The 625,024-byte instrumentation APK had SHA-256
`3d252e98529ca133b77b026bcd7af6dc7215fff181a5583a7847d145ac9790ec`.
The device reported `OK (4 tests)` for the current Rust/Kotlin/Keystore bridge,
and an independent package query confirmed zero matching debug packages after
cleanup.

## Remaining Acceptance Work

This slice remains `in_progress`, not `review` or `done`:

- an approved production OpenAPI/equivalent contract is unavailable;
- real desensitized backend samples and integration results are unavailable;
- production status, nullability, error-code, and unit semantics have not been
  confirmed against the backend; and
- formal dependencies `ARC-G0-002` and `SEC-G0-003` are not complete.

The development contract must be replaced or reconciled through an explicit
review when those inputs arrive. Renaming the development schema or guessing
production fields cannot close these gaps.

# BOOT-G0-001 Bootstrap Envelope Evidence

- Date: 2026-07-27
- Host: Windows development host
- Slice status: `review`

## Implemented Boundary

- `contracts/bootstrap/bootstrap.schema.json` defines the strict version 1 plaintext model and rejects unknown fields.
- The model contains only candidate outbounds, failover limits, startup DNS, API hosts, configuration version, and expiry.
- `crates/orange-bootstrap` creates an `ORNGBTP1` binary envelope with XChaCha20-Poly1305 and a random 24-byte nonce.
- Authenticated data binds schema, algorithm, channel, product version, configuration version, expiry, and key ID.
- `tools/bootstrap-crypto` reads the 32-byte key only from `ORANGE_BOOTSTRAP_BUILD_KEY_HEX` and reads plaintext only from stdin.
- Key, stdin plaintext, candidate credentials, and serialized plaintext buffers use zeroize cleanup.
- `scripts/ci/run.py bootstrap` generates a non-routable development `bootstrap.enc`, manifest, and hash-only report.
- `scripts/ci/run.py bootstrap-release` is fail-closed until production configuration and all `ORANGE_BOOTSTRAP_*` variables are supplied.

The development fixture uses only `.invalid` nodes and cannot access a backend. No production node, credential, key, user token, URL, or arbitrary sing-box object is checked in.

## Failure And Rotation Tests

The Rust tests cover valid authenticated round trip, random nonce uniqueness, channel/version/key-ID authentication, wrong key, truncation, ciphertext tampering, old schema, expiry, unknown `userToken`, schema alignment, key parsing, and manifest/plaintext leakage checks.

```text
cargo test -p orange-bootstrap -p orange-bootstrap-crypto
9 passed, 0 failed

python scripts/ci/run.py bootstrap
passed; two encryptions produced distinct SHA-256 values
```

Latest development artifacts from the full quality run:

| Artifact | SHA-256 |
| --- | --- |
| `artifacts/bootstrap/bootstrap.enc` | `743654b216c6984ed2f770ec8c7348370f8016e1019266086355d1cad9dd7cee` |
| nonce-check ciphertext | `1f4138114fb16c605224555cfaa5c8007fefa2b01af6b5b3e9415cbdc527ccaa` |
| non-sensitive manifest | `2e8d2b11b7d434fac37e9bd1a629f16c67ad8e1857de0d6608c93ad74e76fa9b` |

## Full Gates

`python scripts/ci/run.py quality` passed all 14 steps:

- source isolation and 22 security unit tests;
- Prettier, ESLint, 6 Vitest tests, TypeScript, and Vite build;
- Rust formatting, Clippy with warnings denied, 19 workspace tests, and full workspace build;
- bootstrap CLI process check and Go check;
- CycloneDX SBOM with 690 components and 53 resources;
- 676 dependency names across 7 ecosystems and 75 approved configured URLs.

The build configuration now also fails when any `GOPROXY` declaration contains a `direct` fallback. Domestic mirror verification passes with rsproxy, goproxy.cn, npmmirror, Aliyun, Tencent, and Tsinghua endpoints.

## Remaining Production Configuration

The local implementation meets the package-format and encryption acceptance rules, but the slice remains `review`: production `bootstrap.enc` cannot be generated until approved nodes, API hosts, expiry/rotation policy, channel metadata, and the Gitee Go secret values are configured. Formal dependencies `ARC-G0-001` and `SEC-G0-004` also remain `blocked`/`review` pending deferred platform evidence.

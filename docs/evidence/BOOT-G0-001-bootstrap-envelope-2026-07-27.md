# BOOT-G0-001 Bootstrap Envelope Evidence

- Date: 2026-07-27; managed CI follow-up: 2026-08-01
- Hosts: Windows development host and GitHub-hosted Windows, Linux, and macOS runners
- Slice status: `done`

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

## Managed CI Acceptance

The approved nodes, API host, expiry/channel metadata, and rotated production
envelope were subsequently supplied through ignored local inputs and passed
authenticated build injection plus a real desktop sidecar request.

The packaging carrier was later migrated from the unverified Gitee adapter to
GitHub Actions. The retained remote record is
[`package #26`](https://github.com/qaqsss117/Orange/actions/runs/30683272345),
attempt 3, for commit
`d3c3aaa3cefae79982713e3db7c0d6cd20004020`. Windows, Linux, and macOS each
completed the `Build production bootstrap` step with the build key and
production configuration supplied only through repository Secrets. All five
platform jobs completed successfully and retained these workflow artifacts:

| Artifact | Bytes | GitHub artifact SHA-256 |
| --- | ---: | --- |
| `orange-windows` | 19,771,694 | `eba0932b5e4849e23b57b61226da39213dce8c7aabb2f3212e4120865f178395` |
| `orange-linux` | 125,874,081 | `ba054e39ca70fd55b844f30955609cd62878f1732beed2c5dc0f0c3f5d9c7f3f` |
| `orange-macos` | 15,015,744 | `36c4a1d33a987bcdc3df478e5c942c3cb41c58527e2929556a4cf6b786b58985` |
| `orange-android` | 26,140,042 | `8b6c219c22b29616c9a6380ba7b35b6510df0ba9ba581a80080d1327deff9034` |
| `orange-ios` | 3,126,049 | `0cf67e4b86af0b8cf3bf4cddc62b6befce29d992a14c8a8de569ebdb6a817ef0` |

The digests above identify the retained GitHub artifact archives; they do not
replace the non-sensitive Bootstrap manifest's ciphertext digest. The workflow
does not print Secret values or decrypted configuration, and no key material is
present in the repository or artifact metadata.

The previous rule 3 gap is closed by this managed Secret injection and remote
record. Together with the format, rotation, authentication, rejection, and
leakage evidence above, all six acceptance rules are satisfied and
`BOOT-G0-001` is `done`. `ARC-G0-001` continues to track its own shell-launch
and quality-gate requirements and is not changed by this result.

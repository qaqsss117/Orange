# BOOT-G0-002 Memory Decrypt And Zeroize Evidence

- Date: 2026-07-27
- Host: Windows development host
- Slice status: `review`

## Implemented Boundary

- `crates/orange-bootstrap` now exposes a production `decrypt(envelope, manifest, key, now_unix)` that authenticates the `ORNGBTP1` envelope, enforces the manifest/envelope agreement, and validates the plaintext schema and expiry before returning.
- Decryption result is returned only through a controlled `SecretBuffer`; it is never converted into a freely cloneable global `String`. Access is scoped through `consume<R>(self, closure)` and per-candidate `with_credential`.
- `BootstrapKey` and the intermediate `PlaintextBuffer` wrap their bytes in `Zeroizing`; the plaintext buffer is zeroized before `decrypt` returns the secret to the caller.
- `SecretBuffer` zeroizes on `consume`, on `Drop`, and during unwind when the consumer panics — the error and panic paths clear the secret exactly once (acceptance rule 4).
- `Debug` for `BootstrapKey`, `BootstrapConfig`, and `SecretBuffer` is redacted: keys show `[REDACTED]`, the buffer shows only a `loaded`/`cleared` state, and configs show counts, never node URIs or credentials (acceptance rules 1, 6).
- Authentication, manifest, schema, and expiry failures are fail-closed via `BootstrapDecryptError`; no plaintext is written to files, logs, or panic messages (acceptance rule 2).

## Failure, Zeroize, And Redaction Tests

The Rust tests cover the authenticated round trip, plaintext-buffer zeroize before return, secret-buffer zeroize on consume/drop, zeroize during a panicking consumer, wrong key / truncation / tampering rejection, channel/version/key-ID authentication, old schema / expiry / unknown-field rejection, key parsing, schema alignment, and Debug/redaction leakage checks. Counters use per-test-thread `thread_local` cells so parallel `decrypt` callers do not disturb the observed deltas.

```text
cargo test -p orange-bootstrap
13 passed, 0 failed
```

## Build Artifact Leak Scan (Rule 5)

`scripts/ci/check_bootstrap_memory.py` derives the forbidden node servers, TLS names, credentials, and API hosts from the development fixture and scans the built production artifacts. No forbidden token appears in either binary.

```text
python scripts/ci/check_bootstrap_memory.py
passed; 5 forbidden tokens checked
```

| Artifact | Bytes | Forbidden token present |
| --- | ---: | --- |
| `orange-bootstrap-crypto.exe` | 992256 | none |
| `orange-app.exe` | 12094464 | none |

The development fixture uses only `.invalid` nodes and placeholder credentials, so a positive hit would indicate a hardcoded node rather than a real leak; the scan is wired into `bootstrap_steps` so any future hardcoded secret fails CI.

## Full Gates

`python scripts/ci/run.py quality` passed all 15 steps:

- source isolation and 22 security unit tests;
- Prettier, ESLint, 6 Vitest tests, TypeScript, and Vite build;
- Rust formatting, Clippy with warnings denied, 24 workspace tests, and full workspace build;
- bootstrap crypto check and the new bootstrap memory leak scan;
- Go check;
- CycloneDX SBOM with 690 components and 53 resources, license and supply-chain validation.

Latest development artifacts from the bootstrap job:

| Artifact | SHA-256 |
| --- | --- |
| `artifacts/bootstrap/bootstrap.enc` | `2c8f2c4d8f4c79c2adbb64b9357ab7308098cf949911fbfab753b1adf2179177` |
| nonce-check ciphertext | `fff0a6138cd017ae698c3bf4a26c44286bed443f952ca5b92d38349b7ceaa997` |
| non-sensitive manifest | `a787192a6c01a1b876f258db287f449a1e3cf9cc4a47f2156133068ff2ec45e7` |

## Remaining Production Configuration

The local implementation meets the in-memory decrypt and zeroize acceptance rules, but the slice remains `review`: the Go/libbox handoff and native-copy release in rule 3 land with `BOOT-G0-003` (direct-dial PoC), and a real `bootstrap.enc` still requires approved production nodes, API hosts, expiry/rotation policy, and the Gitee Go secret values. Formal dependency `BOOT-G0-001` also remains `review` pending production resource generation.

# BOOT-G0-002 Memory Decrypt And Zeroize Evidence

- Date: 2026-07-27
- Host: Windows development host
- Slice status: `review`

## Implemented Boundary

- `crates/orange-bootstrap` now exposes a production `decrypt(envelope, manifest, key, now_unix)` that authenticates the `ORNGBTP1` envelope, enforces the manifest/envelope agreement, and validates the plaintext schema and expiry before returning.
- Decryption result is returned only through a controlled `SecretBuffer`; it is never converted into a freely cloneable global `String`. Access is scoped through `consume<R>(self, closure)`, panic-safe `consume_in_place<R>(&mut self, closure)`, and per-candidate `with_credential`.
- `BootstrapKey` and the intermediate `PlaintextBuffer` wrap their bytes in `Zeroizing`; the plaintext buffer is zeroized before `decrypt` returns the secret to the caller.
- `SecretBuffer` zeroizes on `consume`, `consume_in_place`, and `Drop`, including unwind when the consumer panics. The caller can observe `is_cleared() == true` after both successful and panicking in-place handoff (acceptance rule 4).
- `Debug` for `BootstrapKey`, `BootstrapConfig`, and `SecretBuffer` is redacted: keys show `[REDACTED]`, the buffer shows only a `loaded`/`cleared` state, and configs show counts, never node URIs or credentials (acceptance rules 1, 6).
- Authentication, manifest, schema, and expiry failures are fail-closed via `BootstrapDecryptError`; no plaintext is written to files, logs, or panic messages (acceptance rule 2).

## Failure, Zeroize, And Redaction Tests

The Rust tests cover the authenticated round trip, plaintext-buffer zeroize before return, secret-buffer zeroize on consume/in-place consume/drop, observable zeroize during a panicking consumer, wrong key / truncation / tampering rejection, channel/version/key-ID authentication, old schema / expiry / unknown-field rejection, key parsing, schema alignment, and Debug/redaction leakage checks. Counters use per-test-thread `thread_local` cells so parallel `decrypt` callers do not disturb the observed deltas.

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
| `orange-app.exe` | 12225024 | none |

The development fixture uses only `.invalid` nodes and placeholder credentials, so a positive hit would indicate a hardcoded node rather than a real leak; the scan is wired into `bootstrap_steps` so any future hardcoded secret fails CI.

## Native Handoff (Rule 3)

`scripts/ci/check_control_plane_host.py` decrypts the development envelope into a `SecretBuffer`, consumes it in place to write the production Go sidecar's `init` frame, and asserts that the Rust buffer is cleared immediately after the frame is produced. The real sidecar returns `ready`; closing stdin then exercises EOF shutdown and releases the process-owned sing-box instance. Failure and panic tests also observe the Rust buffer in its cleared state.

## Full Gates

`python scripts/ci/run.py quality` passed all 19 steps:

- source isolation over 241 files (71 text files) and 28 security unit tests;
- Prettier, ESLint, 6 Vitest tests, TypeScript, and Vite build;
- target-aware desktop sidecar preparation, Rust formatting, Clippy with warnings denied, 28 default-feature workspace tests, and full workspace build;
- bootstrap crypto, memory leak, Control Plane direct-dial, 7-process Rust host, and Tauri bundle/integrity audits;
- Go check;
- CycloneDX SBOM with 727 components and 53 resources, license and supply-chain validation.

Latest development artifacts from the bootstrap job:

| Artifact | SHA-256 |
| --- | --- |
| `artifacts/bootstrap/bootstrap.enc` | `81a3881b1df1a3478a79b75f7cde36e1475e1d25bbf1ced3d02a1940c0834f3a` |
| nonce-check ciphertext | `7af852d8a4aa7141f7bc0dfe1002157253277a13941eeaeaa7fb3879603d5f34` |
| non-sensitive manifest | `ca3658257073ccc543a3032500c9da5625ebcc1f520c0db90e6defd42a2ba790` |

## Remaining Production Configuration

The local implementation meets the in-memory decrypt, handoff, native-copy release, and zeroize acceptance rules, but the slice remains `review`: a real `bootstrap.enc` still requires approved production nodes, API hosts, expiry/rotation policy, and the Gitee Go secret values. Formal dependency `BOOT-G0-001` also remains `review` pending production resource generation.

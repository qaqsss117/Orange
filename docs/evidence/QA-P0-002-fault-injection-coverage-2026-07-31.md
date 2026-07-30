# QA-P0-002 Fault Injection and Coverage Evidence (2026-07-31)

## Scope and Result

`QA-P0-002` passed its six acceptance rules. The evidence combines explicit
unit and contract tests, mutation-guarded fault-injection markers, and coverage
reports for the frontend, Rust workspace, and both Go modules. Coverage
percentages support the rule-level evidence; they are not the sole acceptance
criterion and no global percentage threshold is used as a substitute for the
required scenarios.

All automated cases use local fixtures, loopback resources, temporary
directories, or reserved `.invalid` domains. They do not require a production
account, API token, subscription secret, live proxy credential, or unstable
public service. Ignored production acceptance tests remain outside this slice's
ordinary quality and coverage commands.

## Fault Injection Matrix

| Required fault | Locked test |
| --- | --- |
| Process kill | `native_child_crash_is_detected_after_consumer_rebuild` |
| Port conflict | `mixed_port_conflict_owned_by_another_process_fails_readiness` |
| Disk full during atomic write | `disk_full_during_atomic_write_preserves_the_previous_generation` |
| Corrupt route rule | `corrupt_route_rule_is_rejected_before_runtime_generation` |
| Blocked proxy node | `TestBlockedProxyDoesNotFallBackToAPI` |
| Offline-to-online network switch | `network_switch_from_offline_to_online_recovers_on_explicit_retry` |

`scripts/security/check_qa_fault_injection.py` binds all six names to their
owning source files. Its mutation suite removes each marker independently and
requires the audit to fail, so renaming or dropping a required scenario cannot
silently retain acceptance.

## Critical Unit and Contract Coverage

The same audit locks tests for dual state machines, DTO/error handling, AEAD
tamper rejection, native signature rejection, activation rollback,
configuration sanitization, and atomic persistence.

The versioned business API contract covers these 11 operations in order:
`config`, `login`, `register`, `account`, `subscription`, `plans`, `orders`,
`payment`, `invite`, `tickets`, and `update`. The failure fixture covers six
classes: empty 2xx, HTTP 4xx, HTTP 5xx, non-JSON, timeout, and schema drift.
Public DTO and error tests require these fixtures to remain complete and to
keep debug output redacted.

## Coverage Reports

The fixed command is `pnpm coverage`. Tool versions and build inputs are pinned
in `toolchains.toml`: `cargo-llvm-cov 0.8.7`,
`@vitest/coverage-v8 4.1.10`, Go `1.25.5`, and Go build tags
`with_quic,with_utls`. Reports are written only below the ignored
`artifacts/coverage/` directory.

| Target | Result |
| --- | ---: |
| Frontend lines | 82.44% |
| Frontend branches | 76.19% |
| Frontend functions | 81.20% |
| Frontend statements | 81.89% |
| Rust workspace lines | 74.44% |
| Rust workspace functions | 73.62% |
| Rust workspace regions | 75.01% |
| Go Control Plane statements | 66.40% |
| Go Data Plane statements | 60.40% |

| Ignored local report | Bytes | SHA-256 |
| --- | ---: | --- |
| Frontend JSON summary | 9,209 | `78607709867ca756b034590a0e3ba668e2931279ee685f6fcf079a66a217f662` |
| Rust JSON | 4,447,361 | `fbe05bddc257654151c4e64afd41f0b91021f2a6b110dca8e2878e6a3633fae7` |
| Go Control Plane profile | 17,802 | `53291fe7291672f5047a7770573267dfb63bf7bc3d3b331dbd2ecf465cfd2b5a` |
| Go Data Plane profile | 13,149 | `9794dd8d827e2a3ad4079844ced7b762003fa34ff9b7395bc57083362a40897c` |
| Combined summary | 1,400 | `d6dbfcc90b1145d99f841deea9f7a31c7fabacef540c35a5d784b0597e8d1b5f` |

## Verification and Flake Policy

The acceptance commands are:

```text
pnpm coverage
python scripts/security/check_qa_fault_injection.py
python -m unittest scripts.security.tests.test_qa_fault_injection -v
python -m unittest discover scripts/security/tests -v
python scripts/ci/run.py quality
```

The quality runner performs one attempt per test command. The QA audit rejects
`rerun` or retry flags, and its mutation test proves that protection remains
active. A failed test must be fixed or explicitly isolated and recorded; a
green retry is not accepted as evidence.

The direct QA audit passed with six required faults, 11 API operations, six
failure classes, and `flaky_reruns=false`. Its six mutation tests passed, and
the complete Python security/mutation discovery passed all 209 tests in one
run. `pnpm coverage` also passed in one run and reproduced the report hashes
above. With pinned Go `1.25.5` active, `python scripts/ci/run.py quality`
passed all 35 steps in one run.

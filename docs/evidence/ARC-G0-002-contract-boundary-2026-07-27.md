# ARC-G0-002 contract boundary evidence

- Date: 2026-07-27
- Host: Windows development host
- Slice status: `review`

## Implemented boundary

- `contracts/orange-ipc.schema.json` defines schema version 1 and the fixed `get_runtime_info` command request/response pair.
- `crates/orange-domain` owns the Rust DTOs, the nine public error categories, fixed safe messages, compatibility validation, and the command registry.
- `src/ipc.ts` owns the TypeScript DTOs, runtime parsers, and the only typed frontend invoke wrapper.
- `src-tauri/build.rs` generates an app-command ACL from the shared Rust command constant.
- `src-tauri/capabilities/default.json` grants only `allow-get-runtime-info`; `core:default` is not granted.
- `src-tauri/src/lib.rs` registers only `get_runtime_info`, with an explicit request and response DTO.

Requests reject unknown fields, unknown enums, and unsupported schema versions. Responses ignore unknown fields for forward compatibility but still reject unknown enums. No DTO accepts a URL, file path, shell string, arbitrary JSON map, secret, token, node, or diagnostic detail.

## Contract verification

```text
cargo test -p orange-domain
8 passed, 0 failed

pnpm test -- src/ipc.test.ts
5 passed, 0 failed

cargo test -p orange-app
1 passed, 0 failed
```

The Rust and TypeScript suites both consume `contracts/fixtures`. They cover request and response round trips, request unknown-field rejection, response unknown-field compatibility, unknown error enum rejection, schema/registry alignment, fixed public error serialization, and unregistered command rejection.

## Full gates

```text
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/dev/check-mirrors.ps1
Domestic mirror configuration verified.

pnpm check
Prettier passed
ESLint passed
Vitest: 2 files, 6 tests passed
Supply chain: 663 dependencies, 7 ecosystems, 75 configured URLs, 0 errors
Resource manifest: 53 files verified
TypeScript and Vite production build passed

cargo fmt --all --check
passed

cargo clippy --workspace --all-targets -- -D warnings
passed

cargo test --workspace
11 tests passed, 0 failed

python scripts/security/check_source_isolation.py
194 files scanned, 53 text files scanned, 53 registered resources, 0 errors
```

## Remaining dependency

The local implementation meets the `ARC-G0-002` acceptance rules, but the slice remains `review` because its formal `ARC-G0-001` dependency still lacks macOS/iOS runner and remote CI execution evidence. Apple validation is intentionally deferred to the final platform configuration stage.

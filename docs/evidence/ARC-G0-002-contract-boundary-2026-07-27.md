# ARC-G0-002 contract boundary evidence

- Date: 2026-07-27
- Host: Windows development host
- Slice status: `done` after acceptance review on 2026-07-30

## Implemented boundary

- `contracts/orange-ipc.schema.json` defines schema version 1 and the fixed `get_plane_state` and `get_runtime_info` request/response pairs.
- `crates/orange-domain` owns the Rust DTOs, the nine public error categories, fixed safe messages, compatibility validation, and the command registry.
- `src/ipc.ts` owns the TypeScript DTOs, runtime parsers, and the only typed frontend invoke wrapper.
- `src-tauri/build.rs` generates an app-command ACL from the shared Rust command constant.
- `src-tauri/capabilities/default.json` grants only the two read-only command permissions; `core:default` is not granted.
- `src-tauri/src/lib.rs` registers only `get_plane_state` and `get_runtime_info`, both with explicit request and response DTOs.

Requests reject unknown fields, unknown enums, and unsupported schema versions. Responses ignore unknown fields for forward compatibility but still reject unknown enums. No DTO accepts a URL, file path, shell string, arbitrary JSON map, secret, token, node, or diagnostic detail.

## Contract verification

```text
cargo test -p orange-domain
13 passed, 0 failed

pnpm test
6 passed, 0 failed

cargo test -p orange-app
4 passed, 0 failed
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
Supply chain: 824 dependencies, 7 ecosystems, 75 configured URLs, 0 errors
Resource manifest: 53 files verified
TypeScript and Vite production build passed

cargo fmt --all --check
passed

cargo clippy --workspace --all-targets -- -D warnings
passed

cargo test --workspace
55 tests passed, 0 failed

python scripts/security/check_source_isolation.py
281 files scanned, 90 text files scanned, 53 registered resources, 0 errors
```

## Acceptance outcome

All six `ARC-G0-002` rules have direct contract, capability, negative-case,
Rust, TypeScript, and build evidence. The missing macOS/iOS runner and remote
CI run belong to `ARC-G0-001`; they still block the global platform matrix but
do not add another rule to this platform-neutral DTO boundary. The slice is
therefore `done`. Any command, DTO, or capability expansion must rerun these
checks and can reopen it.

# SEC-G0-003 Control Egress And Secret Boundary Evidence

- Date: 2026-07-27
- Hosts: Windows 11 amd64 and Ubuntu 24.04.4 under WSL2
- Slice status: `in_progress`

## Endpoint Policy

`security/control-endpoints.yml` is a JSON-compatible YAML policy parsed with a
strict JSON parser, so the build does not add an unpinned YAML dependency. It
defines the ten required business command categories: login, register, config,
subscription, account, plans, orders, invite, tickets, and update.

The current policy is deliberately development-only:

- `release_allowed` and `production_hosts_configured` are both false;
- the sole host is the non-routable `api.orange.invalid`, and the audit requires
  it to match the encrypted development bootstrap fixture exactly;
- the scheme and port are fixed to HTTPS/443;
- redirect following is denied;
- connect and request timeouts match the encrypted bootstrap failover policy;
- concurrency is capped at 16, request/response bodies at 1 MiB, and request
  attempts at one.

The development paths are fixed fixtures, not a claim about the unapproved
production API contract. Production hosts and paths must replace this policy
through an explicit security review before `release_allowed` can change.

## Automated Egress Audit

`python scripts/security/check_control_egress.py` is a fail-closed quality step.
The passing report recorded:

```text
commands: 10
hosts: 1
production sources scanned for network clients: 24
production sources scanned for runtime log sinks: 24
runtime log sinks: 0
approved network implementation: native/controlplane/bridge.go
release allowed: false
```

The audit verifies:

- direct frontend HTTP dependencies and `fetch`, XHR, WebSocket, or EventSource
  construction are absent;
- direct Rust HTTP dependencies and socket client construction are absent;
- Go `net/http` construction exists only in the audited sing-box direct-dial
  bridge;
- the WebView CSP permits only self and Tauri IPC in `connect-src`;
- the public IPC schema exposes no URL, host, Authorization, token, bootstrap,
  route, or file-path request field;
- the Go bridge constructs only HTTPS targets, fixes the production port to
  443, disables proxy-environment discovery and redirects, requires TLS 1.2 or
  newer, and enforces the bootstrap host allowlist;
- production React, Rust, and Go runtime sources contain no unapproved print,
  console, log, or tracing sink that could receive a body, token, node, or local
  path.

Six focused security tests prove that policy, host, transport, dependency,
source, CSP, IPC, and log violations fail the audit. A real Go integration test
returns an HTTPS 302 response without following it and confirms the redirect
target receives zero requests.

## Secret Storage Contract

`orange-platform` now exposes only fixed `AccessToken` and `RefreshToken` keys,
a bounded non-cloneable `SecretValue`, stable redacted errors, a platform
backend trait, and a shared `SecretStorage` wrapper.

- `SecretValue` zeroizes on drop and its `Debug` output contains only
  `<redacted>`.
- Shared storage clears the caller's value after every store attempt, including
  permission failures.
- Logout attempts deletion of both token keys even if the first deletion fails.
- Loads return the controlled value type and are not exposed through a Tauri
  command or WebView DTO.

Four Rust tests cover bounds, redacted debug output, successful storage/load,
success/error clearing, logout deletion, and partial-failure cleanup. The test
backend exists only inside the Rust unit-test module; it is not a production
plaintext store.

## Full Gates

Windows `python scripts/ci/run.py quality` passed all 20 steps:

- source isolation over 250 files and 72 text files;
- 34 security tests, 6 frontend tests, and 31 Rust workspace tests;
- Control Plane, seven-process Rust host, Tauri bundle/integrity, Go,
  727-component SBOM, 53-resource, license, and supply-chain audits.

The same 20-step gate passed in the existing WSL2 workspace without `.git`:

- source isolation over 265 files and 72 text files;
- the same security/frontend/Rust test totals;
- Linux Control Plane and Tauri sidecar audits;
- 733-component Linux SBOM and the same 53 resources.

## Remaining Acceptance Work

This slice remains `in_progress` because the following evidence is still
missing:

- approved production API hosts, paths, and typed business command wiring
  through a single BootstrapTransport;
- Keystore, Keychain, Credential Manager, and Secret Service backend
  implementations with login/logout lifecycle tests;
- privileged packet captures proving runtime Control Plane destinations match
  the approved allowlist and remain distinct from user tunnel traffic;
- completion of the formal `ARC-G0-002` and `BOOT-G0-003` dependencies.

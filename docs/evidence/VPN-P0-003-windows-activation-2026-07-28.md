# VPN-P0-003 Windows Candidate Activation Evidence

Date: 2026-07-28

## Scope

This increment connects the service-owned candidate, health, activation, active
revision, restore, and discard operations. The application refresh command and
SCM installer are outside this increment and remain pending.

## Candidate Boundary

- The service reads only the fixed sanitized `<revision>.json` file.
- It derives `.<revision>.probe.json` with a structured JSON parser.
- The probe has one mixed inbound on `127.0.0.1`, does not set system proxy,
  and contains no TUN inbound.
- The same fixed, hash-verified `orange-data-plane.exe` performs the managed
  host handshake and the default-node HTTPS delay probe.
- DNS independence is checked from the closed sanitized configuration: exactly
  one local resolver tagged `orange-local-dns` is accepted.
- Probe files use create-new, flush, fixed-parent, and reparse-safe cleanup.

## Activation And Recovery

After all three health fields pass, the probe process is reaped before the
existing supervisor starts the fixed revision as the TUN owner. Active revision
state is service-owned and restore can start a previous revision or clear all
ownership. Candidate discard is idempotent and removes only the fixed candidate
and revision files.

Refreshing while a previous TUN is active currently stops the old instance
before probing the new revision. A failed candidate restores the previous
revision, but this is an interruptible switch and is not claimed as atomic or
zero-downtime.

## Test Trust Mode

Formal builds still require Authenticode and an approved signer allowlist. The
`unsigned-test-runtime` Cargo feature is accepted only while the embedded
manifest still requires Authenticode, has no approved signer, and explicitly
has `release_allowed=false`. Builds without the feature retain the original
fail-closed signature behavior.

## Verification

- Formal service mode: 50 passed tests, one audited-artifact test ignored.
- Explicit unsigned test mode: 50 passed tests, one audited-artifact test
  ignored.
- Candidate fixture proof: loopback-only mixed inbound, no system proxy, no
  TUN tag, preserved selector/default node, and closed local DNS.
- Windows IPC and platform-permission audits passed.
- 27 security mutation tests passed.

The increment does not claim SCM installation, protected installer ACLs,
application-driven subscription activation, signed release eligibility, or an
installed real-network TUN result.

# VPN-P0-004 Production Node Acceptance (2026-07-31)

## Scope

This Windows 10 development acceptance used the ignored production Bootstrap,
live business account, current 18-entry VLESS subscription, audited Control
Plane sidecar, and audited Data Plane executable. Inputs existed only in the
child-process environment. The test did not print or persist the account,
credentials, subscription URL/body, node IDs, servers, ports, UUIDs, Reality
keys, SNI values, or API host.

The run covers the node-runtime portions that were previously supported only by
fixtures and offline process tests. It does not claim signed release status,
installed TUN packet capture, or non-Windows platform completion.

## Live Results

- Production config, login, subscription metadata, and native subscription
  download succeeded through the encrypted Bootstrap Control Plane.
- Sanitization produced one public selector with 18 VLESS nodes.
- The Data Plane accepted all 18 delay requests in batches of at most eight,
  with a fixed 15-second per-request timeout. Results were 16 `available`, zero
  `timed_out`, and two `unavailable`; every input node produced exactly one
  bounded result.
- The test chose an available non-default node without recording its ID, called
  the production selector command, and required the core readback to match
  before continuing.
- A live account request still succeeded through the same Control Plane while
  the selected Data Plane node was active. HTTPS through the mixed listener also
  succeeded after the switch.
- The Data Plane process was stopped and started again with the same sanitized
  config. The production `FileSettingsStore` ledger restored the confirmed
  selection and the restarted core read it back; HTTPS remained available.
- The selected node was then removed from a reduced public catalog after the
  core was reset to the new config's default. Reconciliation ignored the stale
  ledger entry, selected the explicit default, and atomically rewrote the
  ledger. A subscription request through the unchanged Control Plane succeeded
  afterward.
- Both Data Plane processes and the temporary config/settings directory were
  removed by guards at test exit.

## Regression Boundary

`scripts/security/check_data_plane_nodes.py` now requires the production test to
retain the 8-wide delay batches, selection/readback, account request, restart
restore, deleted-node fallback, and post-fallback subscription request. A
mutation test removes restart restore and requires the audit to fail. The live
test remains ignored during ordinary quality runs because it requires explicit
production inputs and a stable network.

## Bound Artifacts

| Artifact | SHA-256 |
| --- | --- |
| Encrypted Bootstrap | `b585043015a01185125a3390eab4e01a746658e03795093b1043db999d5f98df` |
| Bootstrap manifest | `3373a620c4714e1f8fef13719a2bcb01a122b0e6c0e0576ddabcebe2bd196412` |
| Control Plane sidecar | `eb51e15495d5f06616b10a1ee7fe1e703aa809d6d7246bd8b473eb6d22c14606` |
| Data Plane executable | `b185cd22d13e0af8c785d77b40d7c3daaa1f9217465653234235fccf4b75e611` |
| Production test source | `d003ecb3b329c9d4237a4295a90f21f5cfa66db21d5868ea4d6b5ecdcc5480e2` |

The artifacts are ignored local development outputs. The hashes bind the
evidence without making an unsigned executable releasable.

## Remaining Work

`VPN-P0-004` remains `in_progress`. The installed TUN node-switch packet
capture and Linux/macOS/iOS production backends are still missing. Formal
Windows signing remains owned by the Windows/release slices.

The final pinned Windows quality entry point passed all 35 steps, including 202
Python security/mutation tests, 54 frontend tests plus build, the complete Rust
workspace formatting/Clippy/test/build matrix, both Go modules, Bootstrap,
SBOM/supply-chain checks, and the reproducible Windows Data Plane audit.

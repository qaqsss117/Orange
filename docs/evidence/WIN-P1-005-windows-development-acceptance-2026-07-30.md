# WIN-P1-005 Windows Development Acceptance Evidence (2026-07-30)

## Scope and Result

The resumable Windows development workflow in
`scripts/acceptance/windows-development.ps1` completed on Windows 10 Pro 22H2,
build 19045. All 12 reports for `preflight`, `build`, `install`, `proxy`, `tun`,
the four isolated crash cases, `upgrade`, `uninstall`, and `verify-clean` have
`status: passed`.

This closes the unsigned Windows 10 development loop. It is not release
evidence: both installers remain unsigned test artifacts with
`release_allowed=false`, and no Windows restart was performed.

## Execution Context

- Git base: `14ca810e0edeed8a8e7222a280a45b02383a8d66`, with dirty-worktree
  provenance recorded by tracked-diff and untracked-path SHA-256 values.
- Node.js `22.23.1` and pnpm `11.9.0`.
- Rust and Cargo `1.95.0`.
- Go `1.25.5` from the pinned per-user SDK.
- Baseline: revision `6b23686`, version `0.0.9`.
- Candidate: the current acceptance worktree, version `0.1.0`.

The final package set used for the upgrade and clean-state completion is:

| Package | SHA-256 |
| --- | --- |
| `Orange_0.0.9_x64-setup.exe` | `185061b740f3396cf44e8629c420914c3ede50d1d5f0e3dd68cd1dc57d0a58c7` |
| `Orange_0.1.0_x64-setup.exe` | `cfa5a2f6052c8008f9751b8bcfbf8362e6d4a92ce4644ac632b7ce4ee8068ebf` |
| Data Plane runtime | `b185cd22d13e0af8c785d77b40d7c3daaa1f9217465653234235fccf4b75e611` |

Both installers returned Authenticode status `NotSigned`. The install report
retains the hashes of the earlier pre-fix package set that it installed; after
the TUN defect was corrected, the baseline and candidate were rebuilt, and the
build and upgrade reports record the final package hashes above.

## Production and Network Acceptance

The approved environment was read only from `ORANGE_BOOTSTRAP_*`,
`ORANGE_E2E_EMAIL`, `ORANGE_E2E_PASSWORD`, and the exit-probe variable. Reports
and logs contain no credential values, Bootstrap JSON, subscription URL/body,
or node material.

The installed application completed the real production login and subscription
chain, sanitized and activated the VLESS catalog, and kept sensitive
subscription data outside the WebView and evidence reports.

System proxy mode passed with the fixed `127.0.0.1:24836` listener, expected
Data Plane PID ownership, domestic and overseas HTTPS, exit change, traffic
growth, and restoration after stop. Mixed-mode readiness now verifies the
listener owner instead of requiring the TUN interface.

The first TUN run reproduced a DNS black hole: the node and pre-resolved HTTPS
path worked, but all hostname-based requests failed. sing-box 1.13 must sniff a
packet before a `protocol=dns` route can match. The generated route order is now
`sniff`, `hijack-dns`, then the subscription routes, and local resolution uses
the fixed Alibaba DoT endpoint with TLS identity validation. The rerun passed
fixed interface/address checks, DNS, domestic and overseas HTTPS, exit change,
Control Plane bypass, and complete interface/route/DNS cleanup.

## Install, Fault, Upgrade, and Cleanup

Installation passed the fixed Program Files location, automatic LocalSystem
service, unrestricted service SID, protected installation identity/runtime
ACLs, exact packaged-file hashes, per-installation Named Pipe identity, and
firewall binding checks.

Separate UI, Control Plane, Data Plane, and Service termination phases passed
their network-safety and process-cleanup postconditions. A Service termination
safely stops the Data Plane and restores network state. After manually starting
the Service, the existing UI retains a stale Named Pipe connection; restarting
the UI rebuilds the Pipe and restores operation. This run therefore does not
claim in-place Service hot recovery.

The candidate upgrade preserved the installation identity, active revision,
and node state, and did not mix old and new binaries. Uninstall and final
`verify-clean` confirmed no Orange service, process, installation root, runtime,
proxy recovery journal, RunOnce entry, TUN interface/route/DNS state, firewall
rule, or port `24836` listener remained. The user's pre-existing system proxy
state was restored.

## Evidence Index

Raw reports are Git-ignored under
`artifacts/acceptance/windows-development`. The combined schema-v1 result has
SHA-256
`c18702ca2f99c1df1f97c477044e10da0e0a8a4622fb269018e4c95489769c62`.

| Phase | Report SHA-256 |
| --- | --- |
| `preflight` | `596c273b99934f6face6310b784ae83a4c763e124a9bc038b8504913c43541ea` |
| `build` | `9599790872ff273137a7fa6546502a3103eb29fa46e674dd590c3cb29697aa04` |
| `install` | `7eb48b401db631d1843f32873135c062f05777d953d7753b9891ea72a1029e5a` |
| `proxy` | `199e193c3ff1febdb380185d83704a53d19cb2694c5b769bb5a4fa0807c66765` |
| `tun` | `4ee02a8276a9d6e2dc5f2d367bd62fee9d2c5a3417fe348ffcb41cbfa3c1e3c8` |
| `crash-ui` | `3c89c27ddc9b13c1931cc51e813247387d9ef686b82227d611e85e56cdb05365` |
| `crash-control-plane` | `57f1682765062008773453e548331dd1f0559072442736bc26a7ff74de85312e` |
| `crash-data-plane` | `8bb31eb8d70be6dbeabb511e58cdb7d1c7cc19a7b09c3ad28d568489848d0d19` |
| `crash-service` | `fe6a1235f0f593954f9ab260fde1b91b93d59a6fb87a5c3c671f6ba9ee23bb01` |
| `upgrade` | `48a9f9ee82671049d4a86369a2dddf51c226d0b1fd0799f4bfdf74f181286878` |
| `uninstall` | `ed590066669924efb924e899e1396e903b1e5baf8e04e62027333b684d200d2b` |
| `verify-clean` | `2e91a01ca30b1d6647677c771dfdf0c1ab4252aa550b80ade8536c9b5f10af1e` |

## Remaining Boundaries

`UI-P0-005`, `WIN-P0-003`, `WIN-P1-004`, `WIN-P1-005`, and `QA-P0-002`
remain `in_progress`; `QA-G0-001` remains `review`. Formal signing, a real
Windows restart, Windows 11, cross-user and low-integrity testing,
upgrade-failure rollback, a remote CI run link, and other platforms remain
outstanding.

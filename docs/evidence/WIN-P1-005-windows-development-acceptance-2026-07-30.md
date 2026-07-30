# WIN-P1-005 Windows Development Acceptance Evidence (2026-07-30)

## Scope and Result

The resumable Windows development workflow in
`scripts/acceptance/windows-development.ps1` completed on Windows 10 Pro 22H2,
build 19045. All 14 reports for `preflight`, `build`, `install`,
`ipc-boundary`, `proxy`, `tun`, the four isolated crash cases,
`upgrade-failure`, `upgrade`, `uninstall`, and `verify-clean` have
`status: passed`.

This closes the unsigned Windows 10 development loop. It is not release
evidence: all three installers remain unsigned test artifacts with
`release_allowed=false`, and no Windows restart was performed.

## Execution Context

- Git base: `90f8470107c99496f9a1b7efa00757d9e9eced0a`, with dirty-worktree
  provenance recorded by tracked-diff and untracked-path SHA-256 values.
- Node.js `22.23.1` and pnpm `11.9.0`.
- Rust and Cargo `1.95.0`.
- Go `1.25.5` from the pinned per-user SDK.
- Baseline: revision `6b23686`, version `0.0.9`.
- Candidate: the current acceptance worktree, version `0.1.0`.

The final package set used for the upgrade and clean-state completion is:

| Package | SHA-256 |
| --- | --- |
| `Orange_0.0.9_x64-setup.exe` | `bd74325a47b864c43962a09ebbd1f5fdb64ea2258b4f4c08dc04da49f9037ea8` |
| `Orange_0.1.0_x64-upgrade-failure-setup.exe` | `c5be5658b13e17899e223c5c9b481471574bf56d01330321c8ab067cad61087a` |
| `Orange_0.1.0_x64-setup.exe` | `223fa6057317cae7718ca54b1b625dc960fb94f257be2fbf8006a880ffb6d2b7` |
| Data Plane runtime | `b185cd22d13e0af8c785d77b40d7c3daaa1f9217465653234235fccf4b75e611` |

All three installers returned Authenticode status `NotSigned`. The earlier
network and crash reports retain the package hashes from that completed run.
After adding rollback and IPC acceptance, the three packages were rebuilt and
the final build, install, IPC, failure-upgrade, normal-upgrade, uninstall, and
clean-state reports record the package hashes above.

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

The installed Named Pipe rejected an independent process running as a newly
created second local user and an independent process whose token was lowered to
Low Mandatory Level (`S-1-16-4096`). Both probes treated an unexpected successful
connection as failure, the service remained running, and the temporary account
and profile were removed. No username, SID, password, or credential material was
written to the reports.

Separate UI, Control Plane, Data Plane, and Service termination phases passed
their network-safety and process-cleanup postconditions. A Service termination
safely stops the Data Plane and restores network state. After manually starting
the Service, the existing UI retains a stale Named Pipe connection; restarting
the UI rebuilds the Pipe and restores operation. This run therefore does not
claim in-place Service hot recovery.

The dedicated unsigned fault-injection package failed at the
post-payload/pre-service-install boundary with exit code 32. Rollback restored
all six fixed packaged files byte-for-byte, restarted the previous service,
preserved the installation identity, active revision and registry display
version, and removed the rollback backup. The injection macro is used only by
the acceptance package; the normal candidate does not enable it. Rollback
registry metadata is persisted inside the protected backup before the ready
marker is committed, so a subsequent installer process can recover an
interrupted attempt without relying on lost NSIS process variables.

The subsequent normal candidate upgrade preserved the installation identity,
active revision, and node state, and did not mix old and new binaries. Uninstall
and final `verify-clean` confirmed no Orange service, process, installation
root, runtime, proxy recovery journal, RunOnce entry, TUN interface/route/DNS
state, firewall rule, or port `24836` listener remained. The user's pre-existing
system proxy state was restored.

## Evidence Index

Raw reports are Git-ignored under
`artifacts/acceptance/windows-development`. The combined schema-v1 result has
SHA-256
`f64e6a28e2ef7ee1968082edaff23d58bc6b76899671454e8c3d9f4c8d9738e4`.

| Phase | Report SHA-256 |
| --- | --- |
| `preflight` | `596c273b99934f6face6310b784ae83a4c763e124a9bc038b8504913c43541ea` |
| `build` | `4dc9f0a42c0b252db7552e2e007e7d813c8e0be4c2260c084334070aa72ffff3` |
| `install` | `1f04d722c56893b7b1118eea188f28caa278a1787677689a2ea34897c5e6e7f5` |
| `ipc-boundary` | `9bb7f2d872cd3ed6752da7f75daa8a238e4f53933f1dbc84a98dcc1cddfc01c1` |
| `proxy` | `199e193c3ff1febdb380185d83704a53d19cb2694c5b769bb5a4fa0807c66765` |
| `tun` | `4ee02a8276a9d6e2dc5f2d367bd62fee9d2c5a3417fe348ffcb41cbfa3c1e3c8` |
| `crash-ui` | `3c89c27ddc9b13c1931cc51e813247387d9ef686b82227d611e85e56cdb05365` |
| `crash-control-plane` | `57f1682765062008773453e548331dd1f0559072442736bc26a7ff74de85312e` |
| `crash-data-plane` | `8bb31eb8d70be6dbeabb511e58cdb7d1c7cc19a7b09c3ad28d568489848d0d19` |
| `crash-service` | `fe6a1235f0f593954f9ab260fde1b91b93d59a6fb87a5c3c671f6ba9ee23bb01` |
| `upgrade-failure` | `c183415a06c1ffe6969d5d5b14016ad3ac07e3f4dac3ab6a449d922c3c9940b8` |
| `upgrade` | `d07ad7a51f8744ba06a8a4a6edc7b32f70a39ad1d2737a419803d94eef6796ab` |
| `uninstall` | `c8861f9fa97ce8cf62ea25f62c630df38238a96cc36c30203fd3eec463df97a1` |
| `verify-clean` | `07b015c8a98f25fdf40cc35376a43ead64f3bc46a6aeef4d4708c06b29787d95` |

## Remaining Boundaries

`WIN-P0-003` is now `review` because implementation is complete but a real
Windows restart required by its acceptance rule is still missing.
`UI-P0-005`, `WIN-P1-004`, `WIN-P1-005`, and `QA-P0-002` remain
`in_progress`; `QA-G0-001` remains `review`. Formal signing, Windows 11, a
remote CI run link, and other platforms remain outstanding. In-place UI
recovery after a manually restarted Service is also not claimed.

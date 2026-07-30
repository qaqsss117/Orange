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

A focused follow-up on 2026-07-31 refreshed `build`, `install`, `uninstall`,
and `verify-clean` against the new uninstall-data contract. The other ten
reports retain the previously completed production network, IPC, crash, and
upgrade evidence; all 14 reports remain `passed` in the combined result.

## Execution Context

- Git base: `90f8470107c99496f9a1b7efa00757d9e9eced0a`, with dirty-worktree
  provenance recorded by tracked-diff and untracked-path SHA-256 values.
- Node.js `22.23.1` and pnpm `11.9.0`.
- Rust and Cargo `1.95.0`.
- Go `1.25.5` from the pinned per-user SDK.
- Baseline: revision `6b23686`, version `0.0.9`.
- Candidate: the current acceptance worktree, version `0.1.0`.

The focused follow-up used Git base
`ab086880175cd996aa3cc5cd6a184bfaf36d569c`; its dirty-worktree provenance was
again recorded only as tracked-diff and untracked-path SHA-256 values.

The final package set used for the upgrade and clean-state completion is:

| Package | SHA-256 |
| --- | --- |
| `Orange_0.0.9_x64-setup.exe` | `bd74325a47b864c43962a09ebbd1f5fdb64ea2258b4f4c08dc04da49f9037ea8` |
| `Orange_0.1.0_x64-upgrade-failure-setup.exe` | `c5be5658b13e17899e223c5c9b481471574bf56d01330321c8ab067cad61087a` |
| `Orange_0.1.0_x64-setup.exe` | `223fa6057317cae7718ca54b1b625dc960fb94f257be2fbf8006a880ffb6d2b7` |
| Data Plane runtime | `b185cd22d13e0af8c785d77b40d7c3daaa1f9217465653234235fccf4b75e611` |

The 2026-07-31 uninstall follow-up rebuilt the package set as follows. The
normal candidate was the package installed, uninstalled with default
retention, reinstalled, and uninstalled with explicit data deletion.

| Package | Bytes | SHA-256 |
| --- | ---: | --- |
| `Orange_0.0.9_x64-setup.exe` | 19,082,468 | `1fa9c4f2ba4239e91ff5935ff4154e880e3875532e0ea663643aa583358ce266` |
| `Orange_0.1.0_x64-upgrade-failure-setup.exe` | 19,510,999 | `c9d4218b806a7ab3719dfdaa51d0ad65cb793c5bb11fd3111a1427a59885924c` |
| `Orange_0.1.0_x64-setup.exe` | 19,500,243 | `338942109f592db3b5991313f6fc9204f614264f19b0a9774229039a2b3d109c` |

After fixing update mode to use `prepare-upgrade`, all three NSIS packages were
rebuilt again. These final static artifacts were not substituted for the
already accepted runtime candidate above; generated NSIS inspection confirms
that update mode preserves credentials, full uninstall invokes `uninstall`,
and Tauri deletes only the exact Roaming/Local bundle directories.

| Package | Bytes | SHA-256 |
| --- | ---: | --- |
| `Orange_0.0.9_x64-setup.exe` | 19,078,377 | `b62586a8e80a103b1ead77d0556debec83093d1af17107a8ff04330f470d8f52` |
| `Orange_0.1.0_x64-upgrade-failure-setup.exe` | 19,504,947 | `7ad69d6f8ad7d88657e3298d1c5cd08586b5c22fffe07f5ef9f17567ba1629cb` |
| `Orange_0.1.0_x64-setup.exe` | 19,501,774 | `df04b10e33e9234236dc9291225a6bb8d357ed5743e077f9e277c6a384cc3f36` |

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

The follow-up closed the configuration-choice part of Windows acceptance rule
5. Tauri's interactive uninstaller continues to expose its explicit delete-data
checkbox. Silent `/S` now defaults to retention; `/DELETEAPPDATA` is the only
custom flag that sets Tauri's delete state. The custom hook never deletes an
AppData path itself, so the generated Tauri template remains responsible for
the two exact `com.orange.vpn.dev` Roaming and Local directories.

The acceptance phase wrote a non-sensitive marker under both directories,
confirmed that default `/S` retained both, and confirmed through the production
Rust secret-store adapter that the three fixed credentials changed from a
complete state to empty without reading their values. It then reinstalled the
same candidate, ran `/S /DELETEAPPDATA`, and confirmed both fixed directories
were absent. Both uninstall paths removed the service, helper/runtime,
installation root, processes, proxy recovery journal, RunOnce entry, TUN
interface/routes/DNS, firewall rule, and mixed listener. Static mutation tests
reject forced default deletion, a disabled explicit flag, custom broad AppData
deletion, removal of native credential cleanup, or routing update mode through
full uninstall instead of `prepare-upgrade`.

## Evidence Index

Raw reports are Git-ignored under
`artifacts/acceptance/windows-development`. The combined schema-v1 result has
SHA-256
`723c7d0fd6e9bb376a2f7f71be82535e582246317572880f6bf4c9920e289e68`.

| Phase | Report SHA-256 |
| --- | --- |
| `preflight` | `596c273b99934f6face6310b784ae83a4c763e124a9bc038b8504913c43541ea` |
| `build` | `20c55d214d25f4f4190ce29b2f5bd6504535267749872a41ff6eefe8f6d9bb1d` |
| `install` | `00041229f649cd1f54aac65a1e033611d2108b2201823ea0f78200f327b7474c` |
| `ipc-boundary` | `9bb7f2d872cd3ed6752da7f75daa8a238e4f53933f1dbc84a98dcc1cddfc01c1` |
| `proxy` | `199e193c3ff1febdb380185d83704a53d19cb2694c5b769bb5a4fa0807c66765` |
| `tun` | `4ee02a8276a9d6e2dc5f2d367bd62fee9d2c5a3417fe348ffcb41cbfa3c1e3c8` |
| `crash-ui` | `3c89c27ddc9b13c1931cc51e813247387d9ef686b82227d611e85e56cdb05365` |
| `crash-control-plane` | `57f1682765062008773453e548331dd1f0559072442736bc26a7ff74de85312e` |
| `crash-data-plane` | `8bb31eb8d70be6dbeabb511e58cdb7d1c7cc19a7b09c3ad28d568489848d0d19` |
| `crash-service` | `fe6a1235f0f593954f9ab260fde1b91b93d59a6fb87a5c3c671f6ba9ee23bb01` |
| `upgrade-failure` | `c183415a06c1ffe6969d5d5b14016ad3ac07e3f4dac3ab6a449d922c3c9940b8` |
| `upgrade` | `d07ad7a51f8744ba06a8a4a6edc7b32f70a39ad1d2737a419803d94eef6796ab` |
| `uninstall` | `f3ccc3068afef8fec3854a93b4e87cef68bc865978e15717bab9230c79af2276` |
| `verify-clean` | `6f0862ba533cb11871ce0329ccc2f0ef0e5993bb3a1b52fbab8e98b05b36b2db` |

## Remaining Boundaries

`WIN-P0-003` is now `review` because implementation is complete but a real
Windows restart required by its acceptance rule is still missing.
`UI-P0-005`, `WIN-P1-004`, `WIN-P1-005`, and `QA-P0-002` remain
`in_progress`; `QA-G0-001` remains `review`. Formal signing, Windows 11, a
remote CI run link, and other platforms remain outstanding. In-place UI
recovery after a manually restarted Service is also not claimed. Windows
acceptance rule 5's configuration-choice and credential-residue work is closed;
it does not satisfy rule 6 or the explicit Windows P0 dependency.

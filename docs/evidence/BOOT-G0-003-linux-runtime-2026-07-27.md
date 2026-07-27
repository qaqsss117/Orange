# BOOT-G0-003 Linux Runtime Evidence

- Date: 2026-07-27
- Host: Ubuntu 24.04.4 LTS under WSL2
- Kernel: `6.6.87.2-microsoft-standard-WSL2`
- Architecture: x86_64
- Slice status: `in_progress`

## Qualification Scope

This run qualifies the Linux-native Control Plane, Rust host, Tauri sidecar
handoff, and desktop-shell startup inside WSL2. It is not bare-metal Linux or a
distribution installer qualification.

The audit used an isolated copy at
`/home/dev/orange-linux-smoke-20260727152321`. The copy excluded `.git`,
dependency directories, artifacts, build outputs, and generated mobile files.
Passing without Git metadata also verifies that build and SBOM audits do not
silently depend on VCS stamping.

## Fixed Toolchain

| Tool | Version |
| --- | --- |
| Node.js | `22.23.1` |
| pnpm | `11.9.0` |
| rustc | `1.95.0` |
| Cargo | `1.95.0` |
| Go | `1.25.5 linux/amd64` |
| Python | `3.12.3` |

`scripts/dev/setup-linux-toolchain.sh` installed the pinned Go archive from the
configured domestic distribution mirror after verifying SHA-256
`9e9b755d63b36acf30c12a9a3fc379243714c1c6d3dd72861da637f336ebb35b`.

## Linux Quality Gate

`python3 scripts/ci/run.py quality` passed all 19 steps in the isolated copy:

- source isolation passed over 255 files, 71 text files, and 53 registered
  resources;
- all 28 security tests and 6 frontend tests passed;
- Rust formatting, Clippy with warnings denied, all 28 workspace tests, and the
  workspace build passed;
- the Linux Control Plane audit passed all 6 tests, including
  `TestControlPlaneAddsNoTCPOrUDPListener` using process-owned socket inodes and
  `/proc/net/{tcp,tcp6,udp,udp6}`;
- all 7 Rust host process tests passed, including the real Go initialization
  handoff and EOF/forced shutdown paths;
- Tauri emitted the app and fixed sidecar together, copied the sidecar
  byte-for-byte, and embedded the same SHA-256 in the app;
- Go verify, vet, and tests passed; the 733-component SBOM, 53 resources,
  licenses, and 7-ecosystem supply-chain policy passed.

## Artifacts And Startup

| Artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `target/debug/orange-app` | 171,799,232 | `599e67211903561e30144ffa5c01f6e2ca5557938f548b9ff718a1e62faeabcc` |
| `target/debug/orange-control-plane` | 22,666,331 | `864d44fa56e6595bd30758390f97a6f0c4a2dfb63dd219a454b1f55fdd113330` |

The sidecar digest matched the generated
`artifacts/tauri-sidecars/orange-control-plane-x86_64-unknown-linux-gnu`
artifact and the integrity digest embedded in `orange-app`.

The desktop shell ran under `dbus-run-session` and `xvfb-run`. `timeout 8s`
returned status 124, proving that the process remained alive for the complete
eight-second smoke window. Expected software-rendering and desktop-portal
warnings did not terminate the application.

## Remaining Claims

This evidence does not prove macOS or Android/iOS runtime behavior, privileged
packet capture, a production bootstrap proxy/API, a bare-metal Linux desktop,
or a signed and distributable installer. Those acceptance gaps keep
`BOOT-G0-003` in progress.

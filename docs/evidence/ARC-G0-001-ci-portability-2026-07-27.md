# ARC-G0-001 Provider-Neutral CI Evidence

Date: 2026-07-27

Updated: 2026-08-01

## CI Command Boundary

`scripts/ci/run.py` is the provider-neutral entry for all current CI jobs:

- `security`
- `frontend`
- `rust`
- `rust-core`
- `go`
- `supply-chain`
- `desktop-shell`
- `android-shell`
- `ios-shell`
- `portable-quality`
- `quality`

The script reads mirror endpoints from `toolchains.toml` and injects npmmirror,
rsproxy, goproxy.cn, and the Chinese Go checksum database into every child
process. Android generation continues to install the Tencent Gradle
distribution URL and Aliyun Maven repositories.

The native Gitee Go adapters in `.workflow` invoke `portable-quality` through
`scripts/ci/run-gitee-cloud.sh`. That bootstrap uses only registered domestic
mirrors and pins the Python TOML compatibility package by version and hash.

As of 2026-07-31, `.github/workflows/quality.yml` is a package-only workflow by
repository-owner decision. It no longer delegates lint, unit, security, or
coverage commands to `scripts/ci/run.py`. The entry remains usable locally and
by provider adapters, but it is not a current GitHub merge gate.

## Gitee Adapter Local Verification

`python scripts/ci/run.py portable-quality` passed all 12 steps locally. The
result includes 12 security unit tests, frontend formatting/lint/test/build,
format/lint/test/build for the three portable Rust crates, Go 1.25.5, a
CycloneDX SBOM with 676 components and 53 resources, and a supply-chain scan of
662 dependency names and 75 configured URLs. Prettier parsed all three Gitee
YAML files, and `bash -n scripts/ci/run-gitee-cloud.sh` passed.

This is local adapter evidence only. The Gitee carrier is not accepted as
verified until the files are pushed, Gitee Go parses them, and a successful
remote run link is retained.

## Windows Host Verification

The following commands passed on the pinned Windows host:

```powershell
python scripts/ci/run.py --list
python scripts/ci/run.py quality
python scripts/ci/run.py desktop-shell
powershell -ExecutionPolicy Bypass -File scripts/dev/check-mirrors.ps1
powershell -ExecutionPolicy Bypass -File scripts/dev/check-toolchain.ps1
```

The `quality` job completed all 12 steps. Evidence included 12 security unit
tests, one Vitest file, the full Rust workspace checks, Go 1.25.5, a CycloneDX
SBOM with 676 components and 53 resources, and a supply-chain scan of 662
unique dependency names and 72 configured URLs.

The desktop shell built successfully:

- Artifact: `target/debug/orange-app.exe`
- Size: `12456960` bytes
- SHA-256: `e25c7cc4828df99bee9cdcccd188aa42e335c70ea12dbf6a83bdc170b3522cf3`

## Isolated Android Verification

`python scripts/ci/run.py android-shell` ran in an isolated copy that excluded
Git metadata, dependencies, build output, artifacts, and generated mobile
files. It completed Android initialization, domestic mirror configuration, and
an aarch64 debug APK build.

- APK size: `120860404` bytes
- APK SHA-256: `8b08cbd2902ba33e39a798ea36f41beb17f77cc524d4865cb49c483cd402a13e`
- Compile/target SDK: `36`
- Minimum SDK: `24`
- Gradle distribution: Tencent Cloud mirror, version `8.14.3`
- Maven repositories: Aliyun gradle-plugin, google, public, and central

`aapt dump permissions` reported only:

```text
android.permission.INTERNET
com.orange.vpn.dev.DYNAMIC_RECEIVER_NOT_EXPORTED_PERMISSION
```

The package-specific permission is the AndroidX non-exported dynamic receiver
permission. No photo, camera, microphone, contacts, location, or storage
permission was present.

## GitHub Five-Platform and Apple Launch Verification

GitHub Actions [`package #31`](https://github.com/qaqsss117/Orange/actions/runs/30688063273)
completed successfully for commit `4a612bc0c1be119bd8ac302150ed9b8a9b924c1a`.
All five matrix jobs passed on GitHub-hosted Windows, Ubuntu, and macOS runners
in 11 minutes 30 seconds.

The macOS job copied the same release `.app` used for signing before mutation,
started that copy through LaunchServices, resolved the exact bundle executable
PID, and confirmed that it remained alive for the eight-second startup
checkpoint. The signed source bundle continued through `codesign`, PKG
creation, and signature verification. `macos-shell.txt` is retained beside the
PKG in the macOS artifact.

The iOS job built the signed App Store IPA and a separate unsigned
`aarch64-sim` debug shell. The smoke probe selected an available iPhone
simulator, booted it, installed the simulator app, launched the configured
bundle identifier, checked the host-visible application PID after eight
seconds, and captured a non-empty screenshot. `ios-shell.txt` and
`ios-shell.png` are retained beside the IPA in the iOS artifact.

The public run summary retained these workflow artifacts (sizes are the rounded
values displayed by GitHub):

| Artifact | Displayed size | Archive digest |
| --- | ---: | --- |
| `orange-windows` | 18.9 MB | `sha256:f128e2a7494367e5ff30e538fe694becc4277eb059f41a383d33f11e34264d5a` |
| `orange-linux` | 120 MB | `sha256:bd90d88650d16d7b654ed503adfa205846587ac2f982d41b594d6bb3adbfbcbd` |
| `orange-macos` | 14.3 MB | `sha256:8dcd77da8b33f4f025ff3d18130695f0aca47ab6e3ff3dd1eae06169cfcc1ca1` |
| `orange-android` | 24.9 MB | `sha256:167bf14591b7cb01edae808a3e82554c81a2be3563120779ec1157dfecd72139` |
| `orange-ios` | 3.08 MB | `sha256:1961f798c6809dbef2e0c40a35b88e6beb521bf284ad638e05d894cf9fd25ddf` |

## Remaining Acceptance Gap

The Apple runner and launch-evidence blocker is closed. Together with the
previous Windows, Linux, and Android evidence, acceptance rule 1 now has a
retained platform result.

`ARC-G0-001` remains `in_progress`, not `done`, because acceptance rule 3
requires TypeScript strict/ESLint/format/Vitest, Rust fmt/clippy/test, and Go
checks to run in CI. The current GitHub workflow only builds and uploads the
five platform packages, and no successful remote Gitee quality run is retained
as an active substitute. Local quality evidence does not satisfy that remote
CI requirement.

This evidence does not claim Apple Network Extension entitlement, real-device
VPN operation, store approval, or release completion; those belong to later
Apple and release slices.

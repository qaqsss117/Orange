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

As of 2026-08-01, `.github/workflows/quality.yml` runs a focused
`workspace-quality` job beside the five-platform package matrix. The job invokes
the frontend, Rust, and Go quality commands directly. `scripts/ci/run.py`
remains the provider-neutral local and Gitee adapter, but the current GitHub
workflow does not restore the broader security, coverage, or SBOM commands.

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

## Current GitHub Quality, Toolchain, and Resource Gates

GitHub Actions [`package #39`](https://github.com/qaqsss117/Orange/actions/runs/30692142817)
completed successfully for commit `3a3f00039470068f4d049f4258fe2fd25db8e41c`.
Its Ubuntu `workspace-quality` job ran frontend formatting, ESLint, 18 Vitest
contract tests, the production frontend build, Rust workspace fmt/clippy/test,
and fmt/vet/test for both Go modules.

GitHub Actions [`package #40`](https://github.com/qaqsss117/Orange/actions/runs/30692521590)
completed successfully for commit `50db451bc5a90810049b2ca5258a8b9d6b038984`.
The workspace, Windows, Linux, macOS, Android, and iOS profiles all passed
`scripts/ci/check_toolchains.py`. `toolchains.toml` records minimum and
recommended Node, pnpm, Rust, Go, JDK, NDK, and Xcode versions; four negative
tests verify that missing, malformed, and out-of-range tools fail explicitly.

GitHub Actions [`package #41`](https://github.com/qaqsss117/Orange/actions/runs/30692855111)
completed successfully for commit `8019fdc90308082f69ab85bcf2a501d6f128d372`.
All six jobs passed. `pnpm check:frontend` ran the closed
`resources-manifest.json` schema and verified 64 repository files, normalized
paths, source files, unique IDs/paths, SHA-256 values, and release flags. The
same check is the first step of `pnpm build`; Tauri's `beforeBuildCommand`
therefore ran it in every Windows, Linux, macOS, Android, and iOS shell build.

## Acceptance Review

| Rule | Retained evidence | Result |
| ---: | --- | --- |
| 1 | The local Windows/Linux/Android shell results and `package #31` macOS/iOS launch probes cover all five shells, including the retained iOS first-screen screenshot. | Pass |
| 2 | `toolchains.toml`, the fail-closed preflight tests, and all six `package #40` profiles cover every required toolchain. | Pass |
| 3 | `package #39` and the superseding `package #41` `workspace-quality` job run TypeScript strict/build, ESLint, formatting, Vitest, Rust fmt/clippy/test, and both Go checks. | Pass |
| 4 | Isolated Windows/Android debug verification plus fresh GitHub-hosted checkouts, pinned setup actions, frozen lock installation, and five successful package builds demonstrate that no developer-global hidden configuration is required. | Pass |
| 5 | GitHub uses read-only repository permissions and managed secrets; production bootstrap/signing values remain outside Git, public logs, and artifact metadata, and temporary signing material is removed. | Pass |
| 6 | The closed resource schema and 64-file inventory run through both `workspace-quality` and all five `package #41` shell builds. | Pass |

`ARC-G0-001` is therefore `done`. `QA-G0-001` remains `review`: Kotlin/Swift
lint and unit checks, the general permission/SBOM/denylist/secret gates, and
branch protection are separate acceptance requirements and are not restored by
these focused architecture gates.

## Scope Boundary

The Apple runner and launch-evidence blocker is closed. Together with the
previous Windows, Linux, and Android evidence, acceptance rule 1 now has a
retained platform result.

This evidence does not claim Apple Network Extension entitlement, real-device
VPN operation, store approval, or release completion; those belong to later
Apple and release slices.

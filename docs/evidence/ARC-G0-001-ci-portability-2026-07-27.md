# ARC-G0-001 Provider-Neutral CI Evidence

Date: 2026-07-27

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

`.github/workflows/quality.yml` delegates quality commands to this entry. The
native Gitee Go adapters in `.workflow` invoke `portable-quality` through
`scripts/ci/run-gitee-cloud.sh`. That bootstrap uses only registered domestic
mirrors and pins the Python TOML compatibility package by version and hash.
Gitee's managed carrier is limited to portable checks; complete native jobs
continue to use the same Python boundary on platform hosts.

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

## Remaining External Evidence

The current host has no `xcodebuild` or `xcrun`, no GitHub CLI authentication,
and no GitHub mirror remote. The only repository remote is Gitee. Running the
iOS job locally fails closed with `ios-shell requires a macOS host with Xcode`.

`ARC-G0-001` therefore remains blocked until both of these external conditions
are supplied:

1. A macOS runner with pinned Xcode that can build and launch the macOS shell
   and build/launch the iOS simulator shell.
2. A retained successful Gitee Go run link after the checked-in `.workflow`
   files are pushed and the repository service is enabled. Complete native CI
   still requires trusted Linux, Windows, and macOS host groups.

This evidence does not claim Apple entitlement, signing, Network Extension, or
real-device completion; those belong to later Apple slices.

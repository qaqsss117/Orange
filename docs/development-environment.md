# Development Environment

## Pinned toolchains

The authoritative versions and mirror endpoints are in `toolchains.toml`.

| Tool | Minimum | Recommended/pinned |
| --- | --- | --- |
| Node.js | 22.22.0 | 22.23.1 LTS |
| pnpm | 11.9.0 | 11.9.0 |
| Rust/Cargo | 1.95.0 | 1.95.0 |
| Go | 1.25.0 | 1.25.5 |
| JDK | 17.0.17 | 17.0.17 |
| Android compile SDK | 36 | 36 |
| Android NDK | 29.0.14206865 | 29.0.14206865 |

Apple builds additionally require the Xcode/SDK version fixed by
`APL-G0-001`. Linux and macOS build hosts are required for native release
evidence; cross-compilation from Windows is not accepted as platform proof.

## Domestic mirrors

Run the mirror setup once from PowerShell:

```powershell
powershell -ExecutionPolicy Bypass -File scripts/dev/setup-mirrors.ps1 -Persist
```

This configures npmmirror, rsproxy, goproxy.cn, Aliyun package and Maven
repositories, and the Tencent Gradle distribution mirror. It writes only
mirror environment variables, Go's user environment file, and
`~/.gradle/init.d/orange-domestic-mirrors.gradle`.

Verify mirrors and toolchains with:

```powershell
powershell -ExecutionPolicy Bypass -File scripts/dev/check-mirrors.ps1
powershell -ExecutionPolicy Bypass -File scripts/dev/check-toolchain.ps1
```

Initialize Tauri's generated Android project with:

```powershell
pnpm android:init
```

The post-generation step replaces Gradle/Maven upstreams with approved domestic
mirrors and removes Tauri's default Leanback declaration. Do not call
`tauri android init` without subsequently running `pnpm android:configure`.

Do not silently replace a failed domestic mirror with an unregistered upstream
URL. Add or change a mirror in `toolchains.toml`, document why, then update the
verification script. `GOPROXY` intentionally has no `direct` fallback; a
goproxy.cn outage must fail closed instead of reaching an unregistered origin.

## Supply-chain evidence

Generate and validate the dependency, license, resource, and native artifact
evidence with:

```powershell
pnpm sbom
python scripts/security/check_supply_chain.py --sbom artifacts/sbom/orange.cdx.json
python scripts/security/check_build_artifacts.py artifacts/security/desktop-artifacts.json
```

The SBOM gate requires exact agreement between CycloneDX components and the
license report. Every required ecosystem must have either a checked-in lockfile
or an explicit empty reason. Desktop and Android CI jobs create debug artifact
manifests automatically; `unsigned-debug` artifacts are never release-allowed.
The same restriction applies to artifacts carrying an untrusted debug
signature; only a policy-approved release signature state is release-eligible.

## Bootstrap encryption

The portable development check creates an encrypted, non-routable fixture with
an ephemeral in-memory key:

```powershell
python scripts/ci/run.py bootstrap
```

It writes `artifacts/bootstrap/bootstrap.enc`, a non-sensitive manifest, and a
hash-only report. The fixture contains only `.invalid` test nodes. The command
verifies that the key and plaintext do not appear in process output, ciphertext,
or the manifest, and that repeated encryption uses distinct nonces.

Production CI uses the separate `bootstrap-release` job. Configure
`ORANGE_BOOTSTRAP_BUILD_KEY_HEX` and `ORANGE_BOOTSTRAP_CONFIG_JSON` as masked
secrets, then set the non-secret `ORANGE_BOOTSTRAP_CHANNEL`,
`ORANGE_BOOTSTRAP_PRODUCT_VERSION`, and `ORANGE_BOOTSTRAP_KEY_ID` variables.
Do not place any of these values in a workflow command, repository file, dotenv
file, CI artifact, or build log. The release job is intentionally not attached
to an automatic pipeline before the secret store and production node contract
are approved.

## Provider-neutral CI entry

CI providers must call the checked-in Python entry instead of maintaining a
second copy of the quality commands:

```powershell
python scripts/ci/run.py --list
python scripts/ci/run.py quality
python scripts/ci/run.py portable-quality
python scripts/ci/run.py bootstrap
python scripts/ci/run.py desktop-shell
```

The entry reads domestic mirror endpoints from `toolchains.toml` and injects
them into every child process. Platform jobs are `desktop-shell`,
`android-shell`, and `ios-shell`; the iOS job rejects non-macOS hosts and the
Android job verifies the pinned NDK before building.

`quality` checks the complete Rust workspace and requires native platform
libraries. `portable-quality` checks security, frontend, the three portable
Rust crates, Go, and the supply chain without compiling the Tauri shell. It is
the appropriate gate for a restricted Linux build image, not a replacement
for the complete native jobs.

## Gitee Go

The repository contains native Gitee Go adapters in `.workflow`:

| File | Automatic trigger |
| --- | --- |
| `MasterPipeline.yml` | Push to `master` or `main` |
| `BranchPipeline.yml` | Push to any other branch |
| `PRPipeline.yml` | Create or update a PR targeting `master` or `main` |

Each adapter uses Gitee's `build@python` carrier and calls
`scripts/ci/run-gitee-cloud.sh`. The script installs the fixed Node.js, pnpm,
Rust, and Go versions from registered domestic mirrors, verifies downloaded
archives, then runs `portable-quality`. Gitee Go automatically checks out push
commits and pre-merges the source and target branches for PR builds.

To enable the checked-in pipelines:

1. Commit and push `.workflow` and the related CI files to Gitee.
2. Open the repository's **Gitee Go** service and enable it for the repository.
3. Push a commit or open/update a PR to use the automatic triggers above.
4. For an ad-hoc run, open a pipeline in Gitee Go, select **Run pipeline**, and
   choose the branch or commit offered by Gitee.

The cloud carrier cannot prove desktop or mobile shells: its documented Linux
image does not provide the pinned Apple/Android SDKs or the Linux WebKit 4.1
development stack. Configure trusted Gitee Go host groups with the pinned
toolchains and invoke `python scripts/ci/run.py quality`, `desktop-shell`,
`android-shell`, or `ios-shell` there. Do not lower repository toolchain
versions to fit Gitee's older managed images.

Official references: [three-step setup](https://help.gitee.com/gitee-go/get-started-in-3-steps),
[pipeline triggers](https://help.gitee.com/gitee-go/pipeline/trigger), and
[build plugins](https://help.gitee.com/gitee-go/plugin/ci-build).

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

This configures npmmirror, rsproxy, goproxy.cn, Aliyun Maven repositories, and
the Tencent Gradle distribution mirror. It writes only mirror environment
variables, Go's user environment file, and
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
verification script.

## Provider-neutral CI entry

CI providers must call the checked-in Python entry instead of maintaining a
second copy of the quality commands:

```powershell
python scripts/ci/run.py --list
python scripts/ci/run.py quality
python scripts/ci/run.py desktop-shell
```

The entry reads domestic mirror endpoints from `toolchains.toml` and injects
them into every child process. Platform jobs are `desktop-shell`,
`android-shell`, and `ios-shell`; the iOS job rejects non-macOS hosts and the
Android job verifies the pinned NDK before building.

`.github/workflows/quality.yml` is the versioned GitHub Actions adapter. A
Gitee Enterprise pipeline or self-hosted runner may invoke the same jobs after
installing the pinned tools. Do not invent an unverified Gitee YAML format;
configure its pipeline in the supported UI and use `scripts/ci/run.py` as the
command boundary.

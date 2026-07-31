# Development Environment

## Toolchains

The pinned versions live in `toolchains.toml`.

| Tool | Version |
| --- | --- |
| Node.js | 22.23.1 |
| pnpm | 11.9.0 |
| Rust/Cargo | 1.95.0 |
| Go | 1.25.5 |
| JDK | 17.0.17 |
| Android compile SDK | 36 |
| Android NDK | 29.0.14206865 |

GitHub Actions uses the official npm, Rust, crates.io, Go, Ubuntu, Gradle,
Google Maven and Maven Central sources. The workflow restores and saves pnpm,
Cargo, Go and Gradle caches. Apple builds run on macOS with the installed Xcode
toolchain.

## Local setup

Install frontend dependencies and build the web bundle:

```powershell
pnpm install --frozen-lockfile
pnpm build
```

Desktop release builds require an encrypted production bootstrap resource. Set
the following environment variables without committing them:

- `ORANGE_BOOTSTRAP_BUILD_KEY_HEX`
- `ORANGE_BOOTSTRAP_CONFIG_JSON`
- `ORANGE_BOOTSTRAP_CHANNEL`
- `ORANGE_BOOTSTRAP_PRODUCT_VERSION`
- `ORANGE_BOOTSTRAP_KEY_ID`

Then build the resource and package the current desktop platform:

```powershell
python scripts/ci/build_bootstrap_resource.py
pnpm tauri build --ci
```

Windows additionally requires `ORANGE_WINDOWS_SIGNER_SHA1` and a certificate
installed in the current user's certificate store. Its preparation step builds
the service, installer helper and pinned Data Plane before NSIS packaging:

```powershell
python scripts/ci/prepare_windows_bundle.py
pnpm tauri build --bundles nsis --ci
```

Initialize and package Android with:

```powershell
pnpm tauri android init --ci
python scripts/dev/configure-generated-android.py
pnpm tauri android build --apk --aab --ci
```

The generated Android project uses the official Gradle and Maven repositories.
Release signing reads `src-tauri/gen/android/keystore.properties`; the GitHub
workflow creates this ignored file from repository secrets.

Initialize and package iOS on macOS with:

```bash
pnpm tauri ios init --ci
pnpm tauri ios build --ci --method app-store-connect
```

## GitHub variables

Configure these repository variables before running `.github/workflows/quality.yml`:

- `ORANGE_BOOTSTRAP_CHANNEL`
- `ORANGE_BOOTSTRAP_PRODUCT_VERSION`
- `ORANGE_BOOTSTRAP_KEY_ID`
- `ORANGE_WINDOWS_SIGNER_SHA1`
- `APPLE_SIGNING_IDENTITY`
- `APPLE_TEAM_ID`
- `APPLE_DEVELOPMENT_TEAM`

Configure these repository secrets:

- `ORANGE_BOOTSTRAP_BUILD_KEY_HEX`
- `ORANGE_BOOTSTRAP_CONFIG_JSON`
- `WINDOWS_CERTIFICATE`
- `WINDOWS_CERTIFICATE_PASSWORD`
- `APPLE_CERTIFICATE`
- `APPLE_CERTIFICATE_PASSWORD`
- `APPLE_ID`
- `APPLE_PASSWORD`
- `ANDROID_KEYSTORE_BASE64`
- `ANDROID_KEYSTORE_PASSWORD`
- `ANDROID_KEY_ALIAS`
- `ANDROID_KEY_PASSWORD`
- `APPLE_API_ISSUER`
- `APPLE_API_KEY`
- `APPLE_API_PRIVATE_KEY`
- `IOS_CERTIFICATE`
- `IOS_CERTIFICATE_PASSWORD`
- `IOS_MOBILE_PROVISION`

Certificate, keystore, provisioning and bootstrap values must remain in GitHub
Secrets. The workflow only uploads the five platform installation artifacts.

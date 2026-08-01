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
| Xcode | 16.4.0 |
| Android compile SDK | 36 |
| Android NDK | 29.0.14206865 |

GitHub Actions uses the official npm, Rust, crates.io, Go, Ubuntu, Gradle,
Google Maven and Maven Central sources. The workflow restores and saves pnpm,
Cargo, Go and Gradle caches. Apple builds run on macOS with the installed Xcode
toolchain.

Run the platform-specific preflight before local builds:

```powershell
python scripts/ci/check_toolchains.py windows
```

Profiles are `workspace`, `windows`, `linux`, `macos`, `android`, and `ios`.
The preflight fails when a required tool is missing or older than the recorded
minimum. It reports compatible versions that differ from the recommendation;
Android additionally requires the pinned platform, build-tools, and NDK
directories. GitHub Actions runs the matching profile after tool setup in every
quality and package job.

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
pnpm tauri ios build --ci --export-method app-store-connect
```

## GitHub variables

Configure these repository variables before running `.github/workflows/package.yml`:

- `ORANGE_BOOTSTRAP_CHANNEL`
- `ORANGE_BOOTSTRAP_PRODUCT_VERSION`
- `ORANGE_BOOTSTRAP_KEY_ID`
- `APPLE_DEVELOPMENT_TEAM`
- `APPLE_API_ISSUER`
- `APPLE_API_KEY`
- `MACOS_APP_SIGNING_IDENTITY`
- `MACOS_INSTALLER_SIGNING_IDENTITY`

Configure these repository secrets:

- `ORANGE_BOOTSTRAP_BUILD_KEY_HEX`
- `ORANGE_BOOTSTRAP_CONFIG_JSON`
- `WINDOWS_CERTIFICATE`
- `WINDOWS_CERTIFICATE_PASSWORD`
- `ANDROID_KEYSTORE_BASE64`
- `ANDROID_KEYSTORE_PASSWORD`
- `ANDROID_KEY_ALIAS`
- `ANDROID_KEY_PASSWORD`
- `APPLE_API_PRIVATE_KEY`
- `MACOS_APP_CERTIFICATE`
- `MACOS_APP_CERTIFICATE_PASSWORD`
- `MACOS_INSTALLER_CERTIFICATE`
- `MACOS_INSTALLER_CERTIFICATE_PASSWORD`
- `MACOS_PROVISIONING_PROFILE`

Certificate, keystore and bootstrap values must remain in GitHub Secrets. The
GitHub artifact step includes five platform installation artifacts;
every successful iOS package is additionally uploaded to App Store Connect.
Windows CI derives the signing thumbprint directly from the imported
`WINDOWS_CERTIFICATE`; no separate thumbprint variable is required.

`APPLE_API_PRIVATE_KEY` contains the raw App Store Connect `.p8` private key.
The matching key ID and issuer ID use the `APPLE_API_KEY` and
`APPLE_API_ISSUER` repository variables. The API key must have access to
Certificates, Identifiers & Profiles and permission to upload builds.

The iOS and macOS bundle IDs are both fixed to `com.orangevpn.cn`. The values
live in their Tauri platform configuration files.
iOS uses Xcode automatic signing with the API key and
`APPLE_DEVELOPMENT_TEAM`. macOS builds a Mac App Store package: it imports
the Apple Distribution application certificate, Mac Installer Distribution
certificate and Mac App Store provisioning profile, verifies the profile's
bundle ID and team, adds the required App Sandbox signing entitlement, signs
the application and creates a signed `.pkg` installer. iOS uploads after every
successful package build.
Version tags upload macOS automatically, and a manually dispatched workflow can
request a macOS upload with `upload_macos`.

The two macOS certificates must include their private keys and be exported as
base64-encoded PKCS #12 files. Apple does not retain those private keys, so they
cannot be recovered with the App Store Connect API key. The provisioning
profile secret is the base64 encoding of the `.provisionprofile` file.

This packaging path does not create the product's Packet Tunnel extension.
Shipping functional VPN support through the Mac App Store still requires the
approved Network Extension entitlement, an extension App ID and profile, App
Group wiring and the native extension implementation tracked by the Apple
platform slices.

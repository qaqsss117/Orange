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
package job.

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
- `ORANGE_BOOTSTRAP_SIGNING_KEY_HEX`
- `ORANGE_BOOTSTRAP_SIGNING_KEY_ID`
- `ORANGE_BOOTSTRAP_ENVELOPE_URLS`
- `ORANGE_BOOTSTRAP_MINIMUM_CLIENT_VERSION`
- `ORANGE_BOOTSTRAP_MANIFEST_URLS`
- `ORANGE_BOOTSTRAP_TXT_NAMES`
- `ORANGE_BOOTSTRAP_TXT_MANIFEST_URLS`
- `ORANGE_BOOTSTRAP_TXT_ENVELOPE_URLS`
- `ORANGE_BOOTSTRAP_SIGNING_PUBLIC_KEYS`
- `ORANGE_BOOTSTRAP_TXT_SEQUENCE`
- `ORANGE_BOOTSTRAP_TXT_EXPIRES_AT_UNIX`

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

When `ORANGE_WINDOWS_SIGNER_SHA1` is set, the control-plane sidecar integrity
check pins the Authenticode signer SHA-1 thumbprint instead of a byte hash, so
the re-signing performed by the NSIS bundler is supported. Setting the variable
locally therefore requires a signed sidecar produced by
`python scripts/ci/prepare_windows_bundle.py`; with the variable unset,
development builds fall back to the compile-time SHA-256 pin and expect an
unsigned sidecar from `pnpm prepare:desktop`. The signer thumbprint is the
trust anchor: an untrusted chain (self-signed release certificate) is accepted
when the thumbprint matches, while unsigned or tampered binaries are rejected.

Initialize and package Android with:

```powershell
go install golang.org/x/mobile/cmd/gomobile@v0.0.0-20260820023541-8e8303b9da6c
python scripts/ci/build_android_control_plane.py
pnpm tauri android init --ci
python scripts/dev/configure-generated-android.py
pnpm tauri android build --apk --aab --ci
```

The generated Android project uses the official Gradle and Maven repositories.
Release signing reads `src-tauri/gen/android/keystore.properties`; the GitHub
workflow creates this ignored file from repository variables.

Initialize and package iOS on macOS with:

```bash
pnpm tauri ios init --ci
pnpm tauri ios build --ci --export-method app-store-connect
```

## GitHub variables

Configure these repository variables before running `.github/workflows/package.yml`:

The complete OSS object mapping, Cloudflare TXT procedure, and fault-injection
matrix are documented in `docs/bootstrap-release-and-testing.md`.

- `ORANGE_BOOTSTRAP_CHANNEL`
- `ORANGE_BOOTSTRAP_PRODUCT_VERSION`
- `ORANGE_BOOTSTRAP_KEY_ID`
- `ORANGE_BOOTSTRAP_SIGNING_KEY_ID`
- `ORANGE_BOOTSTRAP_ENVELOPE_URLS`
- `ORANGE_BOOTSTRAP_MINIMUM_CLIENT_VERSION`
- `ORANGE_BOOTSTRAP_MANIFEST_URLS`
- `ORANGE_BOOTSTRAP_TXT_NAMES`
- `ORANGE_BOOTSTRAP_TXT_MANIFEST_URLS`
- `ORANGE_BOOTSTRAP_TXT_ENVELOPE_URLS`
- `ORANGE_BOOTSTRAP_SIGNING_PUBLIC_KEYS`
- `ORANGE_BOOTSTRAP_TXT_SEQUENCE`
- `ORANGE_BOOTSTRAP_TXT_EXPIRES_AT_UNIX`
- `ORANGE_WINDOWS_STORE_PRODUCT_ID`
- `ORANGE_ANDROID_PACKAGE_ID`
- `ORANGE_ANDROID_VERSION_CODE`
- `ORANGE_ANDROID_VERSION_NAME`
- `ORANGE_ANDROID_SIGNING_CERT_SHA256`
- `ORANGE_ANDROID_UPDATE_MANIFEST_URLS`
- `ORANGE_ANDROID_UPDATE_TXT_NAMES`
- `ORANGE_ANDROID_UPDATE_TXT_MANIFEST_URLS`
- `ORANGE_ANDROID_APK_MIRROR_URLS`
- `ORANGE_ANDROID_UPDATE_EXPIRES_AT_UNIX`
- `ORANGE_ANDROID_UPDATE_TXT_SEQUENCE`
- `APPLE_DEVELOPMENT_TEAM`
- `APPLE_API_ISSUER`
- `APPLE_API_KEY`
- `ORANGE_BOOTSTRAP_BUILD_KEY_HEX`
- `ORANGE_BOOTSTRAP_CONFIG_JSON`
- `ORANGE_BOOTSTRAP_SIGNING_KEY_HEX`
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
- `TAURI_SIGNING_PRIVATE_KEY`
- `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`

All values above are configured as GitHub Actions repository Variables. GitHub
does not encrypt or automatically mask Variables like Secrets, so private keys,
certificate passwords, bootstrap plaintext, and keystores can be exposed by
workflow logs or anyone who can read repository Variables. Restrict repository
administration and workflow write access accordingly, and do not print these
values. The GitHub artifact step includes five platform installation artifacts;
every successful iOS package is additionally uploaded to App Store Connect.
Windows CI derives the signing thumbprint directly from the imported
`WINDOWS_CERTIFICATE`; no separate thumbprint variable is required.

`APPLE_API_PRIVATE_KEY` contains the raw App Store Connect `.p8` private key.
The matching key ID and issuer ID use the `APPLE_API_KEY` and
`APPLE_API_ISSUER` repository variables. The API key must have access to
Certificates, Identifiers & Profiles and permission to upload builds.

The iOS and macOS bundle IDs are both fixed to `com.orangevpn.cn`. The values
live in their Tauri platform configuration files. iOS uses Xcode automatic
signing with the API key and `APPLE_DEVELOPMENT_TEAM`, and successful iOS
packages can be uploaded to App Store Connect.

macOS is distributed outside the Mac App Store. CI imports Developer ID
Application and Developer ID Installer PKCS #12 certificates, derives the
signing identities and team ID from those certificates, builds a universal2
application plus privileged helper and data plane, signs a full PKG, submits it
for notarization, staples the ticket, and verifies Gatekeeper acceptance. No
macOS provisioning profile or App Sandbox entitlement is used.

The two macOS certificates must include their private keys and be exported as
base64-encoded PKCS #12 files. Apple does not retain those private keys, so they
cannot be recovered with the App Store Connect API key.

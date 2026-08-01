# Android Native Boundary

Tauri mobile plugin code, `VpnService`, and the narrow libbox bridge live here.
No source or binary from the untrusted Android reference may enter this tree.

`src/main` contains the managed Android platform implementation. The generated
Tauri project is not a source of truth: `scripts/dev/configure-generated-android.py`
copies these files into it.

`AndroidSecretStore` keeps its AES-256-GCM key non-exportable in Android
Keystore. Private SharedPreferences stores only versioned IV/ciphertext
payloads for the three fixed user credential keys. It never accepts an
arbitrary key name, clears every caller buffer after a store attempt, and
exposes only stable, redacted error codes. Logout removes all three ciphertexts and destroys their
dedicated Keystore key.

`AndroidSecretStorePlugin` is the internal Rust-to-Kotlin adapter. It accepts
only protocol version 1, the three fixed user credential keys, canonical Base64
values, and fixed storage operations. The corresponding Rust plugin has no WebView invoke
handler or capability permission.

`src/test` contains host-JVM contract tests for the protocol version, credential
key allowlist, canonical Base64 validation, size limits, stable errors, and
failure-path buffer clearing. The Android package job copies those tests into
the generated project, then runs `scripts/ci/check_android_native_quality.py`
after producing the release packages. The gate downloads the fixed ktlint
1.8.0 all-in-one JAR from Maven Central, verifies its pinned SHA-256, checks the
managed sources, their exact generated copies, and the project-owned
`MainActivity`, then runs the fixed universal debug JUnit suite and Android
lint. Tauri/Wry's build-generated `generated/**` sources are excluded because
they are third-party output and are rewritten by every Tauri build. The ktlint
identity, command, output, and exit code are retained with the Android
artifact.

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

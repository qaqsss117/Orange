# Android Native Boundary

Tauri mobile plugin code, `VpnService`, and the narrow libbox bridge live here.
No source or binary from the untrusted Android reference may enter this tree.

`src/main` contains the managed Android platform implementation and
`src/androidTest` contains device-only verification. The generated Tauri
project is not a source of truth: `scripts/dev/configure-generated-android.py`
copies these files into it and fails if the result differs.

`AndroidSecretStore` keeps its AES-256-GCM key non-exportable in Android
Keystore. Private SharedPreferences stores only versioned IV/ciphertext
payloads for the two fixed token keys. It never accepts an arbitrary key name,
clears every caller buffer after a store attempt, and exposes only stable,
redacted error codes. Logout removes both ciphertexts and destroys their
dedicated Keystore key.

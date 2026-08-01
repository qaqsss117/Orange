# Apple Native Boundary

The internal iOS secret-store plugin lives in `secret-store/`. It uses only the
application's private Keychain namespace and therefore does not require a
Keychain access group or new entitlement. The Rust carrier crate links this
Swift package into the generated Tauri iOS project without granting WebView
permissions.

`secret-store-core/` owns the fixed protocol version, credential keys, stable
errors, canonical Base64 validation, and 16 KiB value limit without depending
on Tauri or a generated Apple project. The iOS package imports the same core.

Swift app extensions, App Group IPC, and Network Extension code remain deferred
until Apple identifiers and entitlements are approved.

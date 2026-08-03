use std::{
    env,
    fmt::Write as _,
    fs::{self, File},
    io::Read,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use orange_bootstrap::{BootstrapKey, BootstrapManifest, decrypt};
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

const BOOTSTRAP_KEY_ENV: &str = "ORANGE_BOOTSTRAP_BUILD_KEY_HEX";
const SIGNER_THUMBPRINT_ENV: &str = "ORANGE_WINDOWS_SIGNER_SHA1";
const MAX_BOOTSTRAP_ENVELOPE_BYTES: usize = 128 * 1024;
const MAX_BOOTSTRAP_MANIFEST_BYTES: usize = 16 * 1024;

fn main() {
    let target = env::var("TARGET").expect("Cargo TARGET is unavailable");
    emit_control_plane_integrity(&target);
    emit_embedded_bootstrap(&target);
    let commands = if target.contains("android") || target.contains("ios") {
        orange_domain::BASE_COMMANDS
    } else {
        orange_domain::REGISTERED_COMMANDS
    };
    let attributes = tauri_build::Attributes::new()
        .app_manifest(tauri_build::AppManifest::new().commands(commands));
    tauri_build::try_build(attributes).expect("failed to build Orange application manifest");
}

fn emit_embedded_bootstrap(target: &str) {
    println!("cargo:rustc-check-cfg=cfg(orange_embedded_bootstrap)");
    println!("cargo:rerun-if-env-changed={BOOTSTRAP_KEY_ENV}");
    let release_dir =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../artifacts/bootstrap/release");
    let envelope_path = release_dir.join("bootstrap.enc");
    let manifest_path = release_dir.join("bootstrap.manifest.json");
    println!("cargo:rerun-if-changed={}", envelope_path.display());
    println!("cargo:rerun-if-changed={}", manifest_path.display());

    let Some(key_hex) = env::var_os(BOOTSTRAP_KEY_ENV) else {
        return;
    };
    if target.contains("android") || target.contains("ios") {
        panic!("embedded production bootstrap is not implemented for mobile targets");
    }
    let key_hex = Zeroizing::new(
        key_hex
            .into_string()
            .unwrap_or_else(|_| panic!("bootstrap build key is not valid UTF-8")),
    );
    let mut key_bytes = decode_key(&key_hex);
    let key = BootstrapKey::from_bytes(key_bytes);
    let envelope = read_bounded(&envelope_path, MAX_BOOTSTRAP_ENVELOPE_BYTES);
    let manifest_bytes = read_bounded(&manifest_path, MAX_BOOTSTRAP_MANIFEST_BYTES);
    let manifest: BootstrapManifest = serde_json::from_slice(&manifest_bytes)
        .unwrap_or_else(|_| panic!("release bootstrap manifest is invalid"));
    if manifest.channel != "production" || manifest.product_version != env!("CARGO_PKG_VERSION") {
        panic!("release bootstrap manifest does not match this application build");
    }
    let now_unix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_else(|_| panic!("system clock is unavailable"))
        .as_secs();
    let mut secret = decrypt(&envelope, &manifest, &key, now_unix)
        .unwrap_or_else(|_| panic!("release bootstrap cannot be authenticated"));
    secret.clear();

    let output_dir = PathBuf::from(env::var_os("OUT_DIR").expect("Cargo OUT_DIR is unavailable"));
    let key_path = output_dir.join("orange-bootstrap.key");
    fs::write(&key_path, key_bytes).expect("failed to prepare embedded bootstrap key");
    key_bytes.fill(0);
    println!("cargo:rustc-cfg=orange_embedded_bootstrap");
    println!(
        "cargo:rustc-env=ORANGE_BOOTSTRAP_ENVELOPE_PATH={}",
        envelope_path.display()
    );
    println!(
        "cargo:rustc-env=ORANGE_BOOTSTRAP_MANIFEST_PATH={}",
        manifest_path.display()
    );
    println!(
        "cargo:rustc-env=ORANGE_BOOTSTRAP_KEY_PATH={}",
        key_path.display()
    );
}

fn decode_key(value: &str) -> [u8; 32] {
    if value.len() != 64 || !value.is_ascii() {
        panic!("bootstrap build key must be 32 bytes encoded as hexadecimal");
    }
    let mut output = [0_u8; 32];
    for (index, byte) in output.iter_mut().enumerate() {
        let offset = index * 2;
        *byte =
            (decode_hex(value.as_bytes()[offset]) << 4) | decode_hex(value.as_bytes()[offset + 1]);
    }
    output
}

fn decode_hex(value: u8) -> u8 {
    match value {
        b'0'..=b'9' => value - b'0',
        b'a'..=b'f' => value - b'a' + 10,
        b'A'..=b'F' => value - b'A' + 10,
        _ => panic!("bootstrap build key must be 32 bytes encoded as hexadecimal"),
    }
}

fn read_bounded(path: &Path, limit: usize) -> Vec<u8> {
    let value = fs::read(path).unwrap_or_else(|_| {
        panic!("missing release bootstrap resource; run scripts/ci/build_bootstrap_resource.py")
    });
    if value.is_empty() || value.len() > limit {
        panic!("release bootstrap resource size is invalid");
    }
    value
}

fn emit_control_plane_integrity(target: &str) {
    if target.contains("android") || target.contains("ios") {
        return;
    }
    println!("cargo:rustc-check-cfg=cfg(orange_control_plane_signer_pin)");
    println!("cargo:rerun-if-env-changed={SIGNER_THUMBPRINT_ENV}");
    if target.contains("windows")
        && let Some(value) = env::var_os(SIGNER_THUMBPRINT_ENV)
    {
        let value = value
            .into_string()
            .unwrap_or_else(|_| panic!("ORANGE_WINDOWS_SIGNER_SHA1 is not valid UTF-8"));
        let thumbprint = value.trim().to_uppercase();
        if thumbprint.len() != 40
            || !thumbprint
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'A'..=b'F').contains(&byte))
        {
            panic!("ORANGE_WINDOWS_SIGNER_SHA1 must be a 40-character SHA-1 thumbprint");
        }
        // The bundler re-signs bundled binaries after this build script runs,
        // so a byte-hash pin cannot survive packaging; pin the Authenticode
        // signer thumbprint instead and verify it at runtime.
        println!("cargo:rustc-cfg=orange_control_plane_signer_pin");
        println!("cargo:rustc-env=ORANGE_CONTROL_PLANE_SIGNER_SHA1={thumbprint}");
        return;
    }
    if target.contains("darwin") {
        // The .app bundle (including the sidecar) is code-signed and notarized
        // as a unit after this script runs; no byte-hash pin is possible.
        return;
    }
    let extension = if target.contains("windows") {
        ".exe"
    } else {
        ""
    };
    let relative = PathBuf::from("../artifacts/tauri-sidecars")
        .join(format!("orange-control-plane-{target}{extension}"));
    println!("cargo:rerun-if-changed={}", relative.display());
    let mut file = File::open(&relative).unwrap_or_else(|_| {
        panic!(
            "missing prepared Control Plane sidecar {}; run scripts/ci/prepare_control_plane_sidecar.py",
            relative.display()
        )
    });
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .expect("failed to hash prepared Control Plane sidecar");
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    let mut digest = String::with_capacity(64);
    for byte in hasher.finalize() {
        write!(&mut digest, "{byte:02x}").expect("failed to format sidecar digest");
    }
    println!("cargo:rustc-env=ORANGE_CONTROL_PLANE_SIDECAR_SHA256={digest}");
}

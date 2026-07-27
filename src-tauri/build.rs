use std::{env, fmt::Write as _, fs::File, io::Read, path::PathBuf};

use sha2::{Digest, Sha256};

fn main() {
    emit_control_plane_integrity();
    let attributes =
        tauri_build::Attributes::new().app_manifest(tauri_build::AppManifest::new().commands(&[
            orange_domain::GET_PLANE_STATE_COMMAND,
            orange_domain::GET_RUNTIME_INFO_COMMAND,
        ]));
    tauri_build::try_build(attributes).expect("failed to build Orange application manifest");
}

fn emit_control_plane_integrity() {
    let target = env::var("TARGET").expect("Cargo TARGET is unavailable");
    if target.contains("android") || target.contains("ios") {
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

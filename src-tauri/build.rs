fn main() {
    let attributes = tauri_build::Attributes::new().app_manifest(
        tauri_build::AppManifest::new().commands(&[orange_domain::GET_RUNTIME_INFO_COMMAND]),
    );
    tauri_build::try_build(attributes).expect("failed to build Orange application manifest");
}

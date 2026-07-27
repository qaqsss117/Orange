#![forbid(unsafe_code)]

use orange_domain::{
    CommandError, PlaneStateRequest, PlaneStateResponse, RuntimeInfoRequest, RuntimeInfoResponse,
};
use orange_platform::{FileSettingsStore, SettingsStorage};
use tauri::Manager;

#[cfg(target_os = "android")]
mod android_secret_store;
#[cfg(target_os = "ios")]
use orange_ios_secret_store as ios_secret_store;
#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub mod control_plane;
mod planes;

#[tauri::command]
fn get_runtime_info(request: RuntimeInfoRequest) -> Result<RuntimeInfoResponse, CommandError> {
    request.validate()?;
    Ok(RuntimeInfoResponse::new(env!("CARGO_PKG_VERSION")))
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tauri::command]
fn get_plane_state(
    request: PlaneStateRequest,
    planes: tauri::State<'_, planes::ManagedPlanes>,
    control_plane: tauri::State<'_, control_plane::ManagedControlPlane>,
) -> Result<PlaneStateResponse, CommandError> {
    request.validate()?;
    control_plane.status();
    planes.snapshot()
}

#[cfg(any(target_os = "android", target_os = "ios"))]
#[tauri::command]
fn get_plane_state(
    request: PlaneStateRequest,
    planes: tauri::State<'_, planes::ManagedPlanes>,
) -> Result<PlaneStateResponse, CommandError> {
    request.validate()?;
    planes.snapshot()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let planes = planes::ManagedPlanes::default();
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    let control_plane = control_plane::ManagedControlPlane::with_state(
        planes
            .control_handle()
            .expect("failed to initialize shared Control Plane state"),
    );
    let builder = tauri::Builder::default().manage(planes);
    #[cfg(target_os = "android")]
    let builder = builder.plugin(android_secret_store::init());
    #[cfg(target_os = "ios")]
    let builder = builder.plugin(ios_secret_store::init());
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    let builder = builder.manage(control_plane);
    let builder = builder.setup(|app| {
        let store = FileSettingsStore::new(app.path().app_data_dir()?)?;
        let _ = store.load()?;
        app.manage(store);
        Ok(())
    });
    builder
        .invoke_handler(tauri::generate_handler![get_plane_state, get_runtime_info])
        .run(tauri::generate_context!())
        .expect("failed to run Orange application");
}

#[cfg(test)]
mod tests {
    use orange_domain::{DOMAIN_SCHEMA_VERSION, ErrorCode};

    use super::*;

    #[test]
    fn runtime_info_command_validates_the_request() {
        let response = get_runtime_info(RuntimeInfoRequest::current()).unwrap();
        assert_eq!(response.schema_version, DOMAIN_SCHEMA_VERSION);
        assert_eq!(response.product_name, "Orange");

        let error = get_runtime_info(RuntimeInfoRequest { schema_version: 2 }).unwrap_err();
        assert_eq!(error.code(), ErrorCode::Validation);
    }

    #[test]
    fn plane_state_request_validates_before_adapter_access() {
        let error = PlaneStateRequest { schema_version: 2 }
            .validate()
            .unwrap_err();
        assert_eq!(error.code(), ErrorCode::Validation);
    }
}

#![forbid(unsafe_code)]

use orange_domain::{CommandError, RuntimeInfoRequest, RuntimeInfoResponse};

#[cfg(target_os = "android")]
mod android_secret_store;
#[cfg(target_os = "ios")]
use orange_ios_secret_store as ios_secret_store;
#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub mod control_plane;

#[tauri::command]
fn get_runtime_info(request: RuntimeInfoRequest) -> Result<RuntimeInfoResponse, CommandError> {
    request.validate()?;
    Ok(RuntimeInfoResponse::new(env!("CARGO_PKG_VERSION")))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default();
    #[cfg(target_os = "android")]
    let builder = builder.plugin(android_secret_store::init());
    #[cfg(target_os = "ios")]
    let builder = builder.plugin(ios_secret_store::init());
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    let builder = builder.manage(control_plane::ManagedControlPlane::default());
    builder
        .invoke_handler(tauri::generate_handler![get_runtime_info])
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
}

#![forbid(unsafe_code)]

use orange_domain::{CommandError, RuntimeInfoRequest, RuntimeInfoResponse};

#[tauri::command]
fn get_runtime_info(request: RuntimeInfoRequest) -> Result<RuntimeInfoResponse, CommandError> {
    request.validate()?;
    Ok(RuntimeInfoResponse::new(env!("CARGO_PKG_VERSION")))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
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

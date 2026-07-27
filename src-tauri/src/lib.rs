#![forbid(unsafe_code)]

use orange_domain::{
    CommandError, PlaneStateRequest, PlaneStateResponse, RuntimeInfoRequest, RuntimeInfoResponse,
};
use orange_platform::{DiagnosticsHub, FileSettingsStore, SettingsStorage};
use tauri::Manager;

#[cfg(not(any(target_os = "android", target_os = "ios")))]
use orange_domain::{
    AccountRefreshRequest, AccountResponse, AuthPublicResponse, AuthSessionRequest,
    AuthSessionResponse, BusinessInitializationResponse, InitializeBusinessRequest,
    LoginCommandRequest, RegisterCommandRequest, SubscriptionPublicResponse,
    SubscriptionRefreshRequest,
};
#[cfg(not(any(target_os = "android", target_os = "ios")))]
use std::sync::Arc;

#[cfg(not(any(target_os = "android", target_os = "ios")))]
use orange_platform::{
    BusinessApiService, BusinessCommandClient, BusinessServiceError, DesktopSecretStore,
    SystemClock,
};

#[cfg(target_os = "android")]
mod android_secret_store;
#[cfg(target_os = "ios")]
use orange_ios_secret_store as ios_secret_store;
#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub mod control_plane;
mod planes;

#[cfg(not(any(target_os = "android", target_os = "ios")))]
type DesktopBusinessService =
    BusinessApiService<Arc<control_plane::ManagedControlPlane>, DesktopSecretStore>;

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
    control_plane: tauri::State<'_, Arc<control_plane::ManagedControlPlane>>,
) -> Result<PlaneStateResponse, CommandError> {
    request.validate()?;
    control_plane.status();
    planes.snapshot()
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tauri::command]
fn initialize_business(
    request: InitializeBusinessRequest,
    service: tauri::State<'_, DesktopBusinessService>,
) -> Result<BusinessInitializationResponse, CommandError> {
    request.validate()?;
    service.initialize().map_err(map_business_error)
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tauri::command]
fn login(
    request: LoginCommandRequest,
    service: tauri::State<'_, DesktopBusinessService>,
) -> Result<AuthPublicResponse, CommandError> {
    let request = request.validate()?;
    service.login(request).map_err(map_business_error)
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tauri::command]
fn register(
    request: RegisterCommandRequest,
    service: tauri::State<'_, DesktopBusinessService>,
) -> Result<AuthPublicResponse, CommandError> {
    let request = request.validate()?;
    service.register(request).map_err(map_business_error)
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tauri::command]
fn get_auth_session(
    request: AuthSessionRequest,
    service: tauri::State<'_, DesktopBusinessService>,
) -> Result<AuthSessionResponse, CommandError> {
    request.validate()?;
    Ok(service.session())
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tauri::command]
fn refresh_account(
    request: AccountRefreshRequest,
    service: tauri::State<'_, DesktopBusinessService>,
) -> Result<AccountResponse, CommandError> {
    request.validate()?;
    service.refresh_account().map_err(map_business_error)
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tauri::command]
fn refresh_subscription(
    request: SubscriptionRefreshRequest,
    service: tauri::State<'_, DesktopBusinessService>,
) -> Result<SubscriptionPublicResponse, CommandError> {
    request.validate()?;
    service.refresh_subscription().map_err(map_business_error)
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn map_business_error(error: BusinessServiceError) -> CommandError {
    CommandError::from_code(error.public_error_code())
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
    let control_plane = Arc::new(control_plane::ManagedControlPlane::with_state(
        planes
            .control_handle()
            .expect("failed to initialize shared Control Plane state"),
    ));
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    let business_client = Arc::new(BusinessCommandClient::new(
        Arc::clone(&control_plane),
        DesktopSecretStore::new(),
    ));
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    let business_service = BusinessApiService::new(Arc::clone(&business_client), SystemClock);
    let builder = tauri::Builder::default()
        .manage(planes)
        .manage(DiagnosticsHub::default());
    #[cfg(target_os = "android")]
    let builder = builder.plugin(android_secret_store::init());
    #[cfg(target_os = "ios")]
    let builder = builder.plugin(ios_secret_store::init());
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    let builder = builder
        .manage(control_plane)
        .manage(business_client)
        .manage(business_service);
    let builder = builder.setup(|app| {
        let store = FileSettingsStore::new(app.path().app_data_dir()?)?;
        let _ = store.load()?;
        app.manage(store);
        Ok(())
    });
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    let builder = builder.invoke_handler(tauri::generate_handler![
        get_plane_state,
        get_runtime_info,
        initialize_business,
        login,
        register,
        get_auth_session,
        refresh_account,
        refresh_subscription
    ]);
    #[cfg(any(target_os = "android", target_os = "ios"))]
    let builder =
        builder.invoke_handler(tauri::generate_handler![get_plane_state, get_runtime_info]);
    builder
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

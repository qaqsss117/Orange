#![forbid(unsafe_code)]

use orange_domain::{
    CommandError, PlaneStateRequest, PlaneStateResponse, RuntimeInfoRequest, RuntimeInfoResponse,
};
use orange_platform::{DataPlaneEventHub, DiagnosticsHub, FileSettingsStore, SettingsStorage};
use std::sync::Arc;
use tauri::Manager;

#[cfg(not(any(target_os = "android", target_os = "ios")))]
use orange_domain::{
    AccountRefreshRequest, AccountResponse, AuthPublicResponse, AuthSessionRequest,
    AuthSessionResponse, BusinessInitializationResponse, DataPlaneControlRequest,
    DataPlaneControlResponse, DataPlaneEventSnapshotRequest, InitializeBusinessRequest,
    LoginCommandRequest, LogoutRequest, RegisterCommandRequest, SubscriptionPublicResponse,
    SubscriptionRefreshRequest, SubscriptionStatus,
};
#[cfg(target_os = "windows")]
use orange_platform::DataPlaneEventMonitor;
#[cfg(not(any(target_os = "android", target_os = "ios")))]
use orange_platform::{
    BusinessApiService, BusinessCommandClient, BusinessServiceError, DataPlaneEventHubSnapshot,
    DesktopSecretStore, SystemClock,
};

#[cfg(target_os = "android")]
mod android_secret_store;
#[cfg(target_os = "ios")]
use orange_ios_secret_store as ios_secret_store;
#[cfg(not(any(target_os = "android", target_os = "ios")))]
mod bootstrap_resource;
#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub mod control_plane;
mod planes;
#[cfg(target_os = "windows")]
pub mod windows_node_runtime;

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
fn get_data_plane_event_snapshot(
    request: DataPlaneEventSnapshotRequest,
    data_plane_events: tauri::State<'_, Arc<DataPlaneEventHub>>,
) -> Result<DataPlaneEventHubSnapshot, CommandError> {
    request.validate()?;
    Ok(data_plane_events.snapshot())
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tauri::command]
fn control_data_plane(
    request: DataPlaneControlRequest,
    planes: tauri::State<'_, planes::ManagedPlanes>,
    control: tauri::State<'_, planes::ManagedDataPlaneControl>,
) -> Result<DataPlaneControlResponse, CommandError> {
    let request = request.validate()?;
    control.execute(request.action, &planes)
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
    #[cfg(target_os = "windows")] business_client: tauri::State<
        '_,
        Arc<BusinessCommandClient<Arc<control_plane::ManagedControlPlane>, DesktopSecretStore>>,
    >,
    #[cfg(target_os = "windows")] subscription_runtime: tauri::State<
        '_,
        Arc<windows_node_runtime::WindowsSubscriptionRuntime>,
    >,
) -> Result<AuthPublicResponse, CommandError> {
    let request = request.validate()?;
    let response = service.login(request).map_err(map_business_error)?;
    #[cfg(target_os = "windows")]
    refresh_and_apply_subscription(&service, &business_client, &subscription_runtime)?;
    Ok(response)
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
fn logout(
    request: LogoutRequest,
    service: tauri::State<'_, DesktopBusinessService>,
    planes: tauri::State<'_, planes::ManagedPlanes>,
) -> Result<AuthSessionResponse, CommandError> {
    request.validate()?;
    service.logout(planes.inner()).map_err(map_business_error)
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
    #[cfg(target_os = "windows")] business_client: tauri::State<
        '_,
        Arc<BusinessCommandClient<Arc<control_plane::ManagedControlPlane>, DesktopSecretStore>>,
    >,
    #[cfg(target_os = "windows")] subscription_runtime: tauri::State<
        '_,
        Arc<windows_node_runtime::WindowsSubscriptionRuntime>,
    >,
) -> Result<SubscriptionPublicResponse, CommandError> {
    request.validate()?;
    #[cfg(target_os = "windows")]
    return refresh_and_apply_subscription(&service, &business_client, &subscription_runtime);
    #[cfg(not(target_os = "windows"))]
    service.refresh_subscription().map_err(map_business_error)
}

#[cfg(target_os = "windows")]
fn refresh_and_apply_subscription(
    service: &DesktopBusinessService,
    business_client: &BusinessCommandClient<
        Arc<control_plane::ManagedControlPlane>,
        DesktopSecretStore,
    >,
    subscription_runtime: &windows_node_runtime::WindowsSubscriptionRuntime,
) -> Result<SubscriptionPublicResponse, CommandError> {
    let response = service.refresh_subscription().map_err(map_business_error)?;
    if !matches!(
        response.status,
        SubscriptionStatus::Trial | SubscriptionStatus::Active
    ) {
        return Ok(response);
    }
    let payload = business_client
        .download_subscription()
        .map_err(|_| CommandError::from_code(orange_domain::ErrorCode::Subscription))?;
    subscription_runtime
        .apply_vless(payload)
        .map_err(|_| CommandError::from_code(orange_domain::ErrorCode::Subscription))?;
    Ok(response)
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
    #[cfg(target_os = "windows")]
    let windows_client = windows_node_runtime::discover_client();
    #[cfg(target_os = "windows")]
    let planes = windows_client
        .as_ref()
        .map_or_else(planes::ManagedPlanes::default, |client| {
            planes::ManagedPlanes::with_adapter(client.clone())
        });
    #[cfg(not(target_os = "windows"))]
    let planes = planes::ManagedPlanes::default();
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    let control_plane = Arc::new(control_plane::ManagedControlPlane::with_state(
        planes
            .control_handle()
            .expect("failed to initialize shared Control Plane state"),
    ));
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    if bootstrap_resource::start_embedded(&control_plane).is_err() {
        control_plane.mark_failed();
    }
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    let business_client = Arc::new(BusinessCommandClient::new(
        Arc::clone(&control_plane),
        DesktopSecretStore::new(),
    ));
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    let business_service = BusinessApiService::new(Arc::clone(&business_client), SystemClock);
    let diagnostics = Arc::new(DiagnosticsHub::default());
    let data_plane_events = Arc::new(DataPlaneEventHub::default());
    let builder = tauri::Builder::default()
        .manage(planes)
        .manage(Arc::clone(&diagnostics))
        .manage(Arc::clone(&data_plane_events));
    #[cfg(target_os = "android")]
    let builder = builder.plugin(android_secret_store::init());
    #[cfg(target_os = "ios")]
    let builder = builder.plugin(ios_secret_store::init());
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    let builder = builder
        .manage(control_plane)
        .manage(business_client)
        .manage(business_service);
    let builder = builder.setup(move |app| {
        let store = Arc::new(FileSettingsStore::new(app.path().app_data_dir()?)?);
        let _ = store.load()?;
        #[cfg(target_os = "windows")]
        let node_runtime = Arc::new(windows_node_runtime::WindowsNodeRuntimeHost::new(
            windows_client.clone(),
            Arc::clone(&store),
        ));
        #[cfg(target_os = "windows")]
        let _ = node_runtime.recover();
        #[cfg(target_os = "windows")]
        let subscription_runtime = Arc::new(windows_node_runtime::WindowsSubscriptionRuntime::new(
            windows_client,
            Arc::clone(&store),
            Arc::clone(&node_runtime),
        ));
        #[cfg(target_os = "windows")]
        let data_plane_event_monitor = node_runtime.is_provisioned().then(|| {
            DataPlaneEventMonitor::start(
                Arc::clone(&node_runtime),
                Arc::clone(&data_plane_events),
                Arc::clone(&diagnostics),
            )
        });
        #[cfg(target_os = "windows")]
        let data_plane_event_monitor = data_plane_event_monitor.transpose()?;
        #[cfg(target_os = "windows")]
        app.manage(planes::ManagedDataPlaneControl::with_source(Arc::clone(
            &node_runtime,
        )));
        #[cfg(target_os = "windows")]
        app.manage(node_runtime);
        #[cfg(target_os = "windows")]
        app.manage(subscription_runtime);
        #[cfg(target_os = "windows")]
        app.manage(data_plane_event_monitor);
        #[cfg(all(
            not(any(target_os = "android", target_os = "ios")),
            not(target_os = "windows")
        ))]
        app.manage(planes::ManagedDataPlaneControl::default());
        app.manage(store);
        Ok(())
    });
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    let builder = builder.invoke_handler(tauri::generate_handler![
        get_plane_state,
        get_runtime_info,
        get_data_plane_event_snapshot,
        control_data_plane,
        initialize_business,
        login,
        register,
        get_auth_session,
        logout,
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
    use orange_domain::{DOMAIN_SCHEMA_VERSION, DataPlaneControlAction, ErrorCode};

    use super::*;

    #[test]
    fn runtime_info_command_validates_the_request() {
        let response = get_runtime_info(RuntimeInfoRequest::current()).unwrap();
        assert_eq!(response.schema_version, DOMAIN_SCHEMA_VERSION);
        assert_eq!(response.product_name, "Orange");

        let error = get_runtime_info(RuntimeInfoRequest { schema_version: 1 }).unwrap_err();
        assert_eq!(error.code(), ErrorCode::Validation);
    }

    #[test]
    fn plane_state_request_validates_before_adapter_access() {
        let error = PlaneStateRequest { schema_version: 1 }
            .validate()
            .unwrap_err();
        assert_eq!(error.code(), ErrorCode::Validation);
    }

    #[test]
    fn event_snapshot_request_validates_before_hub_access() {
        let error = DataPlaneEventSnapshotRequest { schema_version: 1 }
            .validate()
            .unwrap_err();
        assert_eq!(error.code(), ErrorCode::Validation);
    }

    #[test]
    fn data_plane_control_request_validates_before_native_state_access() {
        let error = DataPlaneControlRequest {
            schema_version: 1,
            action: DataPlaneControlAction::Start,
        }
        .validate()
        .unwrap_err();
        assert_eq!(error.code(), ErrorCode::Validation);
    }
}

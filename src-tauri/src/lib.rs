#![forbid(unsafe_code)]

use orange_domain::{
    CommandError, PlaneStateRequest, PlaneStateResponse, RuntimeInfoRequest, RuntimeInfoResponse,
};
use orange_platform::{DataPlaneEventHub, DiagnosticsHub, FileSettingsStore, SettingsStorage};
use std::sync::Arc;
use tauri::Manager;

#[cfg(any(target_os = "windows", target_os = "macos"))]
use orange_domain::SubscriptionStatus;
#[cfg(not(any(target_os = "android", target_os = "ios")))]
use orange_domain::{
    AccountRefreshRequest, AccountResponse, ActiveSessionsRequest, ActiveSessionsResponse,
    AuthPublicResponse, AuthSessionRequest, AuthSessionResponse, BusinessInitializationResponse,
    CancelOrderCommandRequest, CancelOrderResponse, CheckoutOrderCommandRequest,
    CloseTicketCommandRequest, CommissionConfigRequest, CommissionConfigResponse,
    CommissionOperationResponse, ConnectionModeRequest, ConnectionModeResponse,
    CreateOrderCommandRequest, CreateOrderResponse, CreateTicketCommandRequest,
    DataPlaneControlRequest, DataPlaneControlResponse, DataPlaneEventSnapshotRequest,
    EmailVerificationResponse, ErrorCode, GiftCardCheckResponse, GiftCardCodeCommandRequest,
    GiftCardHistoryRequest, GiftCardHistoryResponse, GiftCardRedeemResponse,
    InitializeBusinessRequest, InvitationCenterRequest, InvitationCenterResponse,
    KnowledgeDetailCommandRequest, KnowledgeDetailResponse, KnowledgeListCommandRequest,
    KnowledgeListResponse, LaunchOnStartupRequest, LaunchOnStartupResponse, LegalDocument,
    LoginCommandRequest, LogoutRequest, NetworkTool, NoticesRequest, NoticesResponse,
    OpenLegalDocumentRequest, OpenLegalDocumentResponse, OpenNetworkToolRequest,
    OpenNetworkToolResponse, OpenServicePortalRequest, OpenServicePortalResponse,
    OrderDetailCommandRequest, OrderDetailResponse, OrdersRequest, OrdersResponse,
    PasswordResetResponse, PaymentMethodsRequest, PaymentMethodsResponse, PaymentPublicResponse,
    PlansRequest, PlansResponse, ProxyPortRequest, ProxyPortResponse, RegisterCommandRequest,
    RemoveActiveSessionCommandRequest, ReplyTicketCommandRequest, ResetPasswordCommandRequest,
    RoutingModeRequest, RoutingModeResponse, SendEmailVerificationCommandRequest,
    ServicePortalUrlResponse, SetConnectionModeRequest, SetLaunchOnStartupRequest,
    SetProxyPortRequest, SetRoutingModeRequest, SubscriptionPublicResponse,
    SubscriptionRefreshRequest, TicketDetailCommandRequest, TicketDetailResponse, TicketsRequest,
    TicketsResponse, TransferCommissionCommandRequest, WithdrawCommissionCommandRequest,
};
#[cfg(not(any(target_os = "android", target_os = "ios")))]
use orange_domain::{AuthSessionStatus, DataPlaneControlAction, DataPlaneState};
#[cfg(not(any(target_os = "android", target_os = "ios")))]
use orange_domain::{MacosPackageUpdateRequest, MacosPackageUpdateResponse};
#[cfg(not(any(target_os = "android", target_os = "ios")))]
use orange_domain::{
    NodeCatalogRequest, NodeCatalogResponse, NodeDelayTestRequest, NodeDelayTestResponse,
    NodeSelectionModeResponse, SelectNodeRequest, SelectNodeResponse, SetNodeSelectionModeRequest,
    SubscriptionSnapshotRequest, SubscriptionSnapshotResponse,
};
#[cfg(any(target_os = "windows", target_os = "macos"))]
use orange_platform::{
    BootstrapTransportError, BusinessClientError, DataPlaneEventMonitor, RoutingRuleResources,
    RuleResourceStore,
};
#[cfg(not(any(target_os = "android", target_os = "ios")))]
use orange_platform::{
    BusinessApiService, BusinessCommandClient, BusinessServiceError, DataPlaneEventHubSnapshot,
    DesktopSecretStore, SystemClock,
};
#[cfg(not(any(target_os = "android", target_os = "ios")))]
use tauri_plugin_autostart::ManagerExt as _;

#[cfg(target_os = "android")]
mod android_secret_store;
#[cfg(target_os = "ios")]
use orange_ios_secret_store as ios_secret_store;
#[cfg(not(any(target_os = "android", target_os = "ios")))]
mod bootstrap_resource;
#[cfg(not(any(target_os = "android", target_os = "ios")))]
mod connection_preferences;
#[cfg(not(any(target_os = "android", target_os = "ios")))]
mod connection_recovery;
#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub mod control_plane;
#[cfg(not(any(target_os = "android", target_os = "ios")))]
mod desktop_node_runtime;
#[cfg(target_os = "macos")]
pub mod macos_node_runtime;
#[cfg(any(target_os = "macos", test))]
mod macos_selection_runtime;
#[cfg(target_os = "macos")]
mod macos_update;
mod planes;
#[cfg(target_os = "windows")]
pub mod windows_node_runtime;
#[cfg(target_os = "windows")]
mod windows_proxy_runtime;
#[cfg(target_os = "windows")]
mod windows_tray;

#[cfg(not(any(target_os = "android", target_os = "ios")))]
type DesktopBusinessService =
    Arc<BusinessApiService<Arc<control_plane::ManagedControlPlane>, DesktopSecretStore>>;

#[cfg(not(any(target_os = "android", target_os = "ios")))]
struct EligibleRevisionSource {
    node_runtime: Arc<dyn orange_platform::NodeRuntimeHost>,
    business_service: DesktopBusinessService,
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
impl planes::ActiveConfigurationRevision for EligibleRevisionSource {
    fn active_configuration_revision(
        &self,
    ) -> Result<Option<orange_platform::ConfigurationRevision>, orange_platform::PlatformVpnError>
    {
        if !self
            .business_service
            .subscription_allows_new_data_plane_start()
        {
            return Ok(None);
        }
        planes::ActiveConfigurationRevision::active_configuration_revision(&self.node_runtime)
    }
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
struct DesktopConnectionModeRuntime {
    business_client:
        Arc<BusinessCommandClient<Arc<control_plane::ManagedControlPlane>, DesktopSecretStore>>,
    subscription_runtime: Arc<dyn desktop_node_runtime::DesktopSubscriptionApplier>,
    #[cfg(target_os = "macos")]
    node_runtime: Arc<dyn orange_platform::NodeRuntimeHost>,
    #[cfg(target_os = "windows")]
    proxy_runtime: Arc<windows_proxy_runtime::WindowsProxyRuntime>,
}

#[tauri::command]
fn get_runtime_info(request: RuntimeInfoRequest) -> Result<RuntimeInfoResponse, CommandError> {
    request.validate()?;
    Ok(RuntimeInfoResponse::new(env!("CARGO_PKG_VERSION")))
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tauri::command]
async fn check_macos_package_update(
    request: MacosPackageUpdateRequest,
    app: tauri::AppHandle,
) -> Result<MacosPackageUpdateResponse, CommandError> {
    request.validate()?;
    #[cfg(target_os = "macos")]
    return macos_update::check(&app).await;
    #[cfg(not(target_os = "macos"))]
    {
        let _ = app;
        Ok(MacosPackageUpdateResponse::unsupported())
    }
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tauri::command]
async fn prepare_macos_package_update(
    request: MacosPackageUpdateRequest,
    app: tauri::AppHandle,
) -> Result<MacosPackageUpdateResponse, CommandError> {
    request.validate()?;
    #[cfg(target_os = "macos")]
    return macos_update::prepare(&app).await;
    #[cfg(not(target_os = "macos"))]
    {
        let _ = app;
        Ok(MacosPackageUpdateResponse::unsupported())
    }
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
    app: tauri::AppHandle,
) -> Result<DataPlaneControlResponse, CommandError> {
    let request = request.validate()?;
    #[cfg(target_os = "windows")]
    {
        let proxy_runtime = app.state::<Arc<windows_proxy_runtime::WindowsProxyRuntime>>();
        let recovery = app.state::<connection_recovery::ConnectionRecovery>();
        let node_runtime = app.state::<Arc<dyn orange_platform::NodeRuntimeHost>>();
        execute_windows_data_plane_action(
            request.action,
            &planes,
            &control,
            &proxy_runtime,
            &recovery,
            &**node_runtime,
        )
    }
    #[cfg(target_os = "macos")]
    {
        let recovery = app.state::<connection_recovery::ConnectionRecovery>();
        let node_runtime = app.state::<Arc<dyn orange_platform::NodeRuntimeHost>>();
        execute_desktop_data_plane_action(
            request.action,
            &planes,
            &control,
            &recovery,
            &**node_runtime,
        )
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        let _ = app;
        control.execute(request.action, &planes)
    }
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
fn execute_desktop_data_plane_action(
    action: DataPlaneControlAction,
    planes: &planes::ManagedPlanes,
    control: &planes::ManagedDataPlaneControl,
    recovery: &connection_recovery::ConnectionRecovery,
    node_runtime: &dyn orange_platform::NodeRuntimeHost,
) -> Result<DataPlaneControlResponse, CommandError> {
    if action == DataPlaneControlAction::Start {
        node_runtime
            .prepare_auto_selection()
            .map_err(map_node_runtime_error)?;
    }
    let response = control.execute(action, planes)?;
    match action {
        DataPlaneControlAction::Start => {
            let _ = recovery.mark_connected();
        }
        DataPlaneControlAction::Stop => {
            #[cfg(target_os = "macos")]
            macos_node_runtime::clear_connection_recovery()
                .map_err(|_| CommandError::from_code(ErrorCode::Service))?;
            let _ = recovery.clear();
        }
        DataPlaneControlAction::Status => {}
    }
    Ok(response)
}

#[cfg(target_os = "windows")]
fn execute_windows_data_plane_action(
    action: DataPlaneControlAction,
    planes: &planes::ManagedPlanes,
    control: &planes::ManagedDataPlaneControl,
    proxy_runtime: &windows_proxy_runtime::WindowsProxyRuntime,
    recovery: &connection_recovery::ConnectionRecovery,
    node_runtime: &dyn orange_platform::NodeRuntimeHost,
) -> Result<DataPlaneControlResponse, CommandError> {
    let proxy_operation =
        (action != DataPlaneControlAction::Status).then(|| proxy_runtime.begin_operation());
    if action == DataPlaneControlAction::Stop {
        proxy_runtime
            .restore_before_stop()
            .map_err(|_| CommandError::from_code(ErrorCode::Service))?;
    }
    let response =
        execute_desktop_data_plane_action(action, planes, control, recovery, node_runtime)?;
    // The marker records connection *intent*: a successful Start returns with the
    // data plane still in `Starting` (it transitions to `Online` asynchronously),
    // so it must be written here rather than gated on `Online`. Persistence
    // failures are best-effort — they must not fail an otherwise successful
    // connect/disconnect.
    if proxy_runtime.reconcile_now().is_err() {
        proxy_runtime.fail_closed();
        drop(proxy_operation);
        return Err(CommandError::from_code(ErrorCode::Service));
    }
    Ok(response)
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tauri::command]
fn get_connection_mode(
    request: ConnectionModeRequest,
    preferences: tauri::State<'_, Arc<connection_preferences::ConnectionPreferences>>,
) -> Result<ConnectionModeResponse, CommandError> {
    request.validate()?;
    Ok(ConnectionModeResponse::new(preferences.mode()))
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tauri::command]
fn set_connection_mode(
    request: SetConnectionModeRequest,
    preferences: tauri::State<'_, Arc<connection_preferences::ConnectionPreferences>>,
    app: tauri::AppHandle,
) -> Result<ConnectionModeResponse, CommandError> {
    let request = request.validate()?;
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    {
        let planes = app.state::<planes::ManagedPlanes>();
        let control = app.state::<planes::ManagedDataPlaneControl>();
        let service = app.state::<DesktopBusinessService>();
        let runtime = app.state::<DesktopConnectionModeRuntime>();
        switch_desktop_connection_mode(
            request.mode,
            &preferences,
            &planes,
            &control,
            &service,
            &runtime,
        )
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        let _ = app;
        if preferences.mode() == request.mode {
            return Ok(ConnectionModeResponse::new(request.mode));
        }
        preferences
            .set_mode(request.mode)
            .map_err(|_| CommandError::from_code(ErrorCode::Internal))?;
        Ok(ConnectionModeResponse::new(request.mode))
    }
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tauri::command]
fn get_routing_mode(
    request: RoutingModeRequest,
    preferences: tauri::State<'_, Arc<connection_preferences::ConnectionPreferences>>,
) -> Result<RoutingModeResponse, CommandError> {
    request.validate()?;
    Ok(RoutingModeResponse::new(preferences.routing_mode()))
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tauri::command]
fn set_routing_mode(
    request: SetRoutingModeRequest,
    preferences: tauri::State<'_, Arc<connection_preferences::ConnectionPreferences>>,
    app: tauri::AppHandle,
) -> Result<RoutingModeResponse, CommandError> {
    let request = request.validate()?;
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    {
        let planes = app.state::<planes::ManagedPlanes>();
        let control = app.state::<planes::ManagedDataPlaneControl>();
        let service = app.state::<DesktopBusinessService>();
        let runtime = app.state::<DesktopConnectionModeRuntime>();
        switch_desktop_routing_mode(
            request.mode,
            &preferences,
            &planes,
            &control,
            &service,
            &runtime,
        )
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        let _ = app;
        if preferences.routing_mode() == request.mode {
            return Ok(RoutingModeResponse::new(request.mode));
        }
        preferences
            .set_routing_mode(request.mode)
            .map_err(|_| CommandError::from_code(ErrorCode::Internal))?;
        Ok(RoutingModeResponse::new(request.mode))
    }
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tauri::command]
fn get_proxy_port(
    request: ProxyPortRequest,
    preferences: tauri::State<'_, Arc<connection_preferences::ConnectionPreferences>>,
) -> Result<ProxyPortResponse, CommandError> {
    request.validate()?;
    Ok(ProxyPortResponse::new(preferences.proxy_port()))
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tauri::command]
fn set_proxy_port(
    request: SetProxyPortRequest,
    preferences: tauri::State<'_, Arc<connection_preferences::ConnectionPreferences>>,
    app: tauri::AppHandle,
) -> Result<ProxyPortResponse, CommandError> {
    let port = request.validate()?;
    let _reconfiguration = preferences
        .begin_reconfiguration()
        .map_err(|()| CommandError::from_code(ErrorCode::Cancelled))?;
    let previous = preferences.proxy_port();
    if previous == port {
        return Ok(ProxyPortResponse::new(port));
    }
    if preferences.mode() != orange_domain::ConnectionMode::SystemProxy {
        return Err(CommandError::from_code(ErrorCode::Validation));
    }
    ensure_proxy_port_available(port)?;
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    {
        let planes = app.state::<planes::ManagedPlanes>();
        let control = app.state::<planes::ManagedDataPlaneControl>();
        let runtime = app.state::<DesktopConnectionModeRuntime>();
        reconfigure_desktop_proxy_port(
            port,
            previous,
            &preferences,
            &planes,
            &control,
            &runtime,
        )?;
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        let _ = app;
        preferences
            .set_proxy_port(port)
            .map_err(|_| CommandError::from_code(ErrorCode::Internal))?;
    }
    Ok(ProxyPortResponse::new(port))
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn ensure_proxy_port_available(port: u16) -> Result<(), CommandError> {
    use std::net::{Ipv4Addr, TcpListener, UdpSocket};

    let address = (Ipv4Addr::LOCALHOST, port);
    let tcp = TcpListener::bind(address)
        .map_err(|_| CommandError::from_code(ErrorCode::ProxyPortInUse))?;
    let udp =
        UdpSocket::bind(address).map_err(|_| CommandError::from_code(ErrorCode::ProxyPortInUse))?;
    drop((tcp, udp));
    Ok(())
}

#[cfg(test)]
mod proxy_port_tests {
    use super::*;

    #[test]
    fn occupied_tcp_port_is_rejected() {
        let listener = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        assert_eq!(
            ensure_proxy_port_available(port).unwrap_err().code(),
            ErrorCode::ProxyPortInUse
        );
    }

    #[test]
    fn occupied_udp_port_is_rejected() {
        let socket = std::net::UdpSocket::bind((std::net::Ipv4Addr::LOCALHOST, 0)).unwrap();
        let port = socket.local_addr().unwrap().port();
        assert_eq!(
            ensure_proxy_port_available(port).unwrap_err().code(),
            ErrorCode::ProxyPortInUse
        );
    }

    #[test]
    fn unused_loopback_port_is_available() {
        assert!(
            (orange_domain::MIN_PROXY_PORT..=orange_domain::MAX_PROXY_PORT)
                .filter(|port| *port != orange_domain::RESERVED_PROXY_PROBE_PORT)
                .any(|port| ensure_proxy_port_available(port).is_ok()),
            "find a loopback port available to TCP and UDP"
        );
    }
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tauri::command]
fn get_launch_on_startup(
    request: LaunchOnStartupRequest,
    app: tauri::AppHandle,
) -> Result<LaunchOnStartupResponse, CommandError> {
    request.validate()?;
    let enabled = app
        .autolaunch()
        .is_enabled()
        .map_err(|_| CommandError::from_code(ErrorCode::Service))?;
    Ok(LaunchOnStartupResponse::new(enabled))
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tauri::command]
fn set_launch_on_startup(
    request: SetLaunchOnStartupRequest,
    app: tauri::AppHandle,
    store: tauri::State<'_, Arc<FileSettingsStore>>,
) -> Result<LaunchOnStartupResponse, CommandError> {
    let enabled = request.validate()?.enabled;
    let previous = app
        .autolaunch()
        .is_enabled()
        .map_err(|_| CommandError::from_code(ErrorCode::Service))?;
    if previous != enabled {
        apply_launch_on_startup(&app, enabled)?;
    }

    let persisted = (|| {
        let mut settings = store.load()?.into_settings();
        if settings.launch_on_startup() != enabled {
            settings.set_launch_on_startup(enabled);
            store.save(&settings)?;
        }
        Ok::<(), orange_platform::PersistenceError>(())
    })();
    if persisted.is_err() {
        if previous != enabled && apply_launch_on_startup(&app, previous).is_err() {
            return Err(CommandError::from_code(ErrorCode::Service));
        }
        return Err(CommandError::from_code(ErrorCode::Internal));
    }
    Ok(LaunchOnStartupResponse::new(enabled))
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn apply_launch_on_startup(app: &tauri::AppHandle, enabled: bool) -> Result<(), CommandError> {
    let manager = app.autolaunch();
    if enabled {
        manager.enable()
    } else {
        manager.disable()
    }
    .map_err(|_| CommandError::from_code(ErrorCode::Service))
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tauri::command]
fn initialize_business(
    request: InitializeBusinessRequest,
    service: tauri::State<'_, DesktopBusinessService>,
    app: tauri::AppHandle,
) -> Result<BusinessInitializationResponse, CommandError> {
    request.validate()?;
    let response = service.initialize().map_err(map_business_error)?;
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    if response.session.status == AuthSessionStatus::Authenticated {
        let business_client =
            app.state::<Arc<
                BusinessCommandClient<Arc<control_plane::ManagedControlPlane>, DesktopSecretStore>,
            >>();
        let runtime = app.state::<DesktopConnectionModeRuntime>();
        let connection_preferences =
            app.state::<Arc<connection_preferences::ConnectionPreferences>>();
        let node_runtime = app.state::<Arc<dyn orange_platform::NodeRuntimeHost>>();
        let subscription_result = refresh_and_apply_subscription(
            &service,
            &business_client,
            runtime.subscription_runtime.as_ref(),
            &connection_preferences,
            #[cfg(target_os = "windows")]
            runtime.proxy_runtime.as_ref(),
            &node_runtime,
        );
        let has_local_revision = if subscription_result.is_err() {
            orange_platform::NodeRuntimeHost::active_revision(&**node_runtime)
                .map_err(map_node_runtime_error)?
                .is_some()
        } else {
            false
        };
        accept_startup_subscription(subscription_result, has_local_revision)?;
        resume_desktop_connection_if_needed(&app);
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let _ = app;
    Ok(response)
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
fn resume_desktop_connection_if_needed(app: &tauri::AppHandle) {
    let recovery = app.state::<connection_recovery::ConnectionRecovery>();
    if !recovery.should_reconnect() {
        return;
    }
    let planes = app.state::<planes::ManagedPlanes>();
    let control = app.state::<planes::ManagedDataPlaneControl>();
    let Ok(status) = control.execute(DataPlaneControlAction::Status, &planes) else {
        return;
    };
    if status.data_plane == DataPlaneState::Online || !status.can_start {
        return;
    }
    let node_runtime = app.state::<Arc<dyn orange_platform::NodeRuntimeHost>>();
    #[cfg(target_os = "windows")]
    {
        let proxy_runtime = app.state::<Arc<windows_proxy_runtime::WindowsProxyRuntime>>();
        let _ = execute_windows_data_plane_action(
            DataPlaneControlAction::Start,
            &planes,
            &control,
            &proxy_runtime,
            &recovery,
            &**node_runtime,
        );
    }
    #[cfg(target_os = "macos")]
    let _ = execute_desktop_data_plane_action(
        DataPlaneControlAction::Start,
        &planes,
        &control,
        &recovery,
        &**node_runtime,
    );
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tauri::command]
fn open_service_portal(
    request: OpenServicePortalRequest,
    service: tauri::State<'_, DesktopBusinessService>,
) -> Result<OpenServicePortalResponse, CommandError> {
    request.validate()?;
    let url = service.service_portal_url().map_err(map_business_error)?;
    tauri_plugin_opener::open_url(&url, None::<&str>)
        .map_err(|_| CommandError::from_code(ErrorCode::Service))?;
    Ok(OpenServicePortalResponse::opened())
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tauri::command]
fn get_service_portal_url(
    request: OpenServicePortalRequest,
    service: tauri::State<'_, DesktopBusinessService>,
) -> Result<ServicePortalUrlResponse, CommandError> {
    request.validate()?;
    let url = service.service_portal_url().map_err(map_business_error)?;
    Ok(ServicePortalUrlResponse::new(url))
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tauri::command]
fn open_telegram_bot(
    request: OpenServicePortalRequest,
    service: tauri::State<'_, DesktopBusinessService>,
) -> Result<OpenServicePortalResponse, CommandError> {
    request.validate()?;
    let username = service
        .telegram_bot_username()
        .map_err(map_business_error)?
        .ok_or_else(|| CommandError::from_code(ErrorCode::Service))?;
    let url = format!("https://t.me/{username}");
    tauri_plugin_opener::open_url(&url, None::<&str>)
        .map_err(|_| CommandError::from_code(ErrorCode::Service))?;
    Ok(OpenServicePortalResponse::opened())
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tauri::command]
fn open_network_tool(
    request: OpenNetworkToolRequest,
) -> Result<OpenNetworkToolResponse, CommandError> {
    let request = request.validate()?;
    let url = match request.tool {
        NetworkTool::IpLookup => "https://ipcelou.com",
        NetworkTool::SpeedTest => "https://fast.com",
    };
    tauri_plugin_opener::open_url(url, None::<&str>)
        .map_err(|_| CommandError::from_code(ErrorCode::Service))?;
    Ok(OpenNetworkToolResponse::opened(request.tool))
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tauri::command]
fn open_legal_document(
    request: OpenLegalDocumentRequest,
) -> Result<OpenLegalDocumentResponse, CommandError> {
    let request = request.validate()?;
    let url = match request.document {
        LegalDocument::TermsOfService => "https://minipanda.soccertt.com/teams.html",
        LegalDocument::PrivacyPolicy => "https://minipanda.soccertt.com/privacy.html",
    };
    tauri_plugin_opener::open_url(url, None::<&str>)
        .map_err(|_| CommandError::from_code(ErrorCode::Service))?;
    Ok(OpenLegalDocumentResponse::opened(request.document))
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
fn accept_startup_subscription(
    result: Result<SubscriptionPublicResponse, CommandError>,
    has_local_revision: bool,
) -> Result<(), CommandError> {
    match result {
        Ok(_) => Ok(()),
        Err(_) if has_local_revision => Ok(()),
        Err(error) => Err(error),
    }
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tauri::command]
fn login(
    request: LoginCommandRequest,
    service: tauri::State<'_, DesktopBusinessService>,
    app: tauri::AppHandle,
) -> Result<AuthPublicResponse, CommandError> {
    let request = request.validate()?;
    let response = service.login(request).map_err(map_business_error)?;
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    {
        let business_client =
            app.state::<Arc<
                BusinessCommandClient<Arc<control_plane::ManagedControlPlane>, DesktopSecretStore>,
            >>();
        let runtime = app.state::<DesktopConnectionModeRuntime>();
        let connection_preferences =
            app.state::<Arc<connection_preferences::ConnectionPreferences>>();
        let node_runtime = app.state::<Arc<dyn orange_platform::NodeRuntimeHost>>();
        refresh_and_apply_subscription(
            &service,
            &business_client,
            runtime.subscription_runtime.as_ref(),
            &connection_preferences,
            #[cfg(target_os = "windows")]
            runtime.proxy_runtime.as_ref(),
            &node_runtime,
        )?;
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let _ = app;
    Ok(response)
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tauri::command]
fn send_email_verification(
    request: SendEmailVerificationCommandRequest,
    service: tauri::State<'_, DesktopBusinessService>,
) -> Result<EmailVerificationResponse, CommandError> {
    let request = request.validate()?;
    service
        .send_email_verification(request)
        .map_err(map_business_error)
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tauri::command]
fn reset_password(
    request: ResetPasswordCommandRequest,
    service: tauri::State<'_, DesktopBusinessService>,
) -> Result<PasswordResetResponse, CommandError> {
    let request = request.validate()?;
    service.reset_password(request).map_err(map_business_error)
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
    app: tauri::AppHandle,
) -> Result<AuthSessionResponse, CommandError> {
    request.validate()?;
    #[cfg(target_os = "windows")]
    let proxy_runtime = app.state::<Arc<windows_proxy_runtime::WindowsProxyRuntime>>();
    #[cfg(target_os = "windows")]
    let _proxy_operation = proxy_runtime.begin_operation();
    #[cfg(target_os = "windows")]
    proxy_runtime
        .restore_before_stop()
        .map_err(|_| CommandError::from_code(ErrorCode::Service))?;
    #[cfg(target_os = "macos")]
    macos_node_runtime::clear_connection_recovery()
        .map_err(|_| CommandError::from_code(ErrorCode::Service))?;
    #[cfg(not(target_os = "windows"))]
    let _ = app;
    let response = service.logout(planes.inner()).map_err(map_business_error)?;
    // Best-effort: the session is already logged out server-side, so a marker
    // cleanup failure must not surface as a logout failure.
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    let _ = app
        .state::<connection_recovery::ConnectionRecovery>()
        .clear();
    Ok(response)
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
async fn fetch_notices(
    request: NoticesRequest,
    service: tauri::State<'_, DesktopBusinessService>,
) -> Result<NoticesResponse, CommandError> {
    request.validate()?;
    let service = Arc::clone(service.inner());
    run_blocking_command(move || service.fetch_notices().map_err(map_business_error)).await
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tauri::command]
async fn fetch_plans(
    request: PlansRequest,
    service: tauri::State<'_, DesktopBusinessService>,
) -> Result<PlansResponse, CommandError> {
    request.validate()?;
    let service = Arc::clone(service.inner());
    run_blocking_command(move || service.fetch_plans().map_err(map_business_error)).await
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tauri::command]
fn fetch_orders(
    request: OrdersRequest,
    service: tauri::State<'_, DesktopBusinessService>,
) -> Result<OrdersResponse, CommandError> {
    request.validate()?;
    service.fetch_orders().map_err(map_business_error)
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tauri::command]
fn fetch_order_detail(
    request: OrderDetailCommandRequest,
    service: tauri::State<'_, DesktopBusinessService>,
) -> Result<OrderDetailResponse, CommandError> {
    let order_id = request.validate()?;
    service
        .fetch_order_detail(&order_id)
        .map_err(map_business_error)
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tauri::command]
fn fetch_payment_methods(
    request: PaymentMethodsRequest,
    service: tauri::State<'_, DesktopBusinessService>,
) -> Result<PaymentMethodsResponse, CommandError> {
    request.validate()?;
    service.fetch_payment_methods().map_err(map_business_error)
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tauri::command]
fn checkout_order(
    request: CheckoutOrderCommandRequest,
    service: tauri::State<'_, DesktopBusinessService>,
) -> Result<PaymentPublicResponse, CommandError> {
    let request = request.validate()?;
    service
        .checkout_order(request)
        .map_err(map_business_error)
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tauri::command]
fn cancel_order(
    request: CancelOrderCommandRequest,
    service: tauri::State<'_, DesktopBusinessService>,
) -> Result<CancelOrderResponse, CommandError> {
    let order_id = request.validate()?;
    service.cancel_order(&order_id).map_err(map_business_error)
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tauri::command]
async fn create_order(
    request: CreateOrderCommandRequest,
    service: tauri::State<'_, DesktopBusinessService>,
) -> Result<CreateOrderResponse, CommandError> {
    let request = request.validate()?;
    let service = Arc::clone(service.inner());
    run_blocking_command(move || service.create_order(request).map_err(map_business_error)).await
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tauri::command]
fn fetch_invitation_center(
    request: InvitationCenterRequest,
    service: tauri::State<'_, DesktopBusinessService>,
) -> Result<InvitationCenterResponse, CommandError> {
    request.validate()?;
    service
        .fetch_invitation_center()
        .map_err(map_business_error)
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tauri::command]
fn generate_invitation_code(
    request: InvitationCenterRequest,
    service: tauri::State<'_, DesktopBusinessService>,
) -> Result<InvitationCenterResponse, CommandError> {
    request.validate()?;
    service
        .generate_invitation_code()
        .map_err(map_business_error)
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tauri::command]
async fn check_gift_card(
    request: GiftCardCodeCommandRequest,
    service: tauri::State<'_, DesktopBusinessService>,
) -> Result<GiftCardCheckResponse, CommandError> {
    let code = request.validate()?;
    let service = Arc::clone(service.inner());
    run_blocking_command(move || service.check_gift_card(&code).map_err(map_business_error)).await
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tauri::command]
async fn redeem_gift_card(
    request: GiftCardCodeCommandRequest,
    service: tauri::State<'_, DesktopBusinessService>,
) -> Result<GiftCardRedeemResponse, CommandError> {
    let code = request.validate()?;
    let service = Arc::clone(service.inner());
    run_blocking_command(move || service.redeem_gift_card(&code).map_err(map_business_error)).await
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tauri::command]
async fn fetch_gift_card_history(
    request: GiftCardHistoryRequest,
    service: tauri::State<'_, DesktopBusinessService>,
) -> Result<GiftCardHistoryResponse, CommandError> {
    request.validate()?;
    let service = Arc::clone(service.inner());
    run_blocking_command(move || {
        service
            .fetch_gift_card_history()
            .map_err(map_business_error)
    })
    .await
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tauri::command]
fn fetch_commission_config(
    request: CommissionConfigRequest,
    service: tauri::State<'_, DesktopBusinessService>,
) -> Result<CommissionConfigResponse, CommandError> {
    request.validate()?;
    service
        .fetch_commission_config()
        .map_err(map_business_error)
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tauri::command]
fn withdraw_commission(
    request: WithdrawCommissionCommandRequest,
    service: tauri::State<'_, DesktopBusinessService>,
) -> Result<CommissionOperationResponse, CommandError> {
    let (method, account) = request.validate()?;
    service
        .withdraw_commission(&method, &account)
        .map_err(map_business_error)
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tauri::command]
fn transfer_commission(
    request: TransferCommissionCommandRequest,
    service: tauri::State<'_, DesktopBusinessService>,
) -> Result<CommissionOperationResponse, CommandError> {
    let amount_minor = request.validate()?;
    service
        .transfer_commission(amount_minor)
        .map_err(map_business_error)
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tauri::command]
fn fetch_active_sessions(
    request: ActiveSessionsRequest,
    service: tauri::State<'_, DesktopBusinessService>,
) -> Result<ActiveSessionsResponse, CommandError> {
    request.validate()?;
    service.fetch_active_sessions().map_err(map_business_error)
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tauri::command]
fn remove_active_session(
    request: RemoveActiveSessionCommandRequest,
    service: tauri::State<'_, DesktopBusinessService>,
) -> Result<CommissionOperationResponse, CommandError> {
    let session_id = request.validate()?;
    service
        .remove_active_session(&session_id)
        .map_err(map_business_error)
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tauri::command]
fn fetch_knowledge_list(
    request: KnowledgeListCommandRequest,
    service: tauri::State<'_, DesktopBusinessService>,
) -> Result<KnowledgeListResponse, CommandError> {
    let keyword = request.validate()?;
    service
        .fetch_knowledge_list(keyword.as_deref())
        .map_err(map_business_error)
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tauri::command]
fn fetch_knowledge_detail(
    request: KnowledgeDetailCommandRequest,
    service: tauri::State<'_, DesktopBusinessService>,
) -> Result<KnowledgeDetailResponse, CommandError> {
    let article_id = request.validate()?;
    service
        .fetch_knowledge_detail(&article_id)
        .map_err(map_business_error)
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tauri::command]
fn fetch_tickets(
    request: TicketsRequest,
    service: tauri::State<'_, DesktopBusinessService>,
) -> Result<TicketsResponse, CommandError> {
    request.validate()?;
    service.fetch_tickets().map_err(map_business_error)
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tauri::command]
fn fetch_ticket_detail(
    request: TicketDetailCommandRequest,
    service: tauri::State<'_, DesktopBusinessService>,
) -> Result<TicketDetailResponse, CommandError> {
    let ticket_id = request.validate()?;
    service
        .fetch_ticket_detail(&ticket_id)
        .map_err(map_business_error)
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tauri::command]
fn create_ticket(
    request: CreateTicketCommandRequest,
    service: tauri::State<'_, DesktopBusinessService>,
) -> Result<TicketsResponse, CommandError> {
    let request = request.validate()?;
    service.create_ticket(request).map_err(map_business_error)
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tauri::command]
fn reply_ticket(
    request: ReplyTicketCommandRequest,
    service: tauri::State<'_, DesktopBusinessService>,
) -> Result<TicketDetailResponse, CommandError> {
    let request = request.validate()?;
    service.reply_ticket(request).map_err(map_business_error)
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tauri::command]
fn close_ticket(
    request: CloseTicketCommandRequest,
    service: tauri::State<'_, DesktopBusinessService>,
) -> Result<TicketDetailResponse, CommandError> {
    let ticket_id = request.validate()?;
    service.close_ticket(&ticket_id).map_err(map_business_error)
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tauri::command]
async fn refresh_subscription(
    request: SubscriptionRefreshRequest,
    service: tauri::State<'_, DesktopBusinessService>,
    app: tauri::AppHandle,
) -> Result<SubscriptionPublicResponse, CommandError> {
    request.validate()?;
    let service = Arc::clone(service.inner());
    run_blocking_command(move || {
        #[cfg(any(target_os = "windows", target_os = "macos"))]
        {
            let business_client = app.state::<Arc<
                BusinessCommandClient<Arc<control_plane::ManagedControlPlane>, DesktopSecretStore>,
            >>();
            let runtime = app.state::<DesktopConnectionModeRuntime>();
            let connection_preferences =
                app.state::<Arc<connection_preferences::ConnectionPreferences>>();
            let node_runtime = app.state::<Arc<dyn orange_platform::NodeRuntimeHost>>();
            refresh_and_apply_subscription(
                &service,
                &business_client,
                runtime.subscription_runtime.as_ref(),
                &connection_preferences,
                #[cfg(target_os = "windows")]
                runtime.proxy_runtime.as_ref(),
                &node_runtime,
            )
        }
        #[cfg(not(any(target_os = "windows", target_os = "macos")))]
        {
            let _ = app;
            service.refresh_subscription().map_err(map_business_error)
        }
    })
    .await
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tauri::command]
async fn get_subscription_snapshot(
    request: SubscriptionSnapshotRequest,
    service: tauri::State<'_, DesktopBusinessService>,
    node_runtime: tauri::State<'_, Arc<dyn orange_platform::NodeRuntimeHost>>,
) -> Result<SubscriptionSnapshotResponse, CommandError> {
    request.validate()?;
    require_authenticated(&service)?;
    let service = Arc::clone(service.inner());
    let node_runtime = Arc::clone(node_runtime.inner());
    run_blocking_command(move || {
        let local_revision =
            orange_platform::NodeRuntimeHost::active_revision(node_runtime.as_ref())
                .map_err(map_node_runtime_error)?
                .map(|revision| revision.get());
        Ok(SubscriptionSnapshotResponse::new(
            service.cached_subscription(),
            local_revision,
        ))
    })
    .await
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tauri::command]
async fn get_node_catalog(
    request: NodeCatalogRequest,
    service: tauri::State<'_, DesktopBusinessService>,
    node_runtime: tauri::State<'_, Arc<dyn orange_platform::NodeRuntimeHost>>,
) -> Result<NodeCatalogResponse, CommandError> {
    request.validate()?;
    require_authenticated(&service)?;
    let node_runtime = Arc::clone(node_runtime.inner());
    run_blocking_command(move || {
        node_runtime
            .catalog_snapshot()
            .map_err(map_node_runtime_error)
    })
    .await
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tauri::command]
async fn select_node(
    request: SelectNodeRequest,
    service: tauri::State<'_, DesktopBusinessService>,
    node_runtime: tauri::State<'_, Arc<dyn orange_platform::NodeRuntimeHost>>,
) -> Result<SelectNodeResponse, CommandError> {
    let request = request.validate()?;
    require_authenticated(&service)?;
    let node_runtime = Arc::clone(node_runtime.inner());
    run_blocking_command(move || {
        node_runtime
            .select_node(&request.selector_id, &request.node_id)
            .map_err(map_node_runtime_error)
    })
    .await
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tauri::command]
async fn set_node_selection_mode(
    request: SetNodeSelectionModeRequest,
    service: tauri::State<'_, DesktopBusinessService>,
    node_runtime: tauri::State<'_, Arc<dyn orange_platform::NodeRuntimeHost>>,
) -> Result<NodeSelectionModeResponse, CommandError> {
    let request = request.validate()?;
    require_authenticated(&service)?;
    let node_runtime = Arc::clone(node_runtime.inner());
    run_blocking_command(move || {
        let mode = node_runtime
            .set_selection_mode(request.mode)
            .map_err(map_node_runtime_error)?;
        Ok(NodeSelectionModeResponse::new(mode))
    })
    .await
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tauri::command]
async fn test_node_delays(
    request: NodeDelayTestRequest,
    service: tauri::State<'_, DesktopBusinessService>,
    node_runtime: tauri::State<'_, Arc<dyn orange_platform::NodeRuntimeHost>>,
) -> Result<NodeDelayTestResponse, CommandError> {
    request.validate()?;
    require_authenticated(&service)?;
    let node_runtime = Arc::clone(node_runtime.inner());
    run_blocking_command(move || {
        node_runtime
            .test_all_node_delays()
            .map_err(map_node_runtime_error)
    })
    .await
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
async fn run_blocking_command<T, F>(operation: F) -> Result<T, CommandError>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, CommandError> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(operation)
        .await
        .map_err(|_| CommandError::from_code(ErrorCode::Service))?
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn require_authenticated(service: &DesktopBusinessService) -> Result<(), CommandError> {
    if service.session().status == AuthSessionStatus::Authenticated {
        Ok(())
    } else {
        Err(CommandError::from_code(ErrorCode::Permission))
    }
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn map_node_runtime_error(error: orange_platform::NodeRuntimeError) -> CommandError {
    let code = match error {
        orange_platform::NodeRuntimeError::InvalidRequest
        | orange_platform::NodeRuntimeError::UnknownSelector
        | orange_platform::NodeRuntimeError::UnknownNode => ErrorCode::Validation,
        orange_platform::NodeRuntimeError::OperationInProgress => ErrorCode::Cancelled,
        orange_platform::NodeRuntimeError::BackendUnavailable
        | orange_platform::NodeRuntimeError::SelectionRejected
        | orange_platform::NodeRuntimeError::InvalidBackendState
        | orange_platform::NodeRuntimeError::SelectionReadbackMismatch
        | orange_platform::NodeRuntimeError::Persistence
        | orange_platform::NodeRuntimeError::RollbackFailed
        | orange_platform::NodeRuntimeError::TrafficInactive
        | orange_platform::NodeRuntimeError::TrafficCounterRegression
        | orange_platform::NodeRuntimeError::TrafficClockRegression
        | orange_platform::NodeRuntimeError::TrafficCounterOverflow => ErrorCode::Service,
    };
    CommandError::from_code(code)
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
fn refresh_and_apply_subscription(
    service: &DesktopBusinessService,
    business_client: &BusinessCommandClient<
        Arc<control_plane::ManagedControlPlane>,
        DesktopSecretStore,
    >,
    subscription_runtime: &dyn desktop_node_runtime::DesktopSubscriptionApplier,
    connection_preferences: &connection_preferences::ConnectionPreferences,
    #[cfg(target_os = "windows")] proxy_runtime: &windows_proxy_runtime::WindowsProxyRuntime,
    node_runtime: &Arc<dyn orange_platform::NodeRuntimeHost>,
) -> Result<SubscriptionPublicResponse, CommandError> {
    #[cfg(target_os = "windows")]
    let _proxy_operation = proxy_runtime.begin_operation();
    #[cfg(target_os = "windows")]
    proxy_runtime
        .restore_before_stop()
        .map_err(|_| CommandError::from_code(orange_domain::ErrorCode::Service))?;
    let result = download_and_apply_subscription(
        service,
        business_client,
        subscription_runtime,
        connection_preferences.mode(),
        connection_preferences.routing_mode(),
        connection_preferences.proxy_port(),
    );
    #[cfg(target_os = "windows")]
    if proxy_runtime.reconcile_now().is_err() {
        proxy_runtime.fail_closed();
        return Err(CommandError::from_code(ErrorCode::Service));
    }
    if result.is_ok() {
        let load_service = Arc::clone(service);
        let load_runtime = Arc::clone(node_runtime);
        let _ = std::thread::Builder::new()
            .name("orange-node-load-immediate".to_owned())
            .spawn(move || {
                if let Ok(snapshot) = load_service.fetch_node_loads() {
                    load_runtime.update_load_snapshot(snapshot);
                }
            });
    }
    result
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
fn download_and_apply_subscription(
    service: &DesktopBusinessService,
    business_client: &BusinessCommandClient<
        Arc<control_plane::ManagedControlPlane>,
        DesktopSecretStore,
    >,
    subscription_runtime: &dyn desktop_node_runtime::DesktopSubscriptionApplier,
    connection_mode: orange_domain::ConnectionMode,
    routing_mode: orange_domain::RoutingMode,
    proxy_port: u16,
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
        .map_err(map_subscription_download_error)?;
    subscription_runtime
        .apply_vless(payload, connection_mode, routing_mode, proxy_port)
        .map_err(|_| CommandError::from_code(ErrorCode::Subscription))?;
    Ok(response)
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
fn map_subscription_download_error(error: BusinessClientError) -> CommandError {
    let code = match error {
        BusinessClientError::Transport(
            BootstrapTransportError::Unavailable
            | BootstrapTransportError::DnsFailure
            | BootstrapTransportError::TlsFailure,
        ) => ErrorCode::Network,
        BusinessClientError::Transport(BootstrapTransportError::Timeout) => ErrorCode::Timeout,
        BusinessClientError::Transport(BootstrapTransportError::Cancelled) => ErrorCode::Cancelled,
        BusinessClientError::AuthenticationRequired | BusinessClientError::Unauthorized => {
            ErrorCode::Permission
        }
        BusinessClientError::SecretStore(_) => ErrorCode::Internal,
        BusinessClientError::InvalidRequest
        | BusinessClientError::Transport(
            BootstrapTransportError::InvalidRequest
            | BootstrapTransportError::InvalidResponse
            | BootstrapTransportError::ResponseTooLarge,
        )
        | BusinessClientError::RedirectDenied
        | BusinessClientError::RequestRejected
        | BusinessClientError::RateLimited
        | BusinessClientError::ServiceUnavailable
        | BusinessClientError::InvalidResponse => ErrorCode::Subscription,
    };
    CommandError::from_code(code)
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
fn switch_desktop_connection_mode(
    target: orange_domain::ConnectionMode,
    preferences: &connection_preferences::ConnectionPreferences,
    planes: &planes::ManagedPlanes,
    control: &planes::ManagedDataPlaneControl,
    service: &DesktopBusinessService,
    runtime: &DesktopConnectionModeRuntime,
) -> Result<ConnectionModeResponse, CommandError> {
    let _reconfiguration = preferences
        .begin_reconfiguration()
        .map_err(|()| CommandError::from_code(ErrorCode::Cancelled))?;
    let previous = preferences.mode();
    if previous == target {
        return Ok(ConnectionModeResponse::new(target));
    }
    reconfigure_desktop_data_plane(
        target,
        preferences.routing_mode(),
        preferences.proxy_port(),
        planes,
        control,
        service,
        runtime,
        || preferences.set_mode(target),
        || preferences.set_mode(previous),
    )?;
    Ok(ConnectionModeResponse::new(target))
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
fn switch_desktop_routing_mode(
    target: orange_domain::RoutingMode,
    preferences: &connection_preferences::ConnectionPreferences,
    planes: &planes::ManagedPlanes,
    control: &planes::ManagedDataPlaneControl,
    service: &DesktopBusinessService,
    runtime: &DesktopConnectionModeRuntime,
) -> Result<RoutingModeResponse, CommandError> {
    let _reconfiguration = preferences
        .begin_reconfiguration()
        .map_err(|()| CommandError::from_code(ErrorCode::Cancelled))?;
    let previous = preferences.routing_mode();
    if previous == target {
        return Ok(RoutingModeResponse::new(target));
    }
    reconfigure_desktop_data_plane(
        preferences.mode(),
        target,
        preferences.proxy_port(),
        planes,
        control,
        service,
        runtime,
        || preferences.set_routing_mode(target),
        || preferences.set_routing_mode(previous),
    )?;
    Ok(RoutingModeResponse::new(target))
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
fn reconfigure_desktop_data_plane(
    connection_mode: orange_domain::ConnectionMode,
    routing_mode: orange_domain::RoutingMode,
    proxy_port: u16,
    planes: &planes::ManagedPlanes,
    control: &planes::ManagedDataPlaneControl,
    service: &DesktopBusinessService,
    runtime: &DesktopConnectionModeRuntime,
    persist: impl FnOnce() -> Result<bool, orange_platform::PersistenceError>,
    rollback_preference: impl FnOnce() -> Result<bool, orange_platform::PersistenceError>,
) -> Result<(), CommandError> {
    #[cfg(target_os = "windows")]
    let _proxy_operation = runtime.proxy_runtime.begin_operation();
    let status = control.execute(DataPlaneControlAction::Status, planes)?;
    if matches!(
        status.data_plane,
        DataPlaneState::Validating
            | DataPlaneState::Starting
            | DataPlaneState::Stopping
            | DataPlaneState::Rollback
    ) {
        return Err(CommandError::from_code(ErrorCode::Service));
    }
    let reconnect = status.data_plane == DataPlaneState::Online;
    #[cfg(target_os = "windows")]
    runtime
        .proxy_runtime
        .restore_before_stop()
        .map_err(|_| CommandError::from_code(ErrorCode::Service))?;
    if status.can_stop {
        control.execute(DataPlaneControlAction::Stop, planes)?;
    }

    let applied = download_and_apply_subscription(
        service,
        &runtime.business_client,
        runtime.subscription_runtime.as_ref(),
        connection_mode,
        routing_mode,
        proxy_port,
    );
    if let Err(error) = applied {
        if !restore_previous_connection_after_reconfigure(reconnect, planes, control, runtime) {
            fail_closed_desktop_data_plane(runtime);
            return Err(CommandError::from_code(ErrorCode::Internal));
        }
        return Err(error);
    }
    if persist().is_err() {
        let _ =
            rollback_desktop_reconfiguration(reconnect, planes, control, runtime, rollback_preference);
        return Err(CommandError::from_code(ErrorCode::Internal));
    }
    if reconnect {
        #[cfg(target_os = "macos")]
        {
            let start = runtime
                .node_runtime
                .prepare_auto_selection()
                .map_err(map_node_runtime_error)
                .and_then(|()| {
                    control
                        .execute(DataPlaneControlAction::Start, planes)
                        .map(drop)
                })
                .and_then(|()| wait_for_desktop_data_plane_online(planes, control));
            if start.is_err() {
                let rollback_succeeded = rollback_desktop_reconfiguration(
                    true,
                    planes,
                    control,
                    runtime,
                    rollback_preference,
                );
                return Err(CommandError::from_code(if rollback_succeeded {
                    ErrorCode::Service
                } else {
                    ErrorCode::Internal
                }));
            }
        }
        #[cfg(target_os = "windows")]
        if runtime.proxy_runtime.reconcile_now().is_err() {
            let rollback_succeeded =
                rollback_desktop_reconfiguration(true, planes, control, runtime, rollback_preference);
            return Err(CommandError::from_code(if rollback_succeeded {
                ErrorCode::Service
            } else {
                ErrorCode::Internal
            }));
        }
    } else {
        #[cfg(target_os = "windows")]
        runtime
            .proxy_runtime
            .restore_before_stop()
            .map_err(|_| CommandError::from_code(ErrorCode::Service))?;
        control.execute(DataPlaneControlAction::Stop, planes)?;
    }
    Ok(())
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
fn reconfigure_desktop_proxy_port(
    port: u16,
    previous: u16,
    preferences: &connection_preferences::ConnectionPreferences,
    planes: &planes::ManagedPlanes,
    control: &planes::ManagedDataPlaneControl,
    runtime: &DesktopConnectionModeRuntime,
) -> Result<(), CommandError> {
    #[cfg(target_os = "windows")]
    let _proxy_operation = runtime.proxy_runtime.begin_operation();
    let status = control.execute(DataPlaneControlAction::Status, planes)?;
    if matches!(
        status.data_plane,
        DataPlaneState::Validating
            | DataPlaneState::Starting
            | DataPlaneState::Stopping
            | DataPlaneState::Rollback
    ) {
        return Err(CommandError::from_code(ErrorCode::Service));
    }
    let reconnect = status.data_plane == DataPlaneState::Online;
    let has_revision = runtime
        .subscription_runtime
        .has_active_revision()
        .map_err(|_| CommandError::from_code(ErrorCode::Service))?;
    if !has_revision {
        if reconnect {
            fail_closed_desktop_data_plane(runtime);
            return Err(CommandError::from_code(ErrorCode::Internal));
        }
        preferences
            .set_proxy_port(port)
            .map_err(|_| CommandError::from_code(ErrorCode::Internal))?;
        return Ok(());
    }

    #[cfg(target_os = "windows")]
    runtime
        .proxy_runtime
        .restore_before_stop()
        .map_err(|_| CommandError::from_code(ErrorCode::Service))?;
    if status.can_stop {
        control.execute(DataPlaneControlAction::Stop, planes)?;
    }
    if let Err(error) = runtime
        .subscription_runtime
        .reconfigure_proxy_port(port, reconnect)
    {
        if matches!(
            error,
            orange_platform::PlatformVpnError::ProtocolViolation
                | orange_platform::PlatformVpnError::CleanupFailed
        ) {
            fail_closed_desktop_data_plane(runtime);
            return Err(CommandError::from_code(ErrorCode::Internal));
        }
        if !restore_previous_connection_after_reconfigure(reconnect, planes, control, runtime) {
            fail_closed_desktop_data_plane(runtime);
            return Err(CommandError::from_code(ErrorCode::Internal));
        }
        return Err(CommandError::from_code(ErrorCode::Service));
    }
    if preferences.set_proxy_port(port).is_err() {
        let rollback_succeeded = rollback_desktop_proxy_port_reconfiguration(
            reconnect,
            planes,
            control,
            runtime,
            || preferences.set_proxy_port(previous),
        );
        return Err(CommandError::from_code(if rollback_succeeded {
            ErrorCode::Service
        } else {
            ErrorCode::Internal
        }));
    }
    if !restore_previous_connection_after_reconfigure(reconnect, planes, control, runtime) {
        let rollback_succeeded = rollback_desktop_proxy_port_reconfiguration(
            reconnect,
            planes,
            control,
            runtime,
            || preferences.set_proxy_port(previous),
        );
        return Err(CommandError::from_code(if rollback_succeeded {
            ErrorCode::Service
        } else {
            ErrorCode::Internal
        }));
    }
    Ok(())
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
fn rollback_desktop_proxy_port_reconfiguration(
    reconnect: bool,
    planes: &planes::ManagedPlanes,
    control: &planes::ManagedDataPlaneControl,
    runtime: &DesktopConnectionModeRuntime,
    rollback_preference: impl FnOnce() -> Result<bool, orange_platform::PersistenceError>,
) -> bool {
    let revision_restored = if reconnect {
        runtime
            .subscription_runtime
            .rollback_proxy_port_reconfiguration()
    } else {
        runtime
            .subscription_runtime
            .rollback_proxy_port_reconfiguration_offline()
    }
    .is_ok();
    let preference_restored = rollback_preference().is_ok();
    if revision_restored
        && preference_restored
        && restore_previous_connection_after_reconfigure(reconnect, planes, control, runtime)
    {
        true
    } else {
        fail_closed_desktop_data_plane(runtime);
        false
    }
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
fn rollback_desktop_reconfiguration(
    reconnect: bool,
    planes: &planes::ManagedPlanes,
    control: &planes::ManagedDataPlaneControl,
    runtime: &DesktopConnectionModeRuntime,
    rollback_preference: impl FnOnce() -> Result<bool, orange_platform::PersistenceError>,
) -> bool {
    let revision_restored = runtime.subscription_runtime.rollback_to_previous().is_ok();
    let preference_restored = rollback_preference().is_ok();
    if revision_restored && preference_restored {
        if restore_previous_connection_after_reconfigure(reconnect, planes, control, runtime) {
            true
        } else {
            fail_closed_desktop_data_plane(runtime);
            false
        }
    } else {
        fail_closed_desktop_data_plane(runtime);
        false
    }
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
fn fail_closed_desktop_data_plane(runtime: &DesktopConnectionModeRuntime) {
    #[cfg(target_os = "windows")]
    runtime.proxy_runtime.fail_closed();
    #[cfg(target_os = "macos")]
    let _ = runtime.node_runtime.stop_data_plane();
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
fn wait_for_desktop_data_plane_online(
    planes: &planes::ManagedPlanes,
    control: &planes::ManagedDataPlaneControl,
) -> Result<(), CommandError> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(12);
    loop {
        let status = control.execute(DataPlaneControlAction::Status, planes)?;
        match status.data_plane {
            DataPlaneState::Online => return Ok(()),
            DataPlaneState::Validating | DataPlaneState::Starting
                if std::time::Instant::now() < deadline =>
            {
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            _ => return Err(CommandError::from_code(ErrorCode::Service)),
        }
    }
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
fn restore_previous_connection_after_reconfigure(
    reconnect: bool,
    planes: &planes::ManagedPlanes,
    control: &planes::ManagedDataPlaneControl,
    runtime: &DesktopConnectionModeRuntime,
) -> bool {
    let mut status = match control.execute(DataPlaneControlAction::Status, planes) {
        Ok(status) => status,
        Err(_) => return false,
    };
    if reconnect && status.data_plane != DataPlaneState::Online {
        if !status.can_start {
            return false;
        }
        #[cfg(target_os = "macos")]
        if runtime.node_runtime.prepare_auto_selection().is_err() {
            return false;
        }
        if control
            .execute(DataPlaneControlAction::Start, planes)
            .is_err()
            || wait_for_desktop_data_plane_online(planes, control).is_err()
        {
            return false;
        }
    } else if !reconnect && status.can_stop {
        if control
            .execute(DataPlaneControlAction::Stop, planes)
            .is_err()
        {
            return false;
        }
        status = match control.execute(DataPlaneControlAction::Status, planes) {
            Ok(status) => status,
            Err(_) => return false,
        };
        if status.data_plane == DataPlaneState::Online || status.can_stop {
            return false;
        }
    }
    #[cfg(target_os = "windows")]
    if runtime.proxy_runtime.reconcile_now().is_err() {
        return false;
    }
    #[cfg(target_os = "macos")]
    let _ = runtime;
    true
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
fn load_routing_rule_resources(
    root: &std::path::Path,
) -> Result<RoutingRuleResources, std::io::Error> {
    let manifest = std::fs::read(root.join("resource-manifest.json"))?;
    let store = RuleResourceStore::open_user_private(root)
        .map_err(|_| std::io::Error::other("invalid routing resource directory"))?;
    store
        .activate_manifest(&manifest)
        .map_err(|_| std::io::Error::other("invalid routing resource manifest"))?;
    RoutingRuleResources::from_store(&store)
        .map_err(|_| std::io::Error::other("invalid routing rule resources"))
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

#[cfg(target_os = "windows")]
fn activate_existing_instance(app: &tauri::AppHandle) {
    windows_tray::activate_existing_instance(app);
}

/// Tears the data plane down before exit. Never refuses to exit: an unreachable
/// helper used to abort the exit entirely, which trapped the user in an app that
/// could neither connect nor quit. When the teardown cannot be confirmed the
/// recovery markers are deliberately left in place so the next launch adopts
/// whatever is still running.
#[cfg(target_os = "macos")]
fn cleanup_macos_on_exit(app: &tauri::AppHandle) {
    const STATUS_FAILURES_ALLOWED: usize = 3;
    let control = app.state::<planes::ManagedDataPlaneControl>();
    let planes = app.state::<planes::ManagedPlanes>();
    control.begin_shutdown();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(12);
    let mut status_failures = 0usize;
    while std::time::Instant::now() < deadline {
        if control.operation_in_flight() {
            std::thread::sleep(std::time::Duration::from_millis(50));
            continue;
        }
        let status = match control.execute(DataPlaneControlAction::Status, &planes) {
            Ok(status) => {
                status_failures = 0;
                status
            }
            Err(_) => {
                status_failures += 1;
                if status_failures >= STATUS_FAILURES_ALLOWED {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
                continue;
            }
        };
        if status.can_stop {
            if control.execute_shutdown_stop(&planes).is_err() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
            continue;
        }
        if matches!(
            status.data_plane,
            DataPlaneState::Validating
                | DataPlaneState::Starting
                | DataPlaneState::Stopping
                | DataPlaneState::Rollback
        ) {
            std::thread::sleep(std::time::Duration::from_millis(50));
            continue;
        }
        if status.data_plane != DataPlaneState::Online {
            if macos_node_runtime::clear_connection_recovery().is_ok() {
                let _ = app
                    .state::<connection_recovery::ConnectionRecovery>()
                    .clear();
            }
            return;
        }
        break;
    }
    // Best-effort last resort: the helper could not confirm the stop, so ask the
    // node runtime directly instead of blocking the exit.
    let _ = app
        .state::<std::sync::Arc<dyn orange_platform::NodeRuntimeHost>>()
        .stop_data_plane();
}

#[cfg(all(
    not(any(target_os = "android", target_os = "ios")),
    not(target_os = "windows")
))]
fn activate_existing_instance(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    #[cfg(target_os = "windows")]
    {
        let arguments = std::env::args_os().skip(1).collect::<Vec<_>>();
        if orange_windows_service::is_restore_invocation(&arguments) {
            let _ = orange_windows_service::restore_system_proxy_for_current_user();
            return;
        }
        if let Some(parent_process_id) =
            orange_windows_service::watchdog_parent_process_id(&arguments)
        {
            let _ = orange_windows_service::run_system_proxy_watchdog(parent_process_id);
            return;
        }
    }
    #[cfg(target_os = "windows")]
    let desktop_client = windows_node_runtime::discover_client();
    #[cfg(target_os = "macos")]
    let desktop_client = Some(macos_node_runtime::discover_client());
    #[cfg(target_os = "windows")]
    let planes = desktop_client
        .as_ref()
        .map_or_else(planes::ManagedPlanes::default, |client| {
            planes::ManagedPlanes::with_adapter(client.clone())
        });
    #[cfg(target_os = "macos")]
    let planes = planes::ManagedPlanes::with_adapter(
        desktop_client
            .as_ref()
            .expect("macOS service client must be configured")
            .clone(),
    );
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
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
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    let connection_mode_business_client = Arc::clone(&business_client);
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    let business_service = Arc::new(BusinessApiService::new(
        Arc::clone(&business_client),
        SystemClock,
    ));
    let diagnostics = Arc::new(DiagnosticsHub::default());
    let data_plane_events = Arc::new(DataPlaneEventHub::default());
    let builder = tauri::Builder::default();
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    let builder = builder.plugin(tauri_plugin_single_instance::init(
        |app, _arguments, _working_directory| {
            activate_existing_instance(app);
        },
    ));
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    let builder = builder.plugin(tauri_plugin_updater::Builder::new().build());
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    let builder = builder.plugin(tauri_plugin_process::init());
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    let builder = builder.plugin(
        tauri_plugin_autostart::Builder::new()
            .app_name("Orange")
            .build(),
    );
    let builder = builder
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
        .manage(Arc::clone(&business_service));
    let builder = builder.setup(move |app| {
        let app_data_dir = app.path().app_data_dir()?;
        let store = Arc::new(FileSettingsStore::new(app_data_dir.clone())?);
        let _ = store.load()?;
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        let connection_preferences = Arc::new(connection_preferences::ConnectionPreferences::load(
            Arc::clone(&store),
        )?);
        #[cfg(target_os = "windows")]
        let desktop_node_runtime_host =
            Arc::new(windows_node_runtime::WindowsNodeRuntimeHost::new(
                desktop_client.clone(),
                Arc::clone(&store),
            ));
        #[cfg(target_os = "macos")]
        let desktop_node_runtime_host = Arc::new(macos_node_runtime::MacosNodeRuntimeHost::new(
            desktop_client.clone(),
            Arc::clone(&store),
        ));
        // The command layer only ever sees the platform-neutral façade; desktop
        // targets without a data plane get the unconfigured stand-in so the same
        // command set stays registered everywhere.
        #[cfg(target_os = "windows")]
        let node_runtime: Arc<dyn orange_platform::NodeRuntimeHost> =
            Arc::clone(&desktop_node_runtime_host) as Arc<dyn orange_platform::NodeRuntimeHost>;
        #[cfg(target_os = "macos")]
        let node_runtime: Arc<dyn orange_platform::NodeRuntimeHost> =
            Arc::clone(&desktop_node_runtime_host) as Arc<dyn orange_platform::NodeRuntimeHost>;
        #[cfg(not(any(target_os = "windows", target_os = "macos")))]
        let node_runtime: Arc<dyn orange_platform::NodeRuntimeHost> =
            Arc::new(orange_platform::UnconfiguredNodeRuntimeHost);
        let _ = node_runtime.recover();
        {
            let load_node_runtime = Arc::clone(&node_runtime);
            let load_business_service = Arc::clone(&business_service);
            std::thread::Builder::new()
                .name("orange-node-load-refresh".to_owned())
                .spawn(move || {
                    loop {
                        let authenticated = load_business_service.session().status
                            == orange_domain::AuthSessionStatus::Authenticated;
                        if authenticated
                            && let Ok(snapshot) = load_business_service.fetch_node_loads()
                        {
                            load_node_runtime.update_load_snapshot(snapshot);
                        }
                        let wait_seconds = if authenticated {
                            load_node_runtime.load_refresh_interval_seconds()
                        } else {
                            5
                        };
                        std::thread::sleep(std::time::Duration::from_secs(wait_seconds));
                    }
                })?;
        }
        #[cfg(target_os = "windows")]
        let subscription_runtime = Arc::new(windows_node_runtime::WindowsSubscriptionRuntime::new(
            desktop_client,
            Arc::clone(&store),
            Arc::clone(&desktop_node_runtime_host),
            load_routing_rule_resources(&app.path().resource_dir()?.join("rules"))?,
        ));
        #[cfg(target_os = "macos")]
        let subscription_runtime = Arc::new(macos_node_runtime::MacosSubscriptionRuntime::new(
            desktop_client,
            Arc::clone(&store),
            Arc::clone(&desktop_node_runtime_host),
            load_routing_rule_resources(&app.path().resource_dir()?.join("rules"))?,
        ));
        #[cfg(any(target_os = "windows", target_os = "macos"))]
        let data_plane_event_monitor = node_runtime.is_provisioned().then(|| {
            DataPlaneEventMonitor::start(
                Arc::clone(&node_runtime),
                Arc::clone(&data_plane_events),
                Arc::clone(&diagnostics),
            )
        });
        #[cfg(any(target_os = "windows", target_os = "macos"))]
        let data_plane_event_monitor = data_plane_event_monitor.transpose()?;
        #[cfg(target_os = "macos")]
        let selection_runtime = node_runtime
            .is_provisioned()
            .then(|| {
                macos_selection_runtime::MacosSelectionRuntime::start(Arc::clone(&node_runtime))
            })
            .transpose()?;
        #[cfg(target_os = "windows")]
        let system_proxy_manager = Arc::new(
            orange_windows_service::WindowsSystemProxyManager::new(std::env::current_exe()?)?,
        );
        #[cfg(target_os = "windows")]
        let proxy_runtime = Arc::new(windows_proxy_runtime::WindowsProxyRuntime::start(
            system_proxy_manager,
            Arc::clone(&connection_preferences),
            Arc::clone(&node_runtime),
        )?);
        #[cfg(any(target_os = "windows", target_os = "macos"))]
        app.manage(DesktopConnectionModeRuntime {
            business_client: connection_mode_business_client,
            subscription_runtime: Arc::clone(&subscription_runtime)
                as Arc<dyn desktop_node_runtime::DesktopSubscriptionApplier>,
            #[cfg(target_os = "macos")]
            node_runtime: Arc::clone(&node_runtime),
            #[cfg(target_os = "windows")]
            proxy_runtime: Arc::clone(&proxy_runtime),
        });
        app.manage(planes::ManagedDataPlaneControl::with_source(Arc::new(
            EligibleRevisionSource {
                node_runtime: Arc::clone(&node_runtime),
                business_service: Arc::clone(&business_service),
            },
        )));
        app.manage(node_runtime);
        #[cfg(any(target_os = "windows", target_os = "macos"))]
        app.manage(subscription_runtime);
        #[cfg(any(target_os = "windows", target_os = "macos"))]
        app.manage(data_plane_event_monitor);
        #[cfg(target_os = "macos")]
        app.manage(selection_runtime);
        #[cfg(target_os = "windows")]
        app.manage(proxy_runtime);
        app.manage(connection_recovery::ConnectionRecovery::new(&app_data_dir));
        app.manage(store);
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        app.manage(connection_preferences);
        #[cfg(target_os = "windows")]
        windows_tray::install(app)?;
        Ok(())
    });
    #[cfg(target_os = "windows")]
    let builder = builder
        .on_menu_event(windows_tray::handle_menu_event)
        .on_tray_icon_event(windows_tray::handle_tray_icon_event)
        .on_window_event(windows_tray::handle_window_event);
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    let builder = builder.invoke_handler(tauri::generate_handler![
        get_plane_state,
        get_runtime_info,
        check_macos_package_update,
        prepare_macos_package_update,
        get_data_plane_event_snapshot,
        control_data_plane,
        get_connection_mode,
        set_connection_mode,
        get_routing_mode,
        set_routing_mode,
        get_proxy_port,
        set_proxy_port,
        get_launch_on_startup,
        set_launch_on_startup,
        open_network_tool,
        open_legal_document,
        initialize_business,
        open_service_portal,
        get_service_portal_url,
        open_telegram_bot,
        login,
        send_email_verification,
        reset_password,
        register,
        get_auth_session,
        logout,
        refresh_account,
        fetch_notices,
        fetch_plans,
        fetch_orders,
        fetch_order_detail,
        fetch_payment_methods,
        checkout_order,
        cancel_order,
        create_order,
        fetch_invitation_center,
        generate_invitation_code,
        check_gift_card,
        redeem_gift_card,
        fetch_gift_card_history,
        fetch_commission_config,
        withdraw_commission,
        transfer_commission,
        fetch_active_sessions,
        remove_active_session,
        fetch_knowledge_list,
        fetch_knowledge_detail,
        fetch_tickets,
        fetch_ticket_detail,
        create_ticket,
        reply_ticket,
        close_ticket,
        refresh_subscription,
        get_subscription_snapshot,
        get_node_catalog,
        select_node,
        set_node_selection_mode,
        test_node_delays
    ]);
    #[cfg(any(target_os = "android", target_os = "ios"))]
    let builder =
        builder.invoke_handler(tauri::generate_handler![get_plane_state, get_runtime_info]);
    #[cfg(target_os = "macos")]
    builder
        .build(tauri::generate_context!())
        .expect("failed to build Orange application")
        .run(|app, event| {
            if let tauri::RunEvent::ExitRequested { .. } = event {
                cleanup_macos_on_exit(app);
            }
        });
    #[cfg(not(target_os = "macos"))]
    builder
        .run(tauri::generate_context!())
        .expect("failed to run Orange application");
}

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
    AuthSessionResponse, BusinessInitializationResponse, CancelOrderCommandRequest,
    CancelOrderResponse, CheckoutOrderCommandRequest, CloseTicketCommandRequest,
    ConnectionModeRequest, ConnectionModeResponse, CreateOrderCommandRequest, CreateOrderResponse,
    CreateTicketCommandRequest, DataPlaneControlRequest, DataPlaneControlResponse,
    ActiveSessionsRequest, ActiveSessionsResponse, CommissionConfigRequest,
    CommissionConfigResponse, CommissionOperationResponse,
    DataPlaneEventSnapshotRequest, EmailVerificationResponse, ErrorCode,
    GiftCardCheckResponse, GiftCardCodeCommandRequest, GiftCardHistoryRequest,
    GiftCardHistoryResponse, GiftCardRedeemResponse, InitializeBusinessRequest,
    InvitationCenterRequest, InvitationCenterResponse, KnowledgeDetailCommandRequest,
    KnowledgeDetailResponse, KnowledgeListCommandRequest, KnowledgeListResponse,
    LaunchOnStartupRequest,
    LaunchOnStartupResponse, LegalDocument, LoginCommandRequest, LogoutRequest, NetworkTool,
    NoticesRequest, NoticesResponse, OpenLegalDocumentRequest, OpenLegalDocumentResponse,
    OpenNetworkToolRequest, OpenNetworkToolResponse, OpenServicePortalRequest,
    OpenServicePortalResponse, OrderDetailCommandRequest, OrderDetailResponse, OrdersRequest,
    OrdersResponse, PasswordResetResponse, PaymentMethodsRequest, PaymentMethodsResponse,
    PaymentPublicResponse, PlansRequest, PlansResponse, RegisterCommandRequest,
    RemoveActiveSessionCommandRequest,
    ReplyTicketCommandRequest, ResetPasswordCommandRequest, RoutingModeRequest,
    RoutingModeResponse, SendEmailVerificationCommandRequest, ServicePortalUrlResponse,
    SetConnectionModeRequest,
    SetLaunchOnStartupRequest, SetRoutingModeRequest,
    SubscriptionPublicResponse,
    SubscriptionRefreshRequest, TicketDetailCommandRequest, TicketDetailResponse, TicketsRequest,
    TicketsResponse, TransferCommissionCommandRequest, WithdrawCommissionCommandRequest,
};
#[cfg(target_os = "windows")]
use orange_domain::{
    AuthSessionStatus, DataPlaneControlAction, DataPlaneState, NodeCatalogRequest,
    NodeCatalogResponse, NodeDelayTestRequest, NodeDelayTestResponse, SelectNodeRequest,
    SelectNodeResponse, SubscriptionSnapshotRequest, SubscriptionSnapshotResponse,
    SubscriptionStatus,
};
#[cfg(target_os = "windows")]
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
pub mod control_plane;
mod planes;
#[cfg(target_os = "windows")]
pub mod windows_node_runtime;
#[cfg(target_os = "windows")]
mod windows_connection_recovery;
#[cfg(target_os = "windows")]
mod windows_proxy_runtime;
#[cfg(target_os = "windows")]
mod windows_tray;

#[cfg(not(any(target_os = "android", target_os = "ios")))]
type DesktopBusinessService =
    Arc<BusinessApiService<Arc<control_plane::ManagedControlPlane>, DesktopSecretStore>>;

#[cfg(target_os = "windows")]
struct EligibleWindowsRevisionSource {
    node_runtime: Arc<windows_node_runtime::WindowsNodeRuntimeHost>,
    business_service: DesktopBusinessService,
}

#[cfg(target_os = "windows")]
impl planes::ActiveConfigurationRevision for EligibleWindowsRevisionSource {
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
        planes::ActiveConfigurationRevision::active_configuration_revision(
            self.node_runtime.as_ref(),
        )
    }
}

#[cfg(target_os = "windows")]
struct WindowsConnectionModeRuntime {
    business_client:
        Arc<BusinessCommandClient<Arc<control_plane::ManagedControlPlane>, DesktopSecretStore>>,
    subscription_runtime: Arc<windows_node_runtime::WindowsSubscriptionRuntime>,
    proxy_runtime: Arc<windows_proxy_runtime::WindowsProxyRuntime>,
}

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
    app: tauri::AppHandle,
) -> Result<DataPlaneControlResponse, CommandError> {
    let request = request.validate()?;
    #[cfg(target_os = "windows")]
    {
        let proxy_runtime = app.state::<Arc<windows_proxy_runtime::WindowsProxyRuntime>>();
        let recovery = app.state::<windows_connection_recovery::WindowsConnectionRecovery>();
        execute_windows_data_plane_action(
            request.action,
            &planes,
            &control,
            &proxy_runtime,
            &recovery,
        )
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = app;
        control.execute(request.action, &planes)
    }
}

#[cfg(target_os = "windows")]
fn execute_windows_data_plane_action(
    action: DataPlaneControlAction,
    planes: &planes::ManagedPlanes,
    control: &planes::ManagedDataPlaneControl,
    proxy_runtime: &windows_proxy_runtime::WindowsProxyRuntime,
    recovery: &windows_connection_recovery::WindowsConnectionRecovery,
) -> Result<DataPlaneControlResponse, CommandError> {
    let proxy_operation =
        (action != DataPlaneControlAction::Status).then(|| proxy_runtime.begin_operation());
    if action == DataPlaneControlAction::Stop {
        proxy_runtime
            .restore_before_stop()
            .map_err(|_| CommandError::from_code(ErrorCode::Service))?;
    }
    let response = control.execute(action, planes)?;
    // The marker records connection *intent*: a successful Start returns with the
    // data plane still in `Starting` (it transitions to `Online` asynchronously),
    // so it must be written here rather than gated on `Online`. Persistence
    // failures are best-effort — they must not fail an otherwise successful
    // connect/disconnect.
    match action {
        DataPlaneControlAction::Start => {
            let _ = recovery.mark_connected();
        }
        DataPlaneControlAction::Stop => {
            let _ = recovery.clear();
        }
        DataPlaneControlAction::Status => {}
    }
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
    if preferences.mode() == request.mode {
        return Ok(ConnectionModeResponse::new(request.mode));
    }
    #[cfg(target_os = "windows")]
    {
        let planes = app.state::<planes::ManagedPlanes>();
        let control = app.state::<planes::ManagedDataPlaneControl>();
        let service = app.state::<DesktopBusinessService>();
        let runtime = app.state::<WindowsConnectionModeRuntime>();
        switch_windows_connection_mode(
            request.mode,
            &preferences,
            &planes,
            &control,
            &service,
            &runtime,
        )
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = app;
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
    if preferences.routing_mode() == request.mode {
        return Ok(RoutingModeResponse::new(request.mode));
    }
    #[cfg(target_os = "windows")]
    {
        let planes = app.state::<planes::ManagedPlanes>();
        let control = app.state::<planes::ManagedDataPlaneControl>();
        let service = app.state::<DesktopBusinessService>();
        let runtime = app.state::<WindowsConnectionModeRuntime>();
        switch_windows_routing_mode(
            request.mode,
            &preferences,
            &planes,
            &control,
            &service,
            &runtime,
        )
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = app;
        preferences
            .set_routing_mode(request.mode)
            .map_err(|_| CommandError::from_code(ErrorCode::Internal))?;
        Ok(RoutingModeResponse::new(request.mode))
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
    #[cfg(target_os = "windows")]
    if response.session.status == AuthSessionStatus::Authenticated {
        let business_client =
            app.state::<Arc<
                BusinessCommandClient<Arc<control_plane::ManagedControlPlane>, DesktopSecretStore>,
            >>();
        let subscription_runtime =
            app.state::<Arc<windows_node_runtime::WindowsSubscriptionRuntime>>();
        let connection_preferences =
            app.state::<Arc<connection_preferences::ConnectionPreferences>>();
        let proxy_runtime = app.state::<Arc<windows_proxy_runtime::WindowsProxyRuntime>>();
        let node_runtime = app.state::<Arc<windows_node_runtime::WindowsNodeRuntimeHost>>();
        let subscription_result = refresh_and_apply_subscription(
            &service,
            &business_client,
            &subscription_runtime,
            &connection_preferences,
            &proxy_runtime,
        );
        let has_local_revision = if subscription_result.is_err() {
            node_runtime
                .active_revision()
                .map_err(map_node_runtime_error)?
                .is_some()
        } else {
            false
        };
        accept_startup_subscription(subscription_result, has_local_revision)?;
        resume_windows_connection_if_needed(&app);
    }
    #[cfg(not(target_os = "windows"))]
    let _ = app;
    Ok(response)
}

#[cfg(target_os = "windows")]
fn resume_windows_connection_if_needed(app: &tauri::AppHandle) {
    let recovery = app.state::<windows_connection_recovery::WindowsConnectionRecovery>();
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
    let proxy_runtime = app.state::<Arc<windows_proxy_runtime::WindowsProxyRuntime>>();
    let _ = execute_windows_data_plane_action(
        DataPlaneControlAction::Start,
        &planes,
        &control,
        &proxy_runtime,
        &recovery,
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
fn open_support_chat(
    request: OpenServicePortalRequest,
    app: tauri::AppHandle,
) -> Result<OpenServicePortalResponse, CommandError> {
    request.validate()?;
    const SUPPORT_CHAT_LABEL: &str = "support-chat";
    // Same Crisp website as the UUVPN iOS client; move to config later.
    const CRISP_WEBSITE_ID: &str = "5546c6ea-4b1e-41bc-80e4-4b6648cbca76";
    if let Some(window) = app.get_webview_window(SUPPORT_CHAT_LABEL) {
        let _ = window.set_focus();
        return Ok(OpenServicePortalResponse::opened());
    }
    let url = tauri::Url::parse(&format!(
        "https://go.crisp.chat/chat/embed/?website_id={CRISP_WEBSITE_ID}"
    ))
    .map_err(|_| CommandError::from_code(ErrorCode::Internal))?;
    tauri::WebviewWindowBuilder::new(
        &app,
        SUPPORT_CHAT_LABEL,
        tauri::WebviewUrl::External(url),
    )
    .title("在线客服")
    .inner_size(420.0, 640.0)
    .min_inner_size(360.0, 480.0)
    .build()
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

#[cfg(target_os = "windows")]
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
    #[cfg(target_os = "windows")]
    {
        let business_client =
            app.state::<Arc<
                BusinessCommandClient<Arc<control_plane::ManagedControlPlane>, DesktopSecretStore>,
            >>();
        let subscription_runtime =
            app.state::<Arc<windows_node_runtime::WindowsSubscriptionRuntime>>();
        let connection_preferences =
            app.state::<Arc<connection_preferences::ConnectionPreferences>>();
        let proxy_runtime = app.state::<Arc<windows_proxy_runtime::WindowsProxyRuntime>>();
        refresh_and_apply_subscription(
            &service,
            &business_client,
            &subscription_runtime,
            &connection_preferences,
            &proxy_runtime,
        )?;
    }
    #[cfg(not(target_os = "windows"))]
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
    #[cfg(not(target_os = "windows"))]
    let _ = app;
    let response = service.logout(planes.inner()).map_err(map_business_error)?;
    // Best-effort: the session is already logged out server-side, so a marker
    // cleanup failure must not surface as a logout failure.
    #[cfg(target_os = "windows")]
    let _ = app
        .state::<windows_connection_recovery::WindowsConnectionRecovery>()
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
fn fetch_notices(
    request: NoticesRequest,
    service: tauri::State<'_, DesktopBusinessService>,
) -> Result<NoticesResponse, CommandError> {
    request.validate()?;
    service.fetch_notices().map_err(map_business_error)
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tauri::command]
fn fetch_plans(
    request: PlansRequest,
    service: tauri::State<'_, DesktopBusinessService>,
) -> Result<PlansResponse, CommandError> {
    request.validate()?;
    service.fetch_plans().map_err(map_business_error)
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
    let checkout = service
        .checkout_order(request)
        .map_err(map_business_error)?;
    // Free orders carry no payment URL; only open the browser when the
    // gateway returned one.
    if checkout.has_payment_url() {
        checkout
            .with_payment_url(|url| tauri_plugin_opener::open_url(url, None::<&str>))
            .map_err(|_| CommandError::from_code(ErrorCode::Service))?;
    }
    Ok(checkout.into_public_response())
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
fn create_order(
    request: CreateOrderCommandRequest,
    service: tauri::State<'_, DesktopBusinessService>,
) -> Result<CreateOrderResponse, CommandError> {
    let request = request.validate()?;
    service.create_order(request).map_err(map_business_error)
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
fn check_gift_card(
    request: GiftCardCodeCommandRequest,
    service: tauri::State<'_, DesktopBusinessService>,
) -> Result<GiftCardCheckResponse, CommandError> {
    let code = request.validate()?;
    service.check_gift_card(&code).map_err(map_business_error)
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tauri::command]
fn redeem_gift_card(
    request: GiftCardCodeCommandRequest,
    service: tauri::State<'_, DesktopBusinessService>,
) -> Result<GiftCardRedeemResponse, CommandError> {
    let code = request.validate()?;
    service.redeem_gift_card(&code).map_err(map_business_error)
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tauri::command]
fn fetch_gift_card_history(
    request: GiftCardHistoryRequest,
    service: tauri::State<'_, DesktopBusinessService>,
) -> Result<GiftCardHistoryResponse, CommandError> {
    request.validate()?;
    service
        .fetch_gift_card_history()
        .map_err(map_business_error)
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
    service
        .fetch_active_sessions()
        .map_err(map_business_error)
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
fn refresh_subscription(
    request: SubscriptionRefreshRequest,
    service: tauri::State<'_, DesktopBusinessService>,
    app: tauri::AppHandle,
) -> Result<SubscriptionPublicResponse, CommandError> {
    request.validate()?;
    #[cfg(target_os = "windows")]
    {
        let business_client =
            app.state::<Arc<
                BusinessCommandClient<Arc<control_plane::ManagedControlPlane>, DesktopSecretStore>,
            >>();
        let subscription_runtime =
            app.state::<Arc<windows_node_runtime::WindowsSubscriptionRuntime>>();
        let connection_preferences =
            app.state::<Arc<connection_preferences::ConnectionPreferences>>();
        let proxy_runtime = app.state::<Arc<windows_proxy_runtime::WindowsProxyRuntime>>();
        refresh_and_apply_subscription(
            &service,
            &business_client,
            &subscription_runtime,
            &connection_preferences,
            &proxy_runtime,
        )
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = app;
        service.refresh_subscription().map_err(map_business_error)
    }
}

#[cfg(target_os = "windows")]
#[tauri::command]
fn get_subscription_snapshot(
    request: SubscriptionSnapshotRequest,
    service: tauri::State<'_, DesktopBusinessService>,
    node_runtime: tauri::State<'_, Arc<windows_node_runtime::WindowsNodeRuntimeHost>>,
) -> Result<SubscriptionSnapshotResponse, CommandError> {
    request.validate()?;
    require_authenticated(&service)?;
    let local_revision = node_runtime
        .active_revision()
        .map_err(map_node_runtime_error)?
        .map(|revision| revision.get());
    Ok(SubscriptionSnapshotResponse::new(
        service.cached_subscription(),
        local_revision,
    ))
}

#[cfg(target_os = "windows")]
#[tauri::command]
fn get_node_catalog(
    request: NodeCatalogRequest,
    service: tauri::State<'_, DesktopBusinessService>,
    node_runtime: tauri::State<'_, Arc<windows_node_runtime::WindowsNodeRuntimeHost>>,
) -> Result<NodeCatalogResponse, CommandError> {
    request.validate()?;
    require_authenticated(&service)?;
    node_runtime
        .catalog_snapshot()
        .map_err(map_node_runtime_error)
}

#[cfg(target_os = "windows")]
#[tauri::command]
fn select_node(
    request: SelectNodeRequest,
    service: tauri::State<'_, DesktopBusinessService>,
    node_runtime: tauri::State<'_, Arc<windows_node_runtime::WindowsNodeRuntimeHost>>,
) -> Result<SelectNodeResponse, CommandError> {
    let request = request.validate()?;
    require_authenticated(&service)?;
    node_runtime
        .select_node(&request.selector_id, &request.node_id)
        .map_err(map_node_runtime_error)
}

#[cfg(target_os = "windows")]
#[tauri::command]
fn test_node_delays(
    request: NodeDelayTestRequest,
    service: tauri::State<'_, DesktopBusinessService>,
    node_runtime: tauri::State<'_, Arc<windows_node_runtime::WindowsNodeRuntimeHost>>,
) -> Result<NodeDelayTestResponse, CommandError> {
    request.validate()?;
    require_authenticated(&service)?;
    node_runtime
        .test_all_node_delays()
        .map_err(map_node_runtime_error)
}

#[cfg(target_os = "windows")]
fn require_authenticated(service: &DesktopBusinessService) -> Result<(), CommandError> {
    if service.session().status == AuthSessionStatus::Authenticated {
        Ok(())
    } else {
        Err(CommandError::from_code(ErrorCode::Permission))
    }
}

#[cfg(target_os = "windows")]
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

#[cfg(target_os = "windows")]
fn refresh_and_apply_subscription(
    service: &DesktopBusinessService,
    business_client: &BusinessCommandClient<
        Arc<control_plane::ManagedControlPlane>,
        DesktopSecretStore,
    >,
    subscription_runtime: &windows_node_runtime::WindowsSubscriptionRuntime,
    connection_preferences: &connection_preferences::ConnectionPreferences,
    proxy_runtime: &windows_proxy_runtime::WindowsProxyRuntime,
) -> Result<SubscriptionPublicResponse, CommandError> {
    let _proxy_operation = proxy_runtime.begin_operation();
    proxy_runtime
        .restore_before_stop()
        .map_err(|_| CommandError::from_code(orange_domain::ErrorCode::Service))?;
    let result = download_and_apply_subscription(
        service,
        business_client,
        subscription_runtime,
        connection_preferences.mode(),
        connection_preferences.routing_mode(),
    );
    if proxy_runtime.reconcile_now().is_err() {
        proxy_runtime.fail_closed();
        return Err(CommandError::from_code(ErrorCode::Service));
    }
    result
}

#[cfg(target_os = "windows")]
fn download_and_apply_subscription(
    service: &DesktopBusinessService,
    business_client: &BusinessCommandClient<
        Arc<control_plane::ManagedControlPlane>,
        DesktopSecretStore,
    >,
    subscription_runtime: &windows_node_runtime::WindowsSubscriptionRuntime,
    connection_mode: orange_domain::ConnectionMode,
    routing_mode: orange_domain::RoutingMode,
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
        .apply_vless(payload, connection_mode, routing_mode)
        .map_err(|_| CommandError::from_code(ErrorCode::Subscription))?;
    Ok(response)
}

#[cfg(target_os = "windows")]
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

#[cfg(target_os = "windows")]
fn switch_windows_connection_mode(
    target: orange_domain::ConnectionMode,
    preferences: &connection_preferences::ConnectionPreferences,
    planes: &planes::ManagedPlanes,
    control: &planes::ManagedDataPlaneControl,
    service: &DesktopBusinessService,
    runtime: &WindowsConnectionModeRuntime,
) -> Result<ConnectionModeResponse, CommandError> {
    reconfigure_windows_data_plane(
        target,
        preferences.routing_mode(),
        planes,
        control,
        service,
        runtime,
        || preferences.set_mode(target),
    )?;
    Ok(ConnectionModeResponse::new(target))
}

#[cfg(target_os = "windows")]
fn switch_windows_routing_mode(
    target: orange_domain::RoutingMode,
    preferences: &connection_preferences::ConnectionPreferences,
    planes: &planes::ManagedPlanes,
    control: &planes::ManagedDataPlaneControl,
    service: &DesktopBusinessService,
    runtime: &WindowsConnectionModeRuntime,
) -> Result<RoutingModeResponse, CommandError> {
    reconfigure_windows_data_plane(
        preferences.mode(),
        target,
        planes,
        control,
        service,
        runtime,
        || preferences.set_routing_mode(target),
    )?;
    Ok(RoutingModeResponse::new(target))
}

#[cfg(target_os = "windows")]
fn reconfigure_windows_data_plane(
    connection_mode: orange_domain::ConnectionMode,
    routing_mode: orange_domain::RoutingMode,
    planes: &planes::ManagedPlanes,
    control: &planes::ManagedDataPlaneControl,
    service: &DesktopBusinessService,
    runtime: &WindowsConnectionModeRuntime,
    persist: impl FnOnce() -> Result<bool, orange_platform::PersistenceError>,
) -> Result<(), CommandError> {
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
        &runtime.subscription_runtime,
        connection_mode,
        routing_mode,
    );
    if applied.is_err() {
        runtime.proxy_runtime.fail_closed();
        return applied.map(drop);
    }
    if persist().is_err() {
        runtime.proxy_runtime.fail_closed();
        return Err(CommandError::from_code(ErrorCode::Internal));
    }
    if reconnect {
        if runtime.proxy_runtime.reconcile_now().is_err() {
            runtime.proxy_runtime.fail_closed();
            return Err(CommandError::from_code(ErrorCode::Service));
        }
    } else {
        runtime
            .proxy_runtime
            .restore_before_stop()
            .map_err(|_| CommandError::from_code(ErrorCode::Service))?;
        control.execute(DataPlaneControlAction::Stop, planes)?;
    }
    Ok(())
}

#[cfg(target_os = "windows")]
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
    #[cfg(target_os = "windows")]
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
            load_routing_rule_resources(&app.path().resource_dir()?.join("rules"))?,
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
        let system_proxy_manager = Arc::new(
            orange_windows_service::WindowsSystemProxyManager::new(std::env::current_exe()?)?,
        );
        #[cfg(target_os = "windows")]
        let proxy_runtime = Arc::new(windows_proxy_runtime::WindowsProxyRuntime::start(
            system_proxy_manager,
            Arc::clone(&connection_preferences),
            Arc::clone(&node_runtime),
        )?);
        #[cfg(target_os = "windows")]
        app.manage(WindowsConnectionModeRuntime {
            business_client: connection_mode_business_client,
            subscription_runtime: Arc::clone(&subscription_runtime),
            proxy_runtime: Arc::clone(&proxy_runtime),
        });
        #[cfg(target_os = "windows")]
        app.manage(planes::ManagedDataPlaneControl::with_source(Arc::new(
            EligibleWindowsRevisionSource {
                node_runtime: Arc::clone(&node_runtime),
                business_service: Arc::clone(&business_service),
            },
        )));
        #[cfg(target_os = "windows")]
        app.manage(node_runtime);
        #[cfg(target_os = "windows")]
        app.manage(subscription_runtime);
        #[cfg(target_os = "windows")]
        app.manage(data_plane_event_monitor);
        #[cfg(target_os = "windows")]
        app.manage(proxy_runtime);
        #[cfg(target_os = "windows")]
        app.manage(windows_connection_recovery::WindowsConnectionRecovery::new(
            &app_data_dir,
        ));
        #[cfg(all(
            not(any(target_os = "android", target_os = "ios")),
            not(target_os = "windows")
        ))]
        app.manage(planes::ManagedDataPlaneControl::default());
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
    #[cfg(all(
        not(any(target_os = "android", target_os = "ios")),
        target_os = "windows"
    ))]
    let builder = builder.invoke_handler(tauri::generate_handler![
        get_plane_state,
        get_runtime_info,
        get_data_plane_event_snapshot,
        control_data_plane,
        get_connection_mode,
        set_connection_mode,
        get_routing_mode,
        set_routing_mode,
        get_launch_on_startup,
        set_launch_on_startup,
        open_network_tool,
        open_legal_document,
        initialize_business,
        open_service_portal,
        get_service_portal_url,
        open_telegram_bot,
        open_support_chat,
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
        test_node_delays
    ]);
    #[cfg(all(
        not(any(target_os = "android", target_os = "ios")),
        not(target_os = "windows")
    ))]
    let builder = builder.invoke_handler(tauri::generate_handler![
        get_plane_state,
        get_runtime_info,
        get_data_plane_event_snapshot,
        control_data_plane,
        get_connection_mode,
        set_connection_mode,
        get_routing_mode,
        set_routing_mode,
        get_launch_on_startup,
        set_launch_on_startup,
        open_network_tool,
        open_legal_document,
        initialize_business,
        open_service_portal,
        get_service_portal_url,
        open_telegram_bot,
        open_support_chat,
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
        refresh_subscription
    ]);
    #[cfg(any(target_os = "android", target_os = "ios"))]
    let builder =
        builder.invoke_handler(tauri::generate_handler![get_plane_state, get_runtime_info]);
    builder
        .run(tauri::generate_context!())
        .expect("failed to run Orange application");
}

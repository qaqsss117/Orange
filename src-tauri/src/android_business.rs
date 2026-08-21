use std::sync::Arc;

use orange_domain::*;
use orange_platform::{
    BusinessApiService, BusinessCommandClient, BusinessServiceError, SystemClock,
};

use crate::{
    android_control_plane::AndroidControlPlaneTransport,
    android_secret_store::AndroidSecretStoreBackend, planes,
};

pub(crate) type AndroidBusinessClient = Arc<
    BusinessCommandClient<
        AndroidControlPlaneTransport<tauri::Wry>,
        AndroidSecretStoreBackend<tauri::Wry>,
    >,
>;
pub(crate) type AndroidBusinessService = Arc<
    BusinessApiService<
        AndroidControlPlaneTransport<tauri::Wry>,
        AndroidSecretStoreBackend<tauri::Wry>,
    >,
>;

pub(crate) fn build(
    transport: AndroidControlPlaneTransport<tauri::Wry>,
    secrets: AndroidSecretStoreBackend<tauri::Wry>,
) -> (AndroidBusinessClient, AndroidBusinessService) {
    let client = Arc::new(BusinessCommandClient::new(transport, secrets));
    let service = Arc::new(BusinessApiService::new(Arc::clone(&client), SystemClock));
    (client, service)
}

fn map(error: BusinessServiceError) -> CommandError {
    CommandError::from_code(error.public_error_code())
}

#[tauri::command]
pub(crate) fn initialize_business(
    request: InitializeBusinessRequest,
    service: tauri::State<'_, AndroidBusinessService>,
) -> Result<BusinessInitializationResponse, CommandError> {
    request.validate()?;
    service.initialize().map_err(map)
}
#[tauri::command]
pub(crate) fn get_service_portal_url(
    request: OpenServicePortalRequest,
    service: tauri::State<'_, AndroidBusinessService>,
) -> Result<ServicePortalUrlResponse, CommandError> {
    request.validate()?;
    service
        .service_portal_url()
        .map(ServicePortalUrlResponse::new)
        .map_err(map)
}
#[tauri::command]
pub(crate) fn login(
    request: LoginCommandRequest,
    service: tauri::State<'_, AndroidBusinessService>,
) -> Result<AuthPublicResponse, CommandError> {
    service.login(request.validate()?).map_err(map)
}
#[tauri::command]
pub(crate) fn send_email_verification(
    request: SendEmailVerificationCommandRequest,
    service: tauri::State<'_, AndroidBusinessService>,
) -> Result<EmailVerificationResponse, CommandError> {
    service
        .send_email_verification(request.validate()?)
        .map_err(map)
}
#[tauri::command]
pub(crate) fn reset_password(
    request: ResetPasswordCommandRequest,
    service: tauri::State<'_, AndroidBusinessService>,
) -> Result<PasswordResetResponse, CommandError> {
    service.reset_password(request.validate()?).map_err(map)
}
#[tauri::command]
pub(crate) fn register(
    request: RegisterCommandRequest,
    service: tauri::State<'_, AndroidBusinessService>,
) -> Result<AuthPublicResponse, CommandError> {
    service.register(request.validate()?).map_err(map)
}
#[tauri::command]
pub(crate) fn get_auth_session(
    request: AuthSessionRequest,
    service: tauri::State<'_, AndroidBusinessService>,
) -> Result<AuthSessionResponse, CommandError> {
    request.validate()?;
    Ok(service.session())
}
#[tauri::command]
pub(crate) fn logout(
    request: LogoutRequest,
    service: tauri::State<'_, AndroidBusinessService>,
    planes: tauri::State<'_, planes::ManagedPlanes>,
) -> Result<AuthSessionResponse, CommandError> {
    request.validate()?;
    service.logout(planes.inner()).map_err(map)
}
#[tauri::command]
pub(crate) fn refresh_account(
    request: AccountRefreshRequest,
    service: tauri::State<'_, AndroidBusinessService>,
) -> Result<AccountResponse, CommandError> {
    request.validate()?;
    service.refresh_account().map_err(map)
}
#[tauri::command]
pub(crate) fn fetch_notices(
    request: NoticesRequest,
    service: tauri::State<'_, AndroidBusinessService>,
) -> Result<NoticesResponse, CommandError> {
    request.validate()?;
    service.fetch_notices().map_err(map)
}
#[tauri::command]
pub(crate) fn fetch_plans(
    request: PlansRequest,
    service: tauri::State<'_, AndroidBusinessService>,
) -> Result<PlansResponse, CommandError> {
    request.validate()?;
    service.fetch_plans().map_err(map)
}
#[tauri::command]
pub(crate) fn fetch_orders(
    request: OrdersRequest,
    service: tauri::State<'_, AndroidBusinessService>,
) -> Result<OrdersResponse, CommandError> {
    request.validate()?;
    service.fetch_orders().map_err(map)
}
#[tauri::command]
pub(crate) fn fetch_order_detail(
    request: OrderDetailCommandRequest,
    service: tauri::State<'_, AndroidBusinessService>,
) -> Result<OrderDetailResponse, CommandError> {
    service
        .fetch_order_detail(&request.validate()?)
        .map_err(map)
}
#[tauri::command]
pub(crate) fn fetch_payment_methods(
    request: PaymentMethodsRequest,
    service: tauri::State<'_, AndroidBusinessService>,
) -> Result<PaymentMethodsResponse, CommandError> {
    request.validate()?;
    service.fetch_payment_methods().map_err(map)
}
#[tauri::command]
pub(crate) fn checkout_order(
    request: CheckoutOrderCommandRequest,
    service: tauri::State<'_, AndroidBusinessService>,
) -> Result<PaymentPublicResponse, CommandError> {
    service.checkout_order(request.validate()?).map_err(map)
}
#[tauri::command]
pub(crate) fn cancel_order(
    request: CancelOrderCommandRequest,
    service: tauri::State<'_, AndroidBusinessService>,
) -> Result<CancelOrderResponse, CommandError> {
    service.cancel_order(&request.validate()?).map_err(map)
}
#[tauri::command]
pub(crate) fn create_order(
    request: CreateOrderCommandRequest,
    service: tauri::State<'_, AndroidBusinessService>,
) -> Result<CreateOrderResponse, CommandError> {
    service.create_order(request.validate()?).map_err(map)
}
#[tauri::command]
pub(crate) fn fetch_invitation_center(
    request: InvitationCenterRequest,
    service: tauri::State<'_, AndroidBusinessService>,
) -> Result<InvitationCenterResponse, CommandError> {
    request.validate()?;
    service.fetch_invitation_center().map_err(map)
}
#[tauri::command]
pub(crate) fn generate_invitation_code(
    request: InvitationCenterRequest,
    service: tauri::State<'_, AndroidBusinessService>,
) -> Result<InvitationCenterResponse, CommandError> {
    request.validate()?;
    service.generate_invitation_code().map_err(map)
}
#[tauri::command]
pub(crate) fn check_gift_card(
    request: GiftCardCodeCommandRequest,
    service: tauri::State<'_, AndroidBusinessService>,
) -> Result<GiftCardCheckResponse, CommandError> {
    service.check_gift_card(&request.validate()?).map_err(map)
}
#[tauri::command]
pub(crate) fn redeem_gift_card(
    request: GiftCardCodeCommandRequest,
    service: tauri::State<'_, AndroidBusinessService>,
) -> Result<GiftCardRedeemResponse, CommandError> {
    service.redeem_gift_card(&request.validate()?).map_err(map)
}
#[tauri::command]
pub(crate) fn fetch_gift_card_history(
    request: GiftCardHistoryRequest,
    service: tauri::State<'_, AndroidBusinessService>,
) -> Result<GiftCardHistoryResponse, CommandError> {
    request.validate()?;
    service.fetch_gift_card_history().map_err(map)
}
#[tauri::command]
pub(crate) fn fetch_commission_config(
    request: CommissionConfigRequest,
    service: tauri::State<'_, AndroidBusinessService>,
) -> Result<CommissionConfigResponse, CommandError> {
    request.validate()?;
    service.fetch_commission_config().map_err(map)
}
#[tauri::command]
pub(crate) fn withdraw_commission(
    request: WithdrawCommissionCommandRequest,
    service: tauri::State<'_, AndroidBusinessService>,
) -> Result<CommissionOperationResponse, CommandError> {
    let (method, account) = request.validate()?;
    service.withdraw_commission(&method, &account).map_err(map)
}
#[tauri::command]
pub(crate) fn transfer_commission(
    request: TransferCommissionCommandRequest,
    service: tauri::State<'_, AndroidBusinessService>,
) -> Result<CommissionOperationResponse, CommandError> {
    service
        .transfer_commission(request.validate()?)
        .map_err(map)
}
#[tauri::command]
pub(crate) fn fetch_active_sessions(
    request: ActiveSessionsRequest,
    service: tauri::State<'_, AndroidBusinessService>,
) -> Result<ActiveSessionsResponse, CommandError> {
    request.validate()?;
    service.fetch_active_sessions().map_err(map)
}
#[tauri::command]
pub(crate) fn remove_active_session(
    request: RemoveActiveSessionCommandRequest,
    service: tauri::State<'_, AndroidBusinessService>,
) -> Result<CommissionOperationResponse, CommandError> {
    service
        .remove_active_session(&request.validate()?)
        .map_err(map)
}
#[tauri::command]
pub(crate) fn fetch_knowledge_list(
    request: KnowledgeListCommandRequest,
    service: tauri::State<'_, AndroidBusinessService>,
) -> Result<KnowledgeListResponse, CommandError> {
    let keyword = request.validate()?;
    service
        .fetch_knowledge_list(keyword.as_deref())
        .map_err(map)
}
#[tauri::command]
pub(crate) fn fetch_knowledge_detail(
    request: KnowledgeDetailCommandRequest,
    service: tauri::State<'_, AndroidBusinessService>,
) -> Result<KnowledgeDetailResponse, CommandError> {
    service
        .fetch_knowledge_detail(&request.validate()?)
        .map_err(map)
}
#[tauri::command]
pub(crate) fn fetch_tickets(
    request: TicketsRequest,
    service: tauri::State<'_, AndroidBusinessService>,
) -> Result<TicketsResponse, CommandError> {
    request.validate()?;
    service.fetch_tickets().map_err(map)
}
#[tauri::command]
pub(crate) fn fetch_ticket_detail(
    request: TicketDetailCommandRequest,
    service: tauri::State<'_, AndroidBusinessService>,
) -> Result<TicketDetailResponse, CommandError> {
    service
        .fetch_ticket_detail(&request.validate()?)
        .map_err(map)
}
#[tauri::command]
pub(crate) fn create_ticket(
    request: CreateTicketCommandRequest,
    service: tauri::State<'_, AndroidBusinessService>,
) -> Result<TicketsResponse, CommandError> {
    service.create_ticket(request.validate()?).map_err(map)
}
#[tauri::command]
pub(crate) fn reply_ticket(
    request: ReplyTicketCommandRequest,
    service: tauri::State<'_, AndroidBusinessService>,
) -> Result<TicketDetailResponse, CommandError> {
    service.reply_ticket(request.validate()?).map_err(map)
}
#[tauri::command]
pub(crate) fn close_ticket(
    request: CloseTicketCommandRequest,
    service: tauri::State<'_, AndroidBusinessService>,
) -> Result<TicketDetailResponse, CommandError> {
    service.close_ticket(&request.validate()?).map_err(map)
}
#[tauri::command]
pub(crate) fn refresh_subscription(
    request: SubscriptionRefreshRequest,
    service: tauri::State<'_, AndroidBusinessService>,
) -> Result<SubscriptionPublicResponse, CommandError> {
    request.validate()?;
    service.refresh_subscription().map_err(map)
}
#[tauri::command]
pub(crate) fn get_subscription_snapshot(
    request: SubscriptionSnapshotRequest,
    service: tauri::State<'_, AndroidBusinessService>,
) -> Result<SubscriptionSnapshotResponse, CommandError> {
    request.validate()?;
    if service.session().status != AuthSessionStatus::Authenticated {
        return Err(CommandError::from_code(ErrorCode::Permission));
    }
    Ok(SubscriptionSnapshotResponse::new(
        service.cached_subscription(),
        None,
    ))
}

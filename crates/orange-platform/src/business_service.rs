use std::{
    collections::HashSet,
    fmt,
    sync::{
        Arc, Mutex, MutexGuard,
        atomic::{AtomicBool, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};

use orange_domain::{
    AccountResponse, AccountStatus, AuthPublicResponse, AuthSessionResponse, AuthSessionStatus,
    AuthWireResponse, BUSINESS_API_SCHEMA_VERSION, BusinessInitializationResponse,
    CancelOrderResponse, ConfigResponse, ConfigWireResponse, CreateOrderRequest,
    CreateOrderResponse, CreatePaymentRequest, CreateTicketRequest, CurrencyCode,
    ActiveSessionInfo, ActiveSessionsResponse, CommissionConfigResponse,
    KnowledgeArticleSummary, KnowledgeDetailResponse, KnowledgeGroup, KnowledgeListResponse,
    CommissionOperationResponse, EmailVerificationResponse, ErrorCode,
    GiftCardCheckResponse, GiftCardHistoryRecord,
    GiftCardHistoryResponse, GiftCardRedeemResponse, InvitationCenterResponse, InvitationCode,
    InvitationCodeStatus, InvitationStats, LoginRequest, Money, NodeLoad, NodeLoadState,
    NodeLoadsResponse, Notice, NoticesResponse, OrderDetail, OrderDetailResponse, OrderStatus,
    OrderSummary, OrdersResponse, PasswordResetResponse, PaymentMethod, PaymentMethodsResponse,
    PaymentPublicResponse, PaymentStatus, PaymentWireResponse, Plan, PlansResponse, RegisterRequest,
    ReplyTicketRequest, ResetPasswordRequest, SafeInteger, SendEmailVerificationRequest,
    SubscriptionLinkResponse, SubscriptionPublicResponse, SubscriptionStatus, SubscriptionWireResponse,
    Ticket, TicketDetail, TicketDetailResponse, TicketMessage, TicketStatus, TicketsResponse,
    UnixMillis,
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use url::Url;
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

use crate::{
    AuthenticationSecretState, BootstrapTransport, BootstrapTransportError, BusinessClientError,
    BusinessCommand, BusinessCommandClient, BusinessCommandRequest, BusinessCommandResponse,
    PlatformVpnError, SecretStoreBackend, SecretStoreError, SecretValue,
};

pub const MAX_AUTH_EMAIL_BYTES: usize = 254;
pub const MIN_AUTH_PASSWORD_BYTES: usize = 8;
pub const MAX_AUTH_PASSWORD_BYTES: usize = 128;
pub const MAX_INVITE_CODE_BYTES: usize = 64;
pub const MAX_PUBLIC_PLANS: usize = 256;
pub const MAX_PUBLIC_ORDERS: usize = 256;
pub const MAX_PUBLIC_PAYMENT_METHODS: usize = 64;
pub const MAX_PUBLIC_NOTICES: usize = 64;
pub const MAX_PUBLIC_INVITATION_CODES: usize = 256;
pub const MAX_PUBLIC_TICKETS: usize = 256;
pub const MAX_PUBLIC_TICKET_MESSAGES: usize = 256;
pub const MAX_PUBLIC_NODE_LOADS: usize = 256;

const GIB_BYTES: u64 = 1024 * 1024 * 1024;
const MAX_NOTICE_TITLE_BYTES: usize = 512;
const MAX_NOTICE_CONTENT_BYTES: usize = 64 * 1024;
const MAX_PLAN_NAME_BYTES: usize = 256;
const MAX_PLAN_DESCRIPTION_BYTES: usize = 64 * 1024;

const DEVELOPMENT_PAYMENT_URL_HOSTS: &[&str] = &["pay.orange.invalid"];
const DEVELOPMENT_SUPPORT_URL_HOSTS: &[&str] = &["support.orange.invalid"];
const DEVELOPMENT_BANNER_URL_HOSTS: &[&str] = &["assets.orange.invalid"];
const PRODUCTION_MINIMUM_SUPPORTED_VERSION: &str = "0.1.0";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProductionEnvelope<T> {
    data: T,
    #[serde(default)]
    error: Option<String>,
    message: String,
    status: String,
}

impl<T> ProductionEnvelope<T> {
    fn into_data(self) -> Result<T, BusinessServiceError> {
        if self.error.is_some()
            || self.status != "success"
            || self.message.chars().any(char::is_control)
        {
            return Err(BusinessServiceError::InvalidResponse);
        }
        Ok(self.data)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProductionStatusEnvelope {
    #[serde(default)]
    data: Option<Value>,
    #[serde(default)]
    error: Option<Value>,
    message: String,
    status: String,
}

impl ProductionStatusEnvelope {
    fn ensure_success(self) -> Result<(), BusinessServiceError> {
        if self.error.is_some()
            || self.status != "success"
            || self.message.chars().any(char::is_control)
        {
            return Err(BusinessServiceError::InvalidResponse);
        }
        let _observed_data = self.data;
        Ok(())
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProductionConfigData {
    app_description: String,
    app_url: String,
    captcha_type: String,
    email_whitelist_suffix: Vec<String>,
    is_captcha: u8,
    is_email_verify: u8,
    is_invite_force: u8,
    is_recaptcha: u8,
    logo: Option<String>,
    recaptcha_site_key: Option<String>,
    recaptcha_v3_score_threshold: f64,
    recaptcha_v3_site_key: Option<String>,
    tos_url: Option<String>,
    turnstile_site_key: Option<String>,
}

#[derive(Deserialize, Zeroize, ZeroizeOnDrop)]
#[serde(deny_unknown_fields)]
struct ProductionLoginData {
    auth_data: String,
    #[zeroize(skip)]
    is_admin: bool,
    token: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProductionAccountData {
    avatar_url: String,
    balance: u64,
    banned: bool,
    commission_balance: u64,
    commission_rate: Option<u64>,
    created_at: u64,
    discount: Option<u64>,
    email: String,
    expired_at: Option<u64>,
    last_login_at: Option<u64>,
    plan_id: Option<u64>,
    remind_expire: Option<bool>,
    remind_traffic: Option<bool>,
    telegram_id: Option<u64>,
    transfer_enable: u64,
    uuid: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProductionNoticesResponse {
    data: Vec<ProductionNoticeData>,
    total: u64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProductionNoticeData {
    content: String,
    created_at: Option<Value>,
    id: Option<Value>,
    img_url: Option<Value>,
    show: bool,
    sort: Option<Value>,
    tags: Option<Value>,
    title: String,
    updated_at: Option<Value>,
}

#[derive(Deserialize, Zeroize, ZeroizeOnDrop)]
#[serde(deny_unknown_fields)]
struct ProductionSubscriptionData {
    #[zeroize(skip)]
    d: u64,
    #[zeroize(skip)]
    device_limit: Option<u64>,
    email: String,
    #[zeroize(skip)]
    expired_at: Option<u64>,
    #[zeroize(skip)]
    next_reset_at: Option<u64>,
    #[zeroize(skip)]
    plan: Option<Map<String, Value>>,
    #[zeroize(skip)]
    plan_id: Option<u64>,
    #[zeroize(skip)]
    reset_day: Option<u64>,
    #[zeroize(skip)]
    speed_limit: Option<u64>,
    subscribe_url: String,
    token: String,
    #[zeroize(skip)]
    transfer_enable: u64,
    #[zeroize(skip)]
    u: u64,
    uuid: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProductionNodeLoadsData {
    schema_version: u16,
    generated_at: u64,
    ttl_seconds: u64,
    nodes: Vec<ProductionNodeLoad>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProductionNodeLoad {
    id: String,
    capacity_group: String,
    load: Option<f64>,
    state: NodeLoadState,
    updated_at: Option<u64>,
    selection_weight: f64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProductionPlanData {
    capacity_limit: Option<Value>,
    content: Option<String>,
    created_at: Option<u64>,
    device_limit: Option<u64>,
    group_id: Option<u64>,
    half_year_price: Option<f64>,
    id: u64,
    month_price: Option<f64>,
    name: String,
    onetime_price: Option<f64>,
    quarter_price: Option<f64>,
    renew: Option<bool>,
    reset_price: Option<f64>,
    reset_traffic_method: Option<u64>,
    sell: Option<Value>,
    show: Option<bool>,
    sort: Option<u64>,
    speed_limit: Option<u64>,
    tags: Option<Value>,
    three_year_price: Option<f64>,
    transfer_enable: Option<u64>,
    two_year_price: Option<f64>,
    updated_at: Option<u64>,
    year_price: Option<f64>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProductionOrderData {
    actual_commission_balance: Option<Value>,
    balance_amount: Option<Value>,
    callback_no: Option<Value>,
    commission_balance: Option<Value>,
    commission_status: Option<Value>,
    coupon_code: Option<Value>,
    coupon_id: Option<Value>,
    created_at: Option<u64>,
    discount_amount: Option<Value>,
    handling_amount: Option<Value>,
    id: Option<u64>,
    invite_user_id: Option<Value>,
    paid_at: Option<Value>,
    payment: Option<Value>,
    payment_id: Option<Value>,
    period: Option<String>,
    plan: Option<Map<String, Value>>,
    plan_id: Option<u64>,
    refund_amount: Option<Value>,
    site_id: Option<Value>,
    status: Option<u64>,
    surplus_amount: Option<Value>,
    surplus_credit: Option<Value>,
    surplus_order_ids: Option<Value>,
    surplus_orders: Option<Value>,
    tixianstatus: Option<Value>,
    total_amount: Option<u64>,
    trade_no: Option<String>,
    try_out_plan_id: Option<u64>,
    r#type: Option<u64>,
    updated_at: Option<u64>,
    user_id: Option<u64>,
}

#[derive(Serialize)]
struct ProductionCreateOrderRequest<'a> {
    coupon_code: &'a str,
    period: &'a str,
    plan_id: u64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProductionPaymentMethodData {
    handling_fee_fixed: Option<Value>,
    handling_fee_percent: Option<Value>,
    icon: Option<Value>,
    id: Option<u64>,
    name: Option<String>,
    payment: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProductionInvitationCenterData {
    codes: Option<Vec<ProductionInvitationCodeData>>,
    stat: Option<Vec<u64>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProductionInvitationCodeData {
    code: String,
    created_at: Option<u64>,
    pv: Option<u64>,
    status: Option<bool>,
    updated_at: Option<u64>,
    user_id: Option<u64>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProductionTicketData {
    created_at: Option<u64>,
    id: Option<u64>,
    level: Option<u64>,
    message: Option<Value>,
    reply_status: Option<u64>,
    status: Option<u64>,
    subject: Option<String>,
    updated_at: Option<u64>,
    user_id: Option<u64>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProductionTicketDetailData {
    created_at: Option<u64>,
    id: Option<u64>,
    level: Option<u64>,
    message: Option<Vec<ProductionTicketMessageData>>,
    reply_status: Option<u64>,
    status: Option<u64>,
    subject: Option<String>,
    updated_at: Option<u64>,
    user_id: Option<u64>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProductionTicketMessageData {
    created_at: Option<u64>,
    id: Option<u64>,
    is_me: Option<bool>,
    message: Option<String>,
    photo: Option<Value>,
    #[serde(rename = "profilePic", alias = "profile_pic")]
    profile_pic: Option<Value>,
    ticket_id: Option<u64>,
    updated_at: Option<u64>,
}

#[derive(Serialize)]
struct ProductionCreateTicketRequest<'a> {
    subject: &'a str,
    level: u8,
    message: &'a str,
}

#[derive(Serialize)]
struct ProductionReplyTicketRequest<'a> {
    id: &'a str,
    message: &'a str,
}

#[derive(Serialize)]
struct ProductionCloseTicketRequest {
    id: u64,
}

#[derive(Serialize)]
struct ProductionGiftCardCodeRequest<'a> {
    code: &'a str,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProductionGiftCardCheckData {
    can_redeem: bool,
    code_info: ProductionGiftCardCodeInfo,
    reason: Option<String>,
    reward_preview: Option<Map<String, Value>>,
}

#[derive(Deserialize)]
struct ProductionGiftCardCodeInfo {
    template: ProductionGiftCardTemplateName,
}

#[derive(Deserialize)]
struct ProductionGiftCardTemplateName {
    name: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProductionGiftCardRedeemData {
    invite_rewards: Option<Value>,
    message: String,
    rewards: Option<Map<String, Value>>,
    template_name: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProductionGiftCardHistoryEnvelope {
    data: Vec<ProductionGiftCardHistoryRecord>,
    pagination: ProductionGiftCardPagination,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProductionGiftCardHistoryRecord {
    code: String,
    // The server emits a Unix timestamp (integer), not a formatted string.
    created_at: Option<Value>,
    id: u64,
    invite_rewards: Option<Value>,
    multiplier_applied: Option<Value>,
    rewards_given: Option<Value>,
    template_name: Option<String>,
    template_type: Option<String>,
    template_type_name: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProductionGiftCardPagination {
    current_page: u64,
    last_page: u64,
    per_page: u64,
    total: u64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProductionCommissionConfigData {
    commission_distribution_enable: Option<Value>,
    commission_distribution_l1: Option<Value>,
    commission_distribution_l2: Option<Value>,
    commission_distribution_l3: Option<Value>,
    currency: Option<String>,
    currency_symbol: Option<String>,
    is_telegram: Option<Value>,
    stripe_pk: Option<String>,
    telegram_discuss_link: Option<String>,
    withdraw_close: Option<Value>,
    withdraw_methods: Option<Vec<String>>,
}

#[derive(Serialize)]
struct ProductionWithdrawCommissionRequest<'a> {
    withdraw_account: &'a str,
    withdraw_method: &'a str,
}

#[derive(Serialize)]
struct ProductionTransferCommissionRequest {
    transfer_amount: u64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProductionActiveSessionData {
    abilities: Value,
    created_at: Option<String>,
    expires_at: Option<String>,
    id: u64,
    last_used_at: Option<String>,
    name: Option<String>,
    // Sanctum never serializes the hashed token; the key is absent entirely.
    #[serde(default)]
    token: Option<String>,
    tokenable_id: u64,
    tokenable_type: String,
    updated_at: Option<String>,
}

#[derive(Serialize)]
struct ProductionRemoveActiveSessionRequest<'a> {
    session_id: &'a str,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProductionTelegramBotData {
    username: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProductionKnowledgeArticleData {
    body: String,
    category: String,
    id: u64,
    title: String,
    updated_at: u64,
}

#[derive(Serialize)]
struct ProductionCheckoutOrderRequest<'a> {
    trade_no: &'a str,
    method: u64,
}

#[derive(Serialize)]
struct ProductionLoginRequest<'a> {
    email: &'a str,
    password: &'a str,
}

#[derive(Serialize)]
struct ProductionRegisterRequest<'a> {
    email: &'a str,
    password: &'a str,
    #[serde(rename = "captchaData")]
    captcha_data: &'static str,
    email_code: &'a str,
    invite_code: &'a str,
}

#[derive(Serialize)]
struct ProductionResetPasswordRequest<'a> {
    email: &'a str,
    password: &'a str,
    email_code: &'a str,
}

pub trait BusinessClock: Send + Sync {
    fn now_unix_ms(&self) -> u64;
}

pub trait LogoutDataPlane: Send + Sync {
    /// Stop every user Data Plane resource before credentials are deleted.
    fn stop_for_logout(&self) -> Result<(), PlatformVpnError>;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl BusinessClock for SystemClock {
    fn now_unix_ms(&self) -> u64 {
        let millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        u64::try_from(millis).unwrap_or(u64::MAX)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BusinessServiceError {
    InvalidEmail,
    InvalidPassword,
    EmailVerificationRequired,
    InvalidEmailVerificationCode,
    InviteRequired,
    InvalidInviteCode,
    InvalidPlan,
    NotInitialized,
    SubmissionInProgress,
    InvalidContentType,
    InvalidResponse,
    RejectedConfigUrl,
    ExpiredCredentials,
    DataPlane(PlatformVpnError),
    Client(BusinessClientError),
}

impl BusinessServiceError {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidEmail => "business-invalid-email",
            Self::InvalidPassword => "business-invalid-password",
            Self::EmailVerificationRequired => "business-email-verification-required",
            Self::InvalidEmailVerificationCode => "business-invalid-email-verification-code",
            Self::InviteRequired => "business-invite-required",
            Self::InvalidInviteCode => "business-invalid-invite-code",
            Self::InvalidPlan => "business-invalid-plan",
            Self::NotInitialized => "business-not-initialized",
            Self::SubmissionInProgress => "business-submission-in-progress",
            Self::InvalidContentType => "business-invalid-content-type",
            Self::InvalidResponse => "business-invalid-response",
            Self::RejectedConfigUrl => "business-config-url-rejected",
            Self::ExpiredCredentials => "business-expired-credentials",
            Self::DataPlane(error) => error.as_str(),
            Self::Client(error) => error.as_str(),
        }
    }

    pub const fn public_error_code(self) -> ErrorCode {
        match self {
            Self::InvalidEmail
            | Self::InvalidPassword
            | Self::EmailVerificationRequired
            | Self::InvalidEmailVerificationCode
            | Self::InviteRequired
            | Self::InvalidInviteCode
            | Self::InvalidPlan => ErrorCode::Validation,
            Self::NotInitialized | Self::RejectedConfigUrl => ErrorCode::Bootstrap,
            Self::SubmissionInProgress => ErrorCode::Cancelled,
            Self::InvalidContentType | Self::InvalidResponse | Self::ExpiredCredentials => {
                ErrorCode::Service
            }
            Self::DataPlane(error) => match error {
                PlatformVpnError::InvalidConfiguration | PlatformVpnError::ProtocolViolation => {
                    ErrorCode::Internal
                }
                PlatformVpnError::PermissionDenied => ErrorCode::Permission,
                PlatformVpnError::Timeout => ErrorCode::Timeout,
                PlatformVpnError::OperationInProgress => ErrorCode::Cancelled,
                PlatformVpnError::Crashed
                | PlatformVpnError::Unavailable
                | PlatformVpnError::CleanupFailed => ErrorCode::Service,
            },
            Self::Client(error) => map_client_error(error),
        }
    }
}

impl fmt::Display for BusinessServiceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl std::error::Error for BusinessServiceError {}

impl From<BusinessClientError> for BusinessServiceError {
    fn from(error: BusinessClientError) -> Self {
        Self::Client(error)
    }
}

impl From<PlatformVpnError> for BusinessServiceError {
    fn from(error: PlatformVpnError) -> Self {
        Self::DataPlane(error)
    }
}

#[derive(Clone)]
struct BusinessState {
    config: Option<ConfigResponse>,
    service_portal_url: Option<String>,
    production_backend: bool,
    session: AuthSessionResponse,
    subscription: Option<SubscriptionPublicResponse>,
}

impl Default for BusinessState {
    fn default() -> Self {
        Self {
            config: None,
            service_portal_url: None,
            production_backend: false,
            session: AuthSessionResponse::signed_out(),
            subscription: None,
        }
    }
}

pub struct BusinessApiService<T, B, C = SystemClock> {
    client: Arc<BusinessCommandClient<T, B>>,
    clock: C,
    state: Mutex<BusinessState>,
    operation_in_flight: AtomicBool,
}

impl<T, B, C> BusinessApiService<T, B, C>
where
    T: BootstrapTransport,
    B: SecretStoreBackend,
    C: BusinessClock,
{
    pub fn new(client: Arc<BusinessCommandClient<T, B>>, clock: C) -> Self {
        Self {
            client,
            clock,
            state: Mutex::new(BusinessState::default()),
            operation_in_flight: AtomicBool::new(false),
        }
    }

    pub fn initialize(&self) -> Result<BusinessInitializationResponse, BusinessServiceError> {
        if let Err(error) = self.client.wait_until_ready() {
            self.reconcile_unverified_session()?;
            return Err(error.into());
        }

        let (config, production_backend, service_portal_url) = match self.fetch_config() {
            Ok(config) => config,
            Err(error) => {
                self.reconcile_unverified_session()?;
                return Err(error);
            }
        };
        let session = self.validate_stored_session()?;
        let response = BusinessInitializationResponse {
            schema_version: BUSINESS_API_SCHEMA_VERSION,
            config: config.clone(),
            session: session.clone(),
        };
        let mut state = lock(&self.state);
        state.config = Some(config);
        state.service_portal_url = Some(service_portal_url);
        state.production_backend = production_backend;
        if session.status == AuthSessionStatus::SignedOut {
            state.subscription = None;
        }
        state.session = session;
        Ok(response)
    }

    pub fn login(&self, request: LoginRequest) -> Result<AuthPublicResponse, BusinessServiceError> {
        validate_email(&request.email)?;
        validate_password(&request.password)?;
        self.require_config()?;
        let _operation = self.acquire_operation()?;
        self.authenticate(
            BusinessCommand::Login,
            &ProductionLoginRequest {
                email: &request.email,
                password: &request.password,
            },
        )
    }

    pub fn send_email_verification(
        &self,
        request: SendEmailVerificationRequest,
    ) -> Result<EmailVerificationResponse, BusinessServiceError> {
        validate_email(&request.email)?;
        self.require_config()?;
        let _operation = self.acquire_operation()?;
        let command = BusinessCommand::SendEmailVerification;
        let request = BusinessCommandRequest::post_with_query_parameters(
            command,
            &[("email", &request.email), ("recaptcha_data", "")],
        )?;
        let response = self.client.execute(request)?;
        decode_status_response(response)?;
        Ok(EmailVerificationResponse {
            schema_version: BUSINESS_API_SCHEMA_VERSION,
            sent: true,
        })
    }

    pub fn reset_password(
        &self,
        request: ResetPasswordRequest,
    ) -> Result<PasswordResetResponse, BusinessServiceError> {
        validate_email(&request.email)?;
        validate_password(&request.password)?;
        if !valid_email_verification_code(&request.email_code) {
            return Err(BusinessServiceError::InvalidEmailVerificationCode);
        }
        self.require_config()?;
        let _operation = self.acquire_operation()?;
        let command = BusinessCommand::ResetPassword;
        let request = BusinessCommandRequest::json(
            command,
            &ProductionResetPasswordRequest {
                email: &request.email,
                password: &request.password,
                email_code: &request.email_code,
            },
        )?;
        let response = self.client.execute(request)?;
        decode_status_response(response)?;
        Ok(PasswordResetResponse {
            schema_version: BUSINESS_API_SCHEMA_VERSION,
            succeeded: true,
        })
    }

    pub fn register(
        &self,
        request: RegisterRequest,
    ) -> Result<AuthPublicResponse, BusinessServiceError> {
        validate_email(&request.email)?;
        validate_password(&request.password)?;
        if let Some(invite_code) = request.invite_code.as_deref() {
            validate_invite_code(invite_code)?;
        }
        if request
            .email_code
            .as_deref()
            .is_some_and(|value| !valid_email_verification_code(value))
        {
            return Err(BusinessServiceError::InvalidEmailVerificationCode);
        }
        let state = lock(&self.state);
        let config = state
            .config
            .as_ref()
            .ok_or(BusinessServiceError::NotInitialized)?;
        let registration_requires_invite = config.registration_requires_invite;
        let registration_requires_email_verification =
            config.registration_requires_email_verification;
        let production_backend = state.production_backend;
        drop(state);
        if registration_requires_email_verification && request.email_code.is_none() {
            return Err(BusinessServiceError::EmailVerificationRequired);
        }
        if registration_requires_invite && request.invite_code.is_none() {
            return Err(BusinessServiceError::InviteRequired);
        }
        let _operation = self.acquire_operation()?;
        if production_backend {
            self.authenticate(
                BusinessCommand::Register,
                &ProductionRegisterRequest {
                    email: &request.email,
                    password: &request.password,
                    captcha_data: "",
                    email_code: request.email_code.as_deref().unwrap_or(""),
                    invite_code: request.invite_code.as_deref().unwrap_or(""),
                },
            )
        } else {
            self.authenticate(BusinessCommand::Register, &request)
        }
    }

    pub fn session(&self) -> AuthSessionResponse {
        lock(&self.state).session.clone()
    }

    pub fn service_portal_url(&self) -> Result<String, BusinessServiceError> {
        lock(&self.state)
            .service_portal_url
            .clone()
            .ok_or(BusinessServiceError::NotInitialized)
    }

    pub fn cached_subscription(&self) -> Option<SubscriptionPublicResponse> {
        let now_unix_ms = self.clock.now_unix_ms();
        lock(&self.state)
            .subscription
            .clone()
            .map(|mut subscription| {
                subscription.status = subscription.effective_status(now_unix_ms);
                subscription
            })
    }

    pub fn subscription_allows_new_data_plane_start(&self) -> bool {
        let now_unix_ms = self.clock.now_unix_ms();
        lock(&self.state)
            .subscription
            .as_ref()
            .is_some_and(|subscription| subscription.allows_new_data_plane_start(now_unix_ms))
    }

    pub fn logout<D: LogoutDataPlane + ?Sized>(
        &self,
        data_plane: &D,
    ) -> Result<AuthSessionResponse, BusinessServiceError> {
        let _operation = self.acquire_operation()?;
        data_plane.stop_for_logout()?;
        self.client.clear_authentication()?;

        let mut state = lock(&self.state);
        state.session = AuthSessionResponse::signed_out();
        state.subscription = None;
        Ok(state.session.clone())
    }

    pub fn refresh_account(&self) -> Result<AccountResponse, BusinessServiceError> {
        self.require_authenticated()?;
        let _operation = self.acquire_operation()?;
        let response = self.execute_authenticated(BusinessCommand::Account)?;
        let account = decode_account_response(response)?;
        lock(&self.state).session = AuthSessionResponse::authenticated(account.user.clone());
        Ok(account)
    }

    pub fn fetch_notices(&self) -> Result<NoticesResponse, BusinessServiceError> {
        self.require_authenticated()?;
        let _operation = self.acquire_operation()?;
        let response = self.execute_authenticated(BusinessCommand::Notices)?;
        decode_notices_response(response)
    }

    pub fn refresh_subscription(&self) -> Result<SubscriptionPublicResponse, BusinessServiceError> {
        self.require_authenticated()?;
        let _operation = self.acquire_operation()?;
        let response = self.execute_authenticated(BusinessCommand::Subscription)?;
        let wire = decode_subscription_response(response)?;
        self.apply_subscription_wire(wire)
    }

    pub fn fetch_node_loads(&self) -> Result<NodeLoadsResponse, BusinessServiceError> {
        self.require_authenticated()?;
        let _operation = self.acquire_operation()?;
        let response = self.execute_authenticated(BusinessCommand::NodeLoads)?;
        decode_node_loads_response(response)
    }

    pub fn fetch_subscription_link(
        &self,
    ) -> Result<SubscriptionLinkResponse, BusinessServiceError> {
        self.require_authenticated()?;
        let _operation = self.acquire_operation()?;
        self.fetch_subscription_link_response()
    }

    pub fn reset_subscription_link(
        &self,
    ) -> Result<SubscriptionLinkResponse, BusinessServiceError> {
        self.require_authenticated()?;
        let _operation = self.acquire_operation()?;
        let response = self.execute_authenticated(BusinessCommand::ResetSubscription)?;
        decode_status_response(response)?;
        self.fetch_subscription_link_response()
    }

    fn fetch_subscription_link_response(
        &self,
    ) -> Result<SubscriptionLinkResponse, BusinessServiceError> {
        let response = self.execute_authenticated(BusinessCommand::Subscription)?;
        let wire = decode_subscription_response(response)?;
        let subscribe_url = wire.subscription_credential.clone();
        self.apply_subscription_wire(wire)?;
        Ok(SubscriptionLinkResponse {
            schema_version: BUSINESS_API_SCHEMA_VERSION,
            subscribe_url,
        })
    }

    fn apply_subscription_wire(
        &self,
        mut wire: SubscriptionWireResponse,
    ) -> Result<SubscriptionPublicResponse, BusinessServiceError> {
        let credential = std::mem::take(&mut wire.subscription_credential);
        let mut public = SubscriptionPublicResponse {
            schema_version: wire.schema_version,
            status: wire.status,
            plan_id: std::mem::take(&mut wire.plan_id),
            plan_name: std::mem::take(&mut wire.plan_name),
            expires_at_unix_ms: wire.expires_at_unix_ms,
            used_bytes: wire.used_bytes,
            upload_bytes: wire.upload_bytes,
            download_bytes: wire.download_bytes,
            total_bytes: wire.total_bytes,
        };
        public.status = public.effective_status(self.clock.now_unix_ms());

        if matches!(
            public.status,
            SubscriptionStatus::Trial | SubscriptionStatus::Active
        ) {
            let mut credential =
                SecretValue::new(credential.into_bytes()).map_err(BusinessClientError::from)?;
            self.client
                .replace_subscription_credential(&mut credential)?;
        } else {
            let _credential = Zeroizing::new(credential);
            self.client.clear_subscription_credential()?;
        }

        lock(&self.state).subscription = Some(public.clone());
        Ok(public)
    }

    pub fn fetch_plans(&self) -> Result<PlansResponse, BusinessServiceError> {
        self.require_authenticated()?;
        let _operation = self.acquire_operation()?;
        let response = self.execute_authenticated(BusinessCommand::Plans)?;
        decode_plans_response(response)
    }

    pub fn fetch_orders(&self) -> Result<OrdersResponse, BusinessServiceError> {
        self.require_authenticated()?;
        let _operation = self.acquire_operation()?;
        let response = self.execute_authenticated(BusinessCommand::Orders)?;
        decode_orders_response(response)
    }

    pub fn fetch_order_detail(
        &self,
        order_id: &str,
    ) -> Result<OrderDetailResponse, BusinessServiceError> {
        self.require_authenticated()?;
        if !valid_order_id(order_id) {
            return Err(BusinessServiceError::InvalidResponse);
        }
        let _operation = self.acquire_operation()?;
        let request = BusinessCommandRequest::with_query_parameter(
            BusinessCommand::OrderDetail,
            "trade_no",
            order_id,
        )?;
        let response = self.execute_authenticated_request(request)?;
        decode_order_detail_response(response)
    }

    pub fn fetch_payment_methods(&self) -> Result<PaymentMethodsResponse, BusinessServiceError> {
        self.require_authenticated()?;
        let _operation = self.acquire_operation()?;
        let response = self.execute_authenticated(BusinessCommand::PaymentMethods)?;
        decode_payment_methods_response(response)
    }

    pub fn checkout_order(
        &self,
        request: CreatePaymentRequest,
    ) -> Result<PaymentPublicResponse, BusinessServiceError> {
        self.require_authenticated()?;
        if !valid_order_id(&request.order_id) {
            return Err(BusinessServiceError::InvalidResponse);
        }
        let payment_method = request
            .payment_method
            .parse::<u64>()
            .ok()
            .filter(|value| *value > 0)
            .ok_or(BusinessServiceError::InvalidResponse)?;
        let _operation = self.acquire_operation()?;
        let response = self.execute_authenticated_json(
            BusinessCommand::CheckoutOrder,
            &ProductionCheckoutOrderRequest {
                trade_no: &request.order_id,
                method: payment_method,
            },
        )?;
        let wire = decode_checkout_order_response(response, &request.order_id)?;
        if wire.order_id != request.order_id || wire.status != PaymentStatus::Ready {
            return Err(BusinessServiceError::InvalidResponse);
        }
        Ok(PaymentPublicResponse {
            schema_version: BUSINESS_API_SCHEMA_VERSION,
            order_id: request.order_id,
            status: wire.status,
            available: true,
            qr_code: wire.with_qr_code(|value| value.map(str::to_owned)),
            expires_at_unix_ms: wire.expires_at_unix_ms,
        })
    }

    pub fn cancel_order(
        &self,
        order_id: &str,
    ) -> Result<CancelOrderResponse, BusinessServiceError> {
        self.require_authenticated()?;
        if !valid_order_id(order_id) {
            return Err(BusinessServiceError::InvalidResponse);
        }
        let _operation = self.acquire_operation()?;
        let request = BusinessCommandRequest::post_with_query_parameter(
            BusinessCommand::CancelOrder,
            "trade_no",
            order_id,
        )?;
        let response = self.execute_authenticated_request(request)?;
        decode_cancel_order_response(response, order_id)
    }

    pub fn create_order(
        &self,
        request: CreateOrderRequest,
    ) -> Result<CreateOrderResponse, BusinessServiceError> {
        self.require_authenticated()?;
        let (plan_id, period) = parse_production_plan_selection(&request.plan_id)?;
        let _operation = self.acquire_operation()?;
        let response = self.execute_authenticated_json(
            BusinessCommand::CreateOrder,
            &ProductionCreateOrderRequest {
                coupon_code: request.coupon_code.as_deref().unwrap_or(""),
                period,
                plan_id,
            },
        )?;
        decode_create_order_response(response)
    }

    pub fn fetch_invitation_center(
        &self,
    ) -> Result<InvitationCenterResponse, BusinessServiceError> {
        self.require_authenticated()?;
        let _operation = self.acquire_operation()?;
        let response = self.execute_authenticated(BusinessCommand::InvitationCenter)?;
        decode_invitation_center_response(response)
    }

    pub fn generate_invitation_code(
        &self,
    ) -> Result<InvitationCenterResponse, BusinessServiceError> {
        self.require_authenticated()?;
        let _operation = self.acquire_operation()?;
        let response = self.execute_authenticated(BusinessCommand::GenerateInvitationCode)?;
        decode_status_response(response)?;
        let response = self.execute_authenticated(BusinessCommand::InvitationCenter)?;
        decode_invitation_center_response(response)
    }

    pub fn check_gift_card(&self, code: &str) -> Result<GiftCardCheckResponse, BusinessServiceError> {
        self.require_authenticated()?;
        let _operation = self.acquire_operation()?;
        let response = self.execute_authenticated_json(
            BusinessCommand::GiftCardCheck,
            &ProductionGiftCardCodeRequest { code },
        )?;
        decode_gift_card_check_response(response)
    }

    pub fn redeem_gift_card(
        &self,
        code: &str,
    ) -> Result<GiftCardRedeemResponse, BusinessServiceError> {
        self.require_authenticated()?;
        let _operation = self.acquire_operation()?;
        let response = self.execute_authenticated_json(
            BusinessCommand::GiftCardRedeem,
            &ProductionGiftCardCodeRequest { code },
        )?;
        decode_gift_card_redeem_response(response)
    }

    pub fn fetch_gift_card_history(
        &self,
    ) -> Result<GiftCardHistoryResponse, BusinessServiceError> {
        self.require_authenticated()?;
        let _operation = self.acquire_operation()?;
        let response = self.execute_authenticated(BusinessCommand::GiftCardHistory)?;
        decode_gift_card_history_response(response)
    }

    pub fn fetch_commission_config(
        &self,
    ) -> Result<CommissionConfigResponse, BusinessServiceError> {
        self.require_authenticated()?;
        let _operation = self.acquire_operation()?;
        let response = self.execute_authenticated(BusinessCommand::CommissionConfig)?;
        decode_commission_config_response(response)
    }

    pub fn withdraw_commission(
        &self,
        method: &str,
        account: &str,
    ) -> Result<CommissionOperationResponse, BusinessServiceError> {
        self.require_authenticated()?;
        let _operation = self.acquire_operation()?;
        let response = self.execute_authenticated_json(
            BusinessCommand::WithdrawCommission,
            &ProductionWithdrawCommissionRequest {
                withdraw_account: account,
                withdraw_method: method,
            },
        )?;
        decode_status_response(response)?;
        Ok(CommissionOperationResponse {
            schema_version: BUSINESS_API_SCHEMA_VERSION,
            succeeded: true,
        })
    }

    pub fn transfer_commission(
        &self,
        amount_minor: u64,
    ) -> Result<CommissionOperationResponse, BusinessServiceError> {
        self.require_authenticated()?;
        let _operation = self.acquire_operation()?;
        let response = self.execute_authenticated_json(
            BusinessCommand::TransferCommission,
            &ProductionTransferCommissionRequest {
                transfer_amount: amount_minor,
            },
        )?;
        decode_status_response(response)?;
        Ok(CommissionOperationResponse {
            schema_version: BUSINESS_API_SCHEMA_VERSION,
            succeeded: true,
        })
    }

    pub fn fetch_active_sessions(
        &self,
    ) -> Result<ActiveSessionsResponse, BusinessServiceError> {
        self.require_authenticated()?;
        let _operation = self.acquire_operation()?;
        let response = self.execute_authenticated(BusinessCommand::ActiveSessions)?;
        decode_active_sessions_response(response)
    }

    pub fn remove_active_session(
        &self,
        session_id: &str,
    ) -> Result<CommissionOperationResponse, BusinessServiceError> {
        self.require_authenticated()?;
        let _operation = self.acquire_operation()?;
        let response = self.execute_authenticated_json(
            BusinessCommand::RemoveActiveSession,
            &ProductionRemoveActiveSessionRequest { session_id },
        )?;
        decode_status_response(response)?;
        Ok(CommissionOperationResponse {
            schema_version: BUSINESS_API_SCHEMA_VERSION,
            succeeded: true,
        })
    }

    pub fn telegram_bot_username(&self) -> Result<Option<String>, BusinessServiceError> {
        self.require_authenticated()?;
        let _operation = self.acquire_operation()?;
        let response = self.execute_authenticated(BusinessCommand::TelegramBotInfo)?;
        let body = take_json_body(response)?;
        if let Ok(envelope) =
            serde_json::from_slice::<ProductionEnvelope<ProductionTelegramBotData>>(&body)
        {
            let data = envelope.into_data()?;
            let username = data
                .username
                .map(|value| value.trim().trim_start_matches('@').to_owned())
                .filter(|value| !value.is_empty());
            if username.as_ref().is_some_and(|value| {
                value.len() > 64
                    || !value
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
            }) {
                return Err(BusinessServiceError::InvalidResponse);
            }
            return Ok(username);
        }
        serde_json::from_slice(&body).map_err(|_| BusinessServiceError::InvalidResponse)
    }

    pub fn fetch_knowledge_list(
        &self,
        keyword: Option<&str>,
    ) -> Result<KnowledgeListResponse, BusinessServiceError> {
        self.require_authenticated()?;
        let _operation = self.acquire_operation()?;
        let response = match keyword {
            Some(keyword) => self.execute_authenticated_request(
                BusinessCommandRequest::with_query_parameter(
                    BusinessCommand::KnowledgeFetch,
                    "keyword",
                    keyword,
                )?,
            )?,
            None => self.execute_authenticated(BusinessCommand::KnowledgeFetch)?,
        };
        decode_knowledge_list_response(response)
    }

    pub fn fetch_knowledge_detail(
        &self,
        article_id: &str,
    ) -> Result<KnowledgeDetailResponse, BusinessServiceError> {
        self.require_authenticated()?;
        let _operation = self.acquire_operation()?;
        let response = self.execute_authenticated_request(
            BusinessCommandRequest::with_query_parameter(
                BusinessCommand::KnowledgeFetch,
                "id",
                article_id,
            )?,
        )?;
        decode_knowledge_detail_response(response)
    }

    pub fn fetch_tickets(&self) -> Result<TicketsResponse, BusinessServiceError> {
        self.require_authenticated()?;
        let _operation = self.acquire_operation()?;
        let response = self.execute_authenticated(BusinessCommand::Tickets)?;
        decode_tickets_response(response)
    }

    pub fn fetch_ticket_detail(
        &self,
        ticket_id: &str,
    ) -> Result<TicketDetailResponse, BusinessServiceError> {
        self.require_authenticated()?;
        if !valid_ticket_id(ticket_id) {
            return Err(BusinessServiceError::InvalidResponse);
        }
        let _operation = self.acquire_operation()?;
        self.fetch_ticket_detail_response(ticket_id)
    }

    fn fetch_ticket_detail_response(
        &self,
        ticket_id: &str,
    ) -> Result<TicketDetailResponse, BusinessServiceError> {
        let request = BusinessCommandRequest::with_query_parameter(
            BusinessCommand::TicketDetail,
            "id",
            ticket_id,
        )?;
        let response = self.execute_authenticated_request(request)?;
        decode_ticket_detail_response(response, ticket_id)
    }

    pub fn create_ticket(
        &self,
        request: CreateTicketRequest,
    ) -> Result<TicketsResponse, BusinessServiceError> {
        self.require_authenticated()?;
        if !valid_ticket_subject(&request.subject) || !valid_ticket_message(&request.message) {
            return Err(BusinessServiceError::InvalidResponse);
        }
        let _operation = self.acquire_operation()?;
        let response = self.execute_authenticated_json(
            BusinessCommand::CreateTicket,
            &ProductionCreateTicketRequest {
                subject: &request.subject,
                level: 0,
                message: &request.message,
            },
        )?;
        decode_status_response(response)?;
        let response = self.execute_authenticated(BusinessCommand::Tickets)?;
        decode_tickets_response(response)
    }

    pub fn reply_ticket(
        &self,
        request: ReplyTicketRequest,
    ) -> Result<TicketDetailResponse, BusinessServiceError> {
        self.require_authenticated()?;
        if !valid_ticket_id(&request.ticket_id) || !valid_ticket_message(&request.message) {
            return Err(BusinessServiceError::InvalidResponse);
        }
        let _operation = self.acquire_operation()?;
        let response = self.execute_authenticated_json(
            BusinessCommand::ReplyTicket,
            &ProductionReplyTicketRequest {
                id: &request.ticket_id,
                message: &request.message,
            },
        )?;
        decode_status_response(response)?;
        self.fetch_ticket_detail_response(&request.ticket_id)
    }

    pub fn close_ticket(
        &self,
        ticket_id: &str,
    ) -> Result<TicketDetailResponse, BusinessServiceError> {
        self.require_authenticated()?;
        let id = ticket_id
            .parse::<u64>()
            .ok()
            .filter(|value| *value > 0)
            .ok_or(BusinessServiceError::InvalidResponse)?;
        let _operation = self.acquire_operation()?;
        let response = self.execute_authenticated_json(
            BusinessCommand::CloseTicket,
            &ProductionCloseTicketRequest { id },
        )?;
        decode_status_response(response)?;
        self.fetch_ticket_detail_response(ticket_id)
    }

    fn fetch_config(&self) -> Result<(ConfigResponse, bool, String), BusinessServiceError> {
        let request = BusinessCommandRequest::without_body(BusinessCommand::Config)?;
        let response = self.client.execute(request)?;
        match decode_config_response(response)? {
            DecodedConfig::Development(wire) => {
                self.validate_config_urls(&wire)?;
                Ok((
                    ConfigResponse {
                        schema_version: wire.schema_version,
                        minimum_supported_version: wire.minimum_supported_version.clone(),
                        maintenance: wire.maintenance,
                        notice: wire.notice.clone(),
                        registration_requires_invite: wire.registration_requires_invite,
                        registration_requires_email_verification: wire
                            .registration_requires_email_verification,
                        // The development wire contract carries no whitelist;
                        // an empty list means "any suffix is accepted".
                        email_suffix_whitelist: Vec::new(),
                    },
                    false,
                    wire.support_url.clone(),
                ))
            }
            DecodedConfig::Production(config) => {
                self.validate_production_app_url(&config.app_url)?;
                Ok((
                    ConfigResponse {
                        schema_version: BUSINESS_API_SCHEMA_VERSION,
                        minimum_supported_version: PRODUCTION_MINIMUM_SUPPORTED_VERSION.to_owned(),
                        maintenance: false,
                        notice: (!config.app_description.trim().is_empty())
                            .then_some(config.app_description),
                        registration_requires_invite: config.is_invite_force != 0,
                        registration_requires_email_verification: config.is_email_verify != 0,
                        email_suffix_whitelist: normalize_email_suffixes(
                            config.email_whitelist_suffix,
                        ),
                    },
                    true,
                    config.app_url,
                ))
            }
        }
    }

    fn validate_stored_session(&self) -> Result<AuthSessionResponse, BusinessServiceError> {
        match self.client.authentication_state()? {
            AuthenticationSecretState::Empty => Ok(AuthSessionResponse::signed_out()),
            AuthenticationSecretState::Partial => {
                self.client.clear_authentication()?;
                Ok(AuthSessionResponse::signed_out())
            }
            AuthenticationSecretState::Complete => {
                let request = BusinessCommandRequest::without_body(BusinessCommand::Account)?;
                match self.client.execute(request) {
                    Ok(response) => {
                        let account = decode_account_response(response)?;
                        Ok(AuthSessionResponse::authenticated(account.user))
                    }
                    Err(BusinessClientError::Unauthorized) => Ok(AuthSessionResponse::signed_out()),
                    Err(error) if is_offline_client_error(error) => {
                        let previous_user = lock(&self.state).session.user.clone();
                        Ok(AuthSessionResponse::unverified(previous_user))
                    }
                    Err(error) => Err(error.into()),
                }
            }
        }
    }

    fn validate_config_urls(
        &self,
        config: &ConfigWireResponse,
    ) -> Result<(), BusinessServiceError> {
        let api_url = parse_config_url(&config.api_base_url, true)?;
        let api_host = api_url
            .host_str()
            .ok_or(BusinessServiceError::RejectedConfigUrl)?;
        if !self.client.is_control_api_host_allowed(api_host)? {
            return Err(BusinessServiceError::RejectedConfigUrl);
        }
        validate_config_url(
            &config.payment_base_url,
            DEVELOPMENT_PAYMENT_URL_HOSTS,
            true,
        )?;
        validate_config_url(&config.support_url, DEVELOPMENT_SUPPORT_URL_HOSTS, false)?;
        if let Some(banner_url) = config.banner_url.as_deref() {
            validate_config_url(banner_url, DEVELOPMENT_BANNER_URL_HOSTS, false)?;
        }
        Ok(())
    }

    fn validate_production_app_url(&self, value: &str) -> Result<(), BusinessServiceError> {
        let url = parse_config_url(value, false)?;
        let host = url
            .host_str()
            .ok_or(BusinessServiceError::RejectedConfigUrl)?;
        if !self.client.is_control_api_host_allowed(host)? {
            return Err(BusinessServiceError::RejectedConfigUrl);
        }
        Ok(())
    }

    fn reconcile_unverified_session(&self) -> Result<(), BusinessServiceError> {
        let session = match self.client.authentication_state()? {
            AuthenticationSecretState::Empty => AuthSessionResponse::signed_out(),
            AuthenticationSecretState::Complete => {
                AuthSessionResponse::unverified(lock(&self.state).session.user.clone())
            }
            AuthenticationSecretState::Partial => {
                self.client.clear_authentication()?;
                AuthSessionResponse::signed_out()
            }
        };
        let mut state = lock(&self.state);
        if session.status == AuthSessionStatus::SignedOut {
            state.subscription = None;
        }
        state.session = session;
        Ok(())
    }

    fn require_config(&self) -> Result<ConfigResponse, BusinessServiceError> {
        lock(&self.state)
            .config
            .clone()
            .ok_or(BusinessServiceError::NotInitialized)
    }

    fn require_authenticated(&self) -> Result<(), BusinessServiceError> {
        let state = lock(&self.state);
        if state.config.is_none() {
            return Err(BusinessServiceError::NotInitialized);
        }
        if state.session.status != AuthSessionStatus::Authenticated {
            return Err(BusinessClientError::AuthenticationRequired.into());
        }
        Ok(())
    }

    fn acquire_operation(&self) -> Result<BusinessOperation<'_>, BusinessServiceError> {
        self.operation_in_flight
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| BusinessServiceError::SubmissionInProgress)?;
        Ok(BusinessOperation {
            in_flight: &self.operation_in_flight,
        })
    }

    fn execute_authenticated(
        &self,
        command: BusinessCommand,
    ) -> Result<BusinessCommandResponse, BusinessServiceError> {
        let request = BusinessCommandRequest::without_body(command)?;
        self.execute_authenticated_request(request)
    }

    fn execute_authenticated_json(
        &self,
        command: BusinessCommand,
        value: &impl Serialize,
    ) -> Result<BusinessCommandResponse, BusinessServiceError> {
        let request = BusinessCommandRequest::json(command, value)?;
        self.execute_authenticated_request(request)
    }

    fn execute_authenticated_request(
        &self,
        request: BusinessCommandRequest,
    ) -> Result<BusinessCommandResponse, BusinessServiceError> {
        match self.client.execute(request) {
            Ok(response) => Ok(response),
            Err(
                error @ (BusinessClientError::AuthenticationRequired
                | BusinessClientError::Unauthorized),
            ) => {
                let mut state = lock(&self.state);
                state.session = AuthSessionResponse::signed_out();
                state.subscription = None;
                Err(error.into())
            }
            Err(error) => Err(error.into()),
        }
    }

    fn authenticate(
        &self,
        command: BusinessCommand,
        request: &impl serde::Serialize,
    ) -> Result<AuthPublicResponse, BusinessServiceError> {
        let request = BusinessCommandRequest::json(command, request)?;
        let response = self.client.execute(request)?;
        let response = match decode_authentication_response(response)? {
            DecodedAuthentication::Development(mut wire) => {
                if wire.credentials.expires_at_unix_ms.get() <= self.clock.now_unix_ms() {
                    return Err(BusinessServiceError::ExpiredCredentials);
                }
                let mut access = SecretValue::new(
                    std::mem::take(&mut wire.credentials.access_token).into_bytes(),
                )
                .map_err(BusinessClientError::from)?;
                let mut refresh = SecretValue::new(
                    std::mem::take(&mut wire.credentials.refresh_token).into_bytes(),
                )
                .map_err(BusinessClientError::from)?;
                self.client
                    .replace_authentication(&mut access, &mut refresh)?;
                AuthPublicResponse {
                    schema_version: wire.schema_version,
                    authenticated: true,
                    user: wire.user,
                }
            }
            DecodedAuthentication::Production(mut data) => {
                let mut access = SecretValue::new(std::mem::take(&mut data.auth_data).into_bytes())
                    .map_err(BusinessClientError::from)?;
                let mut refresh = SecretValue::new(std::mem::take(&mut data.token).into_bytes())
                    .map_err(BusinessClientError::from)?;
                self.client
                    .replace_authentication(&mut access, &mut refresh)?;
                let account = match self.execute_authenticated(BusinessCommand::Account) {
                    Ok(response) => decode_account_response(response),
                    Err(error) => Err(error),
                };
                let account = match account {
                    Ok(account) => account,
                    Err(error) => {
                        let _ = self.client.clear_authentication();
                        return Err(error);
                    }
                };
                AuthPublicResponse {
                    schema_version: BUSINESS_API_SCHEMA_VERSION,
                    authenticated: true,
                    user: account.user,
                }
            }
        };
        let mut state = lock(&self.state);
        state.session = AuthSessionResponse::authenticated(response.user.clone());
        state.subscription = None;
        Ok(response)
    }
}

struct BusinessOperation<'a> {
    in_flight: &'a AtomicBool,
}

impl Drop for BusinessOperation<'_> {
    fn drop(&mut self) {
        self.in_flight.store(false, Ordering::Release);
    }
}

enum DecodedConfig {
    Development(ConfigWireResponse),
    Production(ProductionConfigData),
}

enum DecodedAuthentication {
    Development(AuthWireResponse),
    Production(ProductionLoginData),
}

fn decode_config_response(
    response: BusinessCommandResponse,
) -> Result<DecodedConfig, BusinessServiceError> {
    let body = take_json_body(response)?;
    if let Ok(envelope) = serde_json::from_slice::<ProductionEnvelope<ProductionConfigData>>(&body)
    {
        let config = envelope.into_data()?;
        validate_production_config(&config)?;
        return Ok(DecodedConfig::Production(config));
    }
    serde_json::from_slice(&body)
        .map(DecodedConfig::Development)
        .map_err(|_| BusinessServiceError::InvalidResponse)
}

fn decode_authentication_response(
    response: BusinessCommandResponse,
) -> Result<DecodedAuthentication, BusinessServiceError> {
    let body = take_json_body(response)?;
    if let Ok(envelope) = serde_json::from_slice::<ProductionEnvelope<ProductionLoginData>>(&body) {
        let mut data = envelope.into_data()?;
        data.auth_data = normalize_production_bearer(std::mem::take(&mut data.auth_data))?;
        if data.token.is_empty() || data.token.len() > 16 * 1024 {
            return Err(BusinessServiceError::InvalidResponse);
        }
        return Ok(DecodedAuthentication::Production(data));
    }
    serde_json::from_slice(&body)
        .map(DecodedAuthentication::Development)
        .map_err(|_| BusinessServiceError::InvalidResponse)
}

fn normalize_production_bearer(mut value: String) -> Result<String, BusinessServiceError> {
    const BEARER_PREFIX: &str = "Bearer ";
    let token_offset = value
        .strip_prefix(BEARER_PREFIX)
        .map_or(0, |_| BEARER_PREFIX.len());
    let token = &value[token_offset..];
    if token.is_empty()
        || token.len() > 16 * 1024
        || !token.bytes().all(|byte| {
            byte.is_ascii_alphanumeric()
                || matches!(byte, b'-' | b'.' | b'_' | b'~' | b'+' | b'/' | b'=')
        })
    {
        value.zeroize();
        return Err(BusinessServiceError::InvalidResponse);
    }
    if token_offset == 0 {
        return Ok(value);
    }
    let mut normalized = Zeroizing::new(token.to_owned());
    value.zeroize();
    Ok(std::mem::take(&mut normalized))
}

fn decode_account_response(
    response: BusinessCommandResponse,
) -> Result<AccountResponse, BusinessServiceError> {
    let body = take_json_body(response)?;
    if let Ok(envelope) = serde_json::from_slice::<ProductionEnvelope<ProductionAccountData>>(&body)
    {
        let data = envelope.into_data()?;
        validate_email(&data.email)?;
        if data.uuid.is_empty()
            || data.uuid.len() > 128
            || !data.uuid.is_ascii()
            || data.uuid.bytes().any(|byte| byte.is_ascii_control())
            || data.avatar_url.len() > 4 * 1024
            || data.commission_balance > 9_007_199_254_740_991
            || data.transfer_enable > 9_007_199_254_740_991
        {
            return Err(BusinessServiceError::InvalidResponse);
        }
        let balance =
            SafeInteger::new(data.balance).ok_or(BusinessServiceError::InvalidResponse)?;
        let currency = CurrencyCode::cny();
        let _observed_metadata = (
            data.commission_rate,
            data.created_at,
            data.discount,
            data.expired_at,
            data.last_login_at,
            data.plan_id,
            data.remind_expire,
            data.remind_traffic,
            data.telegram_id,
        );
        return Ok(AccountResponse {
            schema_version: BUSINESS_API_SCHEMA_VERSION,
            user: orange_domain::UserProfile {
                user_id: data.uuid,
                email: data.email,
                status: if data.banned {
                    AccountStatus::Disabled
                } else {
                    AccountStatus::Active
                },
                balance: Money {
                    minor_units: balance,
                    currency,
                },
            },
        });
    }
    serde_json::from_slice(&body).map_err(|_| BusinessServiceError::InvalidResponse)
}

fn decode_notices_response(
    response: BusinessCommandResponse,
) -> Result<NoticesResponse, BusinessServiceError> {
    let body = take_json_body(response)?;
    let response: ProductionNoticesResponse =
        serde_json::from_slice(&body).map_err(|_| BusinessServiceError::InvalidResponse)?;
    if response.total < response.data.len() as u64 {
        return Err(BusinessServiceError::InvalidResponse);
    }

    let notices = response
        .data
        .into_iter()
        .filter_map(|notice| {
            let _observed_metadata = (
                &notice.id,
                &notice.img_url,
                &notice.tags,
                &notice.created_at,
                &notice.updated_at,
                &notice.sort,
            );
            if !notice.show {
                return None;
            }
            let title = notice.title.trim();
            let content = notice.content.trim();
            if title.is_empty()
                || title.len() > MAX_NOTICE_TITLE_BYTES
                || title.chars().any(char::is_control)
                || content.is_empty()
                || content.len() > MAX_NOTICE_CONTENT_BYTES
                || !is_safe_ticket_text(content)
            {
                return None;
            }
            Some(Notice {
                title: title.to_owned(),
                content: content.to_owned(),
            })
        })
        .take(MAX_PUBLIC_NOTICES)
        .collect();

    Ok(NoticesResponse {
        schema_version: BUSINESS_API_SCHEMA_VERSION,
        notices,
    })
}

fn decode_subscription_response(
    response: BusinessCommandResponse,
) -> Result<SubscriptionWireResponse, BusinessServiceError> {
    let body = take_json_body(response)?;
    if let Ok(envelope) =
        serde_json::from_slice::<ProductionEnvelope<ProductionSubscriptionData>>(&body)
    {
        let mut data = envelope.into_data()?;
        let used = data
            .u
            .checked_add(data.d)
            .and_then(SafeInteger::new)
            .ok_or(BusinessServiceError::InvalidResponse)?;
        let upload = SafeInteger::new(data.u).ok_or(BusinessServiceError::InvalidResponse)?;
        let download =
            SafeInteger::new(data.d).ok_or(BusinessServiceError::InvalidResponse)?;
        let total =
            SafeInteger::new(data.transfer_enable).ok_or(BusinessServiceError::InvalidResponse)?;
        let expires_at_unix_ms = match data.expired_at {
            None | Some(0) => None,
            Some(seconds) => Some(
                seconds
                    .checked_mul(1_000)
                    .and_then(UnixMillis::new)
                    .ok_or(BusinessServiceError::InvalidResponse)?,
            ),
        };
        validate_email(&data.email)?;
        validate_subscription_url(&data.subscribe_url)?;
        if data.uuid.is_empty()
            || data.uuid.len() > 128
            || data.token.is_empty()
            || data.plan.as_ref().is_some_and(Map::is_empty)
        {
            return Err(BusinessServiceError::InvalidResponse);
        }
        let _observed_metadata = (
            data.device_limit,
            data.next_reset_at,
            data.reset_day,
            data.speed_limit,
        );
        let plan_id = data.plan_id.filter(|plan_id| *plan_id != 0);
        // 订阅接口会把整个 plan 对象回带，从里面取套餐名称。名称非法时
        // 只丢名称而不让整个订阅解码失败——订阅响应还承载着连接凭证。
        let plan_name = data
            .plan
            .as_ref()
            .and_then(|plan| plan.get("name"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|name| {
                !name.is_empty()
                    && name.len() <= MAX_PLAN_NAME_BYTES
                    && !name.chars().any(char::is_control)
            })
            .map(str::to_owned);
        return Ok(SubscriptionWireResponse {
            schema_version: BUSINESS_API_SCHEMA_VERSION,
            status: if plan_id.is_some() {
                SubscriptionStatus::Active
            } else {
                SubscriptionStatus::None
            },
            plan_id: plan_id.map(|plan_id| plan_id.to_string()),
            plan_name,
            expires_at_unix_ms,
            used_bytes: used,
            upload_bytes: Some(upload),
            download_bytes: Some(download),
            total_bytes: Some(total),
            subscription_credential: std::mem::take(&mut data.subscribe_url),
        });
    }
    serde_json::from_slice(&body).map_err(|_| BusinessServiceError::InvalidResponse)
}

fn decode_node_loads_response(
    response: BusinessCommandResponse,
) -> Result<NodeLoadsResponse, BusinessServiceError> {
    let body = take_json_body(response)?;
    let envelope: ProductionEnvelope<ProductionNodeLoadsData> =
        serde_json::from_slice(&body).map_err(|_| BusinessServiceError::InvalidResponse)?;
    let data = envelope.into_data()?;
    if data.schema_version != BUSINESS_API_SCHEMA_VERSION
        || !(30..=600).contains(&data.ttl_seconds)
        || SafeInteger::new(data.generated_at).is_none()
        || data.nodes.len() > MAX_PUBLIC_NODE_LOADS
    {
        return Err(BusinessServiceError::InvalidResponse);
    }

    let mut ids = HashSet::with_capacity(data.nodes.len());
    let mut nodes = Vec::with_capacity(data.nodes.len());
    for node in data.nodes {
        let valid_id = valid_load_identifier(&node.id) && ids.insert(node.id.clone());
        let valid_group = valid_load_identifier(&node.capacity_group);
        let valid_load = node
            .load
            .is_none_or(|load| load.is_finite() && (0.0..=1.0).contains(&load));
        let valid_weight =
            node.selection_weight.is_finite() && (0.1..=10.0).contains(&node.selection_weight);
        let state_matches = match node.state {
            NodeLoadState::Unknown => node.load.is_none(),
            NodeLoadState::Idle
            | NodeLoadState::Normal
            | NodeLoadState::Busy
            | NodeLoadState::Overloaded => node.load.is_some(),
        };
        if !valid_id || !valid_group || !valid_load || !valid_weight || !state_matches {
            return Err(BusinessServiceError::InvalidResponse);
        }
        if node
            .updated_at
            .is_some_and(|updated_at| SafeInteger::new(updated_at).is_none())
        {
            return Err(BusinessServiceError::InvalidResponse);
        }
        nodes.push(NodeLoad {
            id: node.id,
            capacity_group: node.capacity_group,
            load: node.load,
            state: node.state,
            updated_at: node.updated_at,
            selection_weight: node.selection_weight,
        });
    }

    Ok(NodeLoadsResponse {
        schema_version: BUSINESS_API_SCHEMA_VERSION,
        generated_at: data.generated_at,
        ttl_seconds: data.ttl_seconds,
        nodes,
    })
}

fn valid_load_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

fn decode_plans_response(
    response: BusinessCommandResponse,
) -> Result<PlansResponse, BusinessServiceError> {
    let body = take_json_body(response)?;
    if let Ok(envelope) =
        serde_json::from_slice::<ProductionEnvelope<Vec<ProductionPlanData>>>(&body)
    {
        let source = envelope.into_data()?;
        if source.len() > MAX_PUBLIC_PLANS {
            return Err(BusinessServiceError::InvalidResponse);
        }
        let currency = CurrencyCode::cny();
        let mut plans = Vec::new();
        for data in source {
            if data.id == 0 || data.show == Some(false) {
                continue;
            }
            let name = data.name.trim();
            if name.is_empty()
                || name.len() > MAX_PLAN_NAME_BYTES
                || name.chars().any(char::is_control)
            {
                return Err(BusinessServiceError::InvalidResponse);
            }
            let description_html = data
                .content
                .as_deref()
                .map(str::trim)
                .filter(|content| !content.is_empty())
                .map(|content| {
                    if content.len() > MAX_PLAN_DESCRIPTION_BYTES {
                        Err(BusinessServiceError::InvalidResponse)
                    } else {
                        Ok(content.to_owned())
                    }
                })
                .transpose()?;
            let traffic_bytes = match data.transfer_enable {
                Some(gibibytes) => Some(
                    gibibytes
                        .checked_mul(GIB_BYTES)
                        .and_then(SafeInteger::new)
                        .ok_or(BusinessServiceError::InvalidResponse)?,
                ),
                None => None,
            };
            let periods = [
                ("month_price", 30, data.month_price),
                ("quarter_price", 90, data.quarter_price),
                ("half_year_price", 180, data.half_year_price),
                ("year_price", 365, data.year_price),
                ("two_year_price", 730, data.two_year_price),
                ("three_year_price", 1_095, data.three_year_price),
                ("onetime_price", 0, data.onetime_price),
            ];
            for (period, billing_period_days, price) in periods {
                // Prices arrive as float cents (e.g. 1999.0); round to the
                // nearest minor unit instead of truncating.
                let Some(price) = price.filter(|price| {
                    price.is_finite() && *price > 0.0 && *price <= 9_007_199_254_740_991.0
                }) else {
                    continue;
                };
                if plans.len() >= MAX_PUBLIC_PLANS {
                    return Err(BusinessServiceError::InvalidResponse);
                }
                plans.push(Plan {
                    plan_id: format!("{}:{period}", data.id),
                    name: name.to_owned(),
                    description_html: description_html.clone(),
                    price: Money {
                        minor_units: SafeInteger::new(price.round() as u64)
                            .ok_or(BusinessServiceError::InvalidResponse)?,
                        currency: currency.clone(),
                    },
                    billing_period_days: SafeInteger::new(billing_period_days)
                        .ok_or(BusinessServiceError::InvalidResponse)?,
                    traffic_bytes,
                });
            }
            let _observed_metadata = (
                data.capacity_limit,
                data.created_at,
                data.device_limit,
                data.group_id,
                data.renew,
                data.reset_price,
                data.reset_traffic_method,
                data.sell,
                data.sort,
                data.speed_limit,
                data.tags,
                data.updated_at,
            );
        }
        return Ok(PlansResponse {
            schema_version: BUSINESS_API_SCHEMA_VERSION,
            plans,
        });
    }
    let plans: PlansResponse =
        serde_json::from_slice(&body).map_err(|_| BusinessServiceError::InvalidResponse)?;
    if plans.plans.len() > MAX_PUBLIC_PLANS {
        return Err(BusinessServiceError::InvalidResponse);
    }
    Ok(plans)
}

fn decode_orders_response(
    response: BusinessCommandResponse,
) -> Result<OrdersResponse, BusinessServiceError> {
    let body = take_json_body(response)?;
    if let Ok(envelope) =
        serde_json::from_slice::<ProductionEnvelope<Vec<ProductionOrderData>>>(&body)
    {
        let source = envelope.into_data()?;
        if source.len() > MAX_PUBLIC_ORDERS {
            return Err(BusinessServiceError::InvalidResponse);
        }
        let currency = CurrencyCode::cny();
        let mut orders = Vec::with_capacity(source.len());
        for data in source {
            orders.push(production_order_summary(&data, &currency)?);
            let _observed_metadata = (
                data.actual_commission_balance,
                data.balance_amount,
                data.callback_no,
                data.commission_balance,
                data.commission_status,
                data.coupon_code,
                data.coupon_id,
                data.discount_amount,
                data.handling_amount,
                data.id,
                data.invite_user_id,
                data.payment,
                data.payment_id,
                data.refund_amount,
                data.site_id,
                data.surplus_amount,
                data.surplus_credit,
                data.surplus_order_ids,
                data.surplus_orders,
                data.tixianstatus,
                data.try_out_plan_id,
                data.r#type,
                data.updated_at,
                data.user_id,
            );
        }
        return Ok(OrdersResponse {
            schema_version: BUSINESS_API_SCHEMA_VERSION,
            orders,
        });
    }
    let orders: OrdersResponse =
        serde_json::from_slice(&body).map_err(|_| BusinessServiceError::InvalidResponse)?;
    if orders.orders.len() > MAX_PUBLIC_ORDERS {
        return Err(BusinessServiceError::InvalidResponse);
    }
    Ok(orders)
}

fn decode_order_detail_response(
    response: BusinessCommandResponse,
) -> Result<OrderDetailResponse, BusinessServiceError> {
    let body = take_json_body(response)?;
    if let Ok(envelope) = serde_json::from_slice::<ProductionEnvelope<ProductionOrderData>>(&body) {
        let data = envelope.into_data()?;
        let currency = CurrencyCode::cny();
        let summary = production_order_summary(&data, &currency)?;
        let traffic_bytes = data
            .plan
            .as_ref()
            .and_then(|plan| plan.get("transfer_enable"))
            .and_then(Value::as_u64)
            .map(|gibibytes| {
                gibibytes
                    .checked_mul(GIB_BYTES)
                    .and_then(SafeInteger::new)
                    .ok_or(BusinessServiceError::InvalidResponse)
            })
            .transpose()?;
        let updated_at_unix_ms = data.updated_at.map(production_unix_seconds).transpose()?;
        return Ok(OrderDetailResponse {
            schema_version: BUSINESS_API_SCHEMA_VERSION,
            order: OrderDetail {
                order_id: summary.order_id,
                plan_id: summary.plan_id,
                plan_name: summary.plan_name,
                billing_period_days: summary.billing_period_days,
                traffic_bytes,
                status: summary.status,
                amount: summary.amount,
                created_at_unix_ms: summary.created_at_unix_ms,
                updated_at_unix_ms,
                paid_at_unix_ms: summary.paid_at_unix_ms,
            },
        });
    }
    serde_json::from_slice(&body).map_err(|_| BusinessServiceError::InvalidResponse)
}

fn production_order_summary(
    data: &ProductionOrderData,
    currency: &CurrencyCode,
) -> Result<OrderSummary, BusinessServiceError> {
    let order_id = data
        .trade_no
        .as_deref()
        .filter(|value| valid_order_id(value))
        .ok_or(BusinessServiceError::InvalidResponse)?;
    let plan_id = data
        .plan_id
        .or_else(|| {
            data.plan
                .as_ref()
                .and_then(|plan| plan.get("id"))
                .and_then(Value::as_u64)
        })
        .filter(|value| *value > 0)
        .ok_or(BusinessServiceError::InvalidResponse)?;
    let plan_name = data
        .plan
        .as_ref()
        .and_then(|plan| plan.get("name"))
        .and_then(Value::as_str)
        .unwrap_or("已下架套餐")
        .trim();
    if plan_name.is_empty() || plan_name.len() > 256 || plan_name.chars().any(char::is_control) {
        return Err(BusinessServiceError::InvalidResponse);
    }
    let created_at_unix_ms = production_unix_seconds(
        data.created_at
            .ok_or(BusinessServiceError::InvalidResponse)?,
    )?;
    let paid_at_unix_ms = production_optional_unix_seconds(data.paid_at.as_ref())?;
    let amount = SafeInteger::new(
        data.total_amount
            .ok_or(BusinessServiceError::InvalidResponse)?,
    )
    .ok_or(BusinessServiceError::InvalidResponse)?;
    let status = match data.status {
        Some(0) => OrderStatus::Pending,
        Some(1 | 3) => OrderStatus::Paid,
        Some(2) => OrderStatus::Cancelled,
        Some(4) => OrderStatus::Refunded,
        Some(_) | None => OrderStatus::Unknown,
    };
    let billing_period_days = match data.period.as_deref() {
        Some("month_price") => SafeInteger::new(30),
        Some("quarter_price") => SafeInteger::new(90),
        Some("half_year_price") => SafeInteger::new(180),
        Some("year_price") => SafeInteger::new(365),
        Some("two_year_price") => SafeInteger::new(730),
        Some("three_year_price") => SafeInteger::new(1_095),
        Some("onetime_price") => SafeInteger::new(0),
        Some(_) | None => None,
    };
    Ok(OrderSummary {
        order_id: order_id.to_owned(),
        plan_id: plan_id.to_string(),
        plan_name: plan_name.to_owned(),
        billing_period_days,
        status,
        amount: Money {
            minor_units: amount,
            currency: currency.clone(),
        },
        created_at_unix_ms,
        paid_at_unix_ms,
    })
}

fn decode_payment_methods_response(
    response: BusinessCommandResponse,
) -> Result<PaymentMethodsResponse, BusinessServiceError> {
    let body = take_json_body(response)?;
    if let Ok(envelope) =
        serde_json::from_slice::<ProductionEnvelope<Vec<ProductionPaymentMethodData>>>(&body)
    {
        let source = envelope.into_data()?;
        if source.len() > MAX_PUBLIC_PAYMENT_METHODS {
            return Err(BusinessServiceError::InvalidResponse);
        }
        let mut payment_methods = Vec::with_capacity(source.len());
        for data in source {
            let payment_method_id = data
                .id
                .filter(|value| *value > 0)
                .ok_or(BusinessServiceError::InvalidResponse)?;
            let name = data
                .name
                .as_deref()
                .map(str::trim)
                .filter(|value| {
                    !value.is_empty() && value.len() <= 128 && !value.chars().any(char::is_control)
                })
                .ok_or(BusinessServiceError::InvalidResponse)?;
            let provider = data
                .payment
                .as_deref()
                .map(str::trim)
                .filter(|value| {
                    !value.is_empty()
                        && value.len() <= 64
                        && value.bytes().all(|byte| {
                            byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-')
                        })
                })
                .ok_or(BusinessServiceError::InvalidResponse)?;
            let handling_fee_percent =
                normalize_nonnegative_decimal(data.handling_fee_percent.as_ref())?;
            payment_methods.push(PaymentMethod {
                payment_method_id: payment_method_id.to_string(),
                name: name.to_owned(),
                provider: provider.to_owned(),
                handling_fee_percent,
            });
            let _observed_metadata = (data.handling_fee_fixed, data.icon);
        }
        return Ok(PaymentMethodsResponse {
            schema_version: BUSINESS_API_SCHEMA_VERSION,
            payment_methods,
        });
    }
    let payment_methods: PaymentMethodsResponse =
        serde_json::from_slice(&body).map_err(|_| BusinessServiceError::InvalidResponse)?;
    if payment_methods.payment_methods.len() > MAX_PUBLIC_PAYMENT_METHODS {
        return Err(BusinessServiceError::InvalidResponse);
    }
    Ok(payment_methods)
}

// The checkout endpoint does NOT use the standard success envelope; it
// responds with a raw `{ "type": int, "data": bool|string }` object:
// type -1 = free order (paid instantly, data: true), type 0 = QR-code content.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProductionCheckoutResponse {
    data: Value,
    r#type: i64,
}

fn decode_checkout_order_response(
    response: BusinessCommandResponse,
    order_id: &str,
) -> Result<PaymentWireResponse, BusinessServiceError> {
    let body = take_json_body(response)?;
    let checkout: ProductionCheckoutResponse =
        serde_json::from_slice(&body).map_err(|_| BusinessServiceError::InvalidResponse)?;
    if checkout.r#type == -1 {
        if checkout.data != Value::Bool(true) {
            return Err(BusinessServiceError::InvalidResponse);
        }
        return Ok(PaymentWireResponse {
            schema_version: BUSINESS_API_SCHEMA_VERSION,
            order_id: order_id.to_owned(),
            status: PaymentStatus::Ready,
            qr_code: None,
            expires_at_unix_ms: None,
        });
    }
    if checkout.r#type != 0 {
        return Err(BusinessServiceError::InvalidResponse);
    }
    let qr_code = checkout
        .data
        .as_str()
        .map(str::trim)
        .filter(|value| {
            !value.is_empty() && value.len() <= 4 * 1024 && !value.chars().any(char::is_control)
        })
        .ok_or(BusinessServiceError::InvalidResponse)?
        .to_owned();
    Ok(PaymentWireResponse {
        schema_version: BUSINESS_API_SCHEMA_VERSION,
        order_id: order_id.to_owned(),
        status: PaymentStatus::Ready,
        qr_code: Some(qr_code),
        expires_at_unix_ms: None,
    })
}

fn decode_cancel_order_response(
    response: BusinessCommandResponse,
    order_id: &str,
) -> Result<CancelOrderResponse, BusinessServiceError> {
    let body = take_json_body(response)?;
    if let Ok(envelope) = serde_json::from_slice::<ProductionStatusEnvelope>(&body) {
        envelope.ensure_success()?;
        return Ok(CancelOrderResponse {
            schema_version: BUSINESS_API_SCHEMA_VERSION,
            order_id: order_id.to_owned(),
            status: OrderStatus::Cancelled,
        });
    }
    serde_json::from_slice(&body).map_err(|_| BusinessServiceError::InvalidResponse)
}

fn decode_invitation_center_response(
    response: BusinessCommandResponse,
) -> Result<InvitationCenterResponse, BusinessServiceError> {
    let body = take_json_body(response)?;
    if let Ok(envelope) =
        serde_json::from_slice::<ProductionEnvelope<ProductionInvitationCenterData>>(&body)
    {
        let data = envelope.into_data()?;
        let source = data.codes.unwrap_or_default();
        if source.len() > MAX_PUBLIC_INVITATION_CODES {
            return Err(BusinessServiceError::InvalidResponse);
        }
        let stat = data.stat.ok_or(BusinessServiceError::InvalidResponse)?;
        // Server shape: [registered users, confirmed commission, pending
        // commission, commission rate percent, available commission].
        let [
            registered_users,
            total_commission,
            pending_commission,
            commission_rate_percent,
            _available_commission,
        ] = stat.as_slice()
        else {
            return Err(BusinessServiceError::InvalidResponse);
        };
        let currency = CurrencyCode::cny();
        let stats = InvitationStats {
            registered_users: SafeInteger::new(*registered_users)
                .ok_or(BusinessServiceError::InvalidResponse)?,
            pending_commission: Money {
                minor_units: SafeInteger::new(*pending_commission)
                    .ok_or(BusinessServiceError::InvalidResponse)?,
                currency: currency.clone(),
            },
            total_commission: Money {
                minor_units: SafeInteger::new(*total_commission)
                    .ok_or(BusinessServiceError::InvalidResponse)?,
                currency,
            },
            commission_rate_percent: SafeInteger::new(*commission_rate_percent)
                .ok_or(BusinessServiceError::InvalidResponse)?,
        };
        let mut codes = Vec::with_capacity(source.len());
        for item in source {
            if !valid_invitation_code(&item.code) {
                return Err(BusinessServiceError::InvalidResponse);
            }
            let views = SafeInteger::new(item.pv.unwrap_or_default())
                .ok_or(BusinessServiceError::InvalidResponse)?;
            let created_at_unix_ms = item.created_at.map(production_unix_seconds).transpose()?;
            let status = match item.status {
                Some(false) => InvitationCodeStatus::Available,
                Some(true) => InvitationCodeStatus::Used,
                None => InvitationCodeStatus::Unknown,
            };
            let _observed_metadata = (item.updated_at, item.user_id);
            codes.push(InvitationCode {
                code: item.code,
                status,
                views,
                created_at_unix_ms,
            });
        }
        return Ok(InvitationCenterResponse {
            schema_version: BUSINESS_API_SCHEMA_VERSION,
            stats,
            codes,
        });
    }
    let invitation: InvitationCenterResponse =
        serde_json::from_slice(&body).map_err(|_| BusinessServiceError::InvalidResponse)?;
    if invitation.codes.len() > MAX_PUBLIC_INVITATION_CODES
        || invitation
            .codes
            .iter()
            .any(|item| !valid_invitation_code(&item.code))
    {
        return Err(BusinessServiceError::InvalidResponse);
    }
    Ok(invitation)
}

fn decode_status_response(response: BusinessCommandResponse) -> Result<(), BusinessServiceError> {
    let body = take_json_body(response)?;
    let envelope: ProductionStatusEnvelope =
        serde_json::from_slice(&body).map_err(|_| BusinessServiceError::InvalidResponse)?;
    envelope.ensure_success()
}

fn decode_tickets_response(
    response: BusinessCommandResponse,
) -> Result<TicketsResponse, BusinessServiceError> {
    let body = take_json_body(response)?;
    if let Ok(envelope) =
        serde_json::from_slice::<ProductionEnvelope<Vec<ProductionTicketData>>>(&body)
    {
        let source = envelope.into_data()?;
        if source.len() > MAX_PUBLIC_TICKETS {
            return Err(BusinessServiceError::InvalidResponse);
        }
        let mut tickets = Vec::with_capacity(source.len());
        for item in source {
            let ticket_id = item
                .id
                .and_then(SafeInteger::new)
                .filter(|value| value.get() > 0)
                .ok_or(BusinessServiceError::InvalidResponse)?
                .get()
                .to_string();
            let subject = item
                .subject
                .as_deref()
                .map(str::trim)
                .filter(|value| {
                    !value.is_empty() && value.len() <= 512 && !value.chars().any(char::is_control)
                })
                .ok_or(BusinessServiceError::InvalidResponse)?
                .to_owned();
            let last_message_at_unix_ms = production_unix_seconds(
                item.updated_at
                    .or(item.created_at)
                    .ok_or(BusinessServiceError::InvalidResponse)?,
            )?;
            let status = match item.status {
                Some(0) => TicketStatus::Open,
                Some(1) => TicketStatus::Closed,
                Some(2) => TicketStatus::Answered,
                _ => TicketStatus::Unknown,
            };
            let closed_at_unix_ms =
                (status == TicketStatus::Closed).then_some(last_message_at_unix_ms);
            let _observed_metadata = (item.level, item.message, item.reply_status, item.user_id);
            tickets.push(Ticket {
                ticket_id,
                status,
                subject,
                last_message_at_unix_ms,
                closed_at_unix_ms,
            });
        }
        return Ok(TicketsResponse {
            schema_version: BUSINESS_API_SCHEMA_VERSION,
            tickets,
        });
    }
    let tickets: TicketsResponse =
        serde_json::from_slice(&body).map_err(|_| BusinessServiceError::InvalidResponse)?;
    if tickets.tickets.len() > MAX_PUBLIC_TICKETS {
        return Err(BusinessServiceError::InvalidResponse);
    }
    Ok(tickets)
}

fn decode_ticket_detail_response(
    response: BusinessCommandResponse,
    ticket_id: &str,
) -> Result<TicketDetailResponse, BusinessServiceError> {
    let body = take_json_body(response)?;
    if let Ok(envelope) =
        serde_json::from_slice::<ProductionEnvelope<ProductionTicketDetailData>>(&body)
    {
        let data = envelope.into_data()?;
        let numeric_ticket_id = ticket_id
            .parse::<u64>()
            .ok()
            .filter(|value| *value > 0)
            .ok_or(BusinessServiceError::InvalidResponse)?;
        if data.id != Some(numeric_ticket_id) {
            return Err(BusinessServiceError::InvalidResponse);
        }
        let subject = data
            .subject
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty() && value.len() <= 512 && is_safe_ticket_text(value))
            .ok_or(BusinessServiceError::InvalidResponse)?
            .to_owned();
        let created_at_unix_ms = production_unix_seconds(
            data.created_at
                .ok_or(BusinessServiceError::InvalidResponse)?,
        )?;
        let updated_at_unix_ms = production_unix_seconds(
            data.updated_at
                .or(data.created_at)
                .ok_or(BusinessServiceError::InvalidResponse)?,
        )?;
        let status = match data.status {
            Some(0) => TicketStatus::Open,
            Some(1) => TicketStatus::Closed,
            Some(2) => TicketStatus::Answered,
            _ => TicketStatus::Unknown,
        };
        let source = data.message.unwrap_or_default();
        if source.len() > MAX_PUBLIC_TICKET_MESSAGES {
            return Err(BusinessServiceError::InvalidResponse);
        }
        let mut messages = Vec::with_capacity(source.len());
        for item in source {
            let message_id = item
                .id
                .and_then(SafeInteger::new)
                .filter(|value| value.get() > 0)
                .ok_or(BusinessServiceError::InvalidResponse)?
                .get()
                .to_string();
            if item.ticket_id != Some(numeric_ticket_id) {
                return Err(BusinessServiceError::InvalidResponse);
            }
            let message = item
                .message
                .as_deref()
                .map(str::trim)
                .filter(|value| {
                    !value.is_empty() && value.len() <= 64 * 1024 && is_safe_ticket_text(value)
                })
                .ok_or(BusinessServiceError::InvalidResponse)?
                .to_owned();
            let created_at_unix_ms = production_unix_seconds(
                item.created_at
                    .or(item.updated_at)
                    .ok_or(BusinessServiceError::InvalidResponse)?,
            )?;
            let from_user = item.is_me.ok_or(BusinessServiceError::InvalidResponse)?;
            let _observed_metadata = (item.updated_at, item.photo, item.profile_pic);
            messages.push(TicketMessage {
                message_id,
                from_user,
                body: message,
                created_at_unix_ms,
            });
        }
        let _observed_metadata = (data.level, data.reply_status, data.user_id);
        return Ok(TicketDetailResponse {
            schema_version: BUSINESS_API_SCHEMA_VERSION,
            ticket: TicketDetail {
                ticket_id: ticket_id.to_owned(),
                status,
                subject,
                created_at_unix_ms,
                updated_at_unix_ms,
                messages,
            },
        });
    }
    let detail: TicketDetailResponse =
        serde_json::from_slice(&body).map_err(|_| BusinessServiceError::InvalidResponse)?;
    if detail.ticket.ticket_id != ticket_id
        || detail.ticket.messages.len() > MAX_PUBLIC_TICKET_MESSAGES
        || detail.ticket.messages.iter().any(|item| {
            item.body.is_empty() || item.body.len() > 64 * 1024 || !is_safe_ticket_text(&item.body)
        })
    {
        return Err(BusinessServiceError::InvalidResponse);
    }
    Ok(detail)
}

fn normalize_nonnegative_decimal(value: Option<&Value>) -> Result<String, BusinessServiceError> {
    let text = match value {
        None | Some(Value::Null) => return Ok("0".to_owned()),
        Some(Value::Number(number)) => number.to_string(),
        Some(Value::String(text)) => text.trim().to_owned(),
        Some(_) => return Err(BusinessServiceError::InvalidResponse),
    };
    let mut parts = text.split('.');
    let whole = parts.next().unwrap_or_default();
    let fraction = parts.next();
    let valid = !text.is_empty()
        && text.len() <= 32
        && parts.next().is_none()
        && !whole.is_empty()
        && whole.bytes().all(|byte| byte.is_ascii_digit())
        && fraction.is_none_or(|fraction| {
            !fraction.is_empty()
                && fraction.len() <= 6
                && fraction.bytes().all(|byte| byte.is_ascii_digit())
        });
    if !valid {
        return Err(BusinessServiceError::InvalidResponse);
    }
    Ok(text)
}

fn decode_create_order_response(
    response: BusinessCommandResponse,
) -> Result<CreateOrderResponse, BusinessServiceError> {
    let body = take_json_body(response)?;
    if let Ok(envelope) = serde_json::from_slice::<ProductionEnvelope<String>>(&body) {
        let order_id = envelope.into_data()?;
        if !valid_order_id(&order_id) {
            return Err(BusinessServiceError::InvalidResponse);
        }
        return Ok(CreateOrderResponse {
            schema_version: BUSINESS_API_SCHEMA_VERSION,
            order_id,
        });
    }
    serde_json::from_slice(&body).map_err(|_| BusinessServiceError::InvalidResponse)
}

fn parse_production_plan_selection(value: &str) -> Result<(u64, &str), BusinessServiceError> {
    let (plan_id, period) = value
        .split_once(':')
        .ok_or(BusinessServiceError::InvalidPlan)?;
    let plan_id = plan_id
        .parse::<u64>()
        .ok()
        .filter(|value| *value > 0)
        .ok_or(BusinessServiceError::InvalidPlan)?;
    if !matches!(
        period,
        "month_price"
            | "quarter_price"
            | "half_year_price"
            | "year_price"
            | "two_year_price"
            | "three_year_price"
            | "onetime_price"
    ) {
        return Err(BusinessServiceError::InvalidPlan);
    }
    Ok((plan_id, period))
}

fn valid_order_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

fn valid_invitation_code(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_INVITE_CODE_BYTES
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

fn valid_ticket_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 20
        && !value.starts_with('0')
        && value.bytes().all(|byte| byte.is_ascii_digit())
}

fn decode_commission_config_response(
    response: BusinessCommandResponse,
) -> Result<CommissionConfigResponse, BusinessServiceError> {
    let body = take_json_body(response)?;
    if let Ok(envelope) =
        serde_json::from_slice::<ProductionEnvelope<ProductionCommissionConfigData>>(&body)
    {
        let data = envelope.into_data()?;
        let methods: Vec<String> = data
            .withdraw_methods
            .unwrap_or_default()
            .into_iter()
            .map(|method| method.trim().to_owned())
            .filter(|method| {
                !method.is_empty()
                    && method.len() <= 64
                    && !method.chars().any(char::is_control)
            })
            .collect();
        let telegram_link = data
            .telegram_discuss_link
            .map(|link| link.trim().to_owned())
            .filter(|link| link.starts_with("https://") && link.len() <= 512);
        let withdraw_closed = data
            .withdraw_close
            .as_ref()
            .is_some_and(|value| value.as_u64() == Some(1) || value.as_bool() == Some(true));
        let _observed_config = (
            data.commission_distribution_enable,
            data.commission_distribution_l1,
            data.commission_distribution_l2,
            data.commission_distribution_l3,
            data.currency,
            data.currency_symbol,
            data.is_telegram,
            data.stripe_pk,
        );
        return Ok(CommissionConfigResponse {
            schema_version: BUSINESS_API_SCHEMA_VERSION,
            withdraw_methods: methods,
            withdraw_closed,
            telegram_discuss_link: telegram_link,
        });
    }
    serde_json::from_slice(&body).map_err(|_| BusinessServiceError::InvalidResponse)
}

const MAX_PUBLIC_ACTIVE_SESSIONS: usize = 64;

fn decode_active_sessions_response(
    response: BusinessCommandResponse,
) -> Result<ActiveSessionsResponse, BusinessServiceError> {
    let body = take_json_body(response)?;
    if let Ok(envelope) =
        serde_json::from_slice::<ProductionEnvelope<Vec<ProductionActiveSessionData>>>(&body)
    {
        let source = envelope.into_data()?;
        if source.len() > MAX_PUBLIC_ACTIVE_SESSIONS {
            return Err(BusinessServiceError::InvalidResponse);
        }
        let clean = |value: Option<String>| {
            value
                .map(|text| text.trim().to_owned())
                .filter(|text| !text.is_empty() && valid_gift_card_text(text))
        };
        let mut sessions = Vec::with_capacity(source.len());
        for session in source {
            // The raw access token is sensitive and never leaves the process.
            let _sensitive_token = session.token.map(zeroize::Zeroizing::new);
            let _observed_metadata = (
                session.abilities,
                session.expires_at,
                session.tokenable_id,
                session.tokenable_type,
                session.updated_at,
            );
            sessions.push(ActiveSessionInfo {
                session_id: session.id.to_string(),
                name: clean(session.name),
                last_used_at: clean(session.last_used_at),
                created_at: clean(session.created_at),
            });
        }
        return Ok(ActiveSessionsResponse {
            schema_version: BUSINESS_API_SCHEMA_VERSION,
            sessions,
        });
    }
    serde_json::from_slice(&body).map_err(|_| BusinessServiceError::InvalidResponse)
}

const MAX_KNOWLEDGE_TITLE_BYTES: usize = 256;
const MAX_KNOWLEDGE_CATEGORY_BYTES: usize = 128;
const MAX_KNOWLEDGE_BODY_BYTES: usize = 256 * 1024;
const MAX_PUBLIC_KNOWLEDGE_ARTICLES: usize = 512;

fn decode_knowledge_article_meta(
    article: &ProductionKnowledgeArticleData,
) -> Result<(String, Option<UnixMillis>), BusinessServiceError> {
    let title = article.title.trim();
    if title.is_empty()
        || title.len() > MAX_KNOWLEDGE_TITLE_BYTES
        || !is_safe_ticket_text(title)
    {
        return Err(BusinessServiceError::InvalidResponse);
    }
    let updated_at = match article.updated_at {
        0 => None,
        seconds => Some(
            seconds
                .checked_mul(1_000)
                .and_then(UnixMillis::new)
                .ok_or(BusinessServiceError::InvalidResponse)?,
        ),
    };
    Ok((title.to_owned(), updated_at))
}

fn decode_knowledge_list_response(
    response: BusinessCommandResponse,
) -> Result<KnowledgeListResponse, BusinessServiceError> {
    let body = take_json_body(response)?;
    if let Ok(envelope) = serde_json::from_slice::<
        ProductionEnvelope<std::collections::BTreeMap<String, Vec<ProductionKnowledgeArticleData>>>,
    >(&body)
    {
        let source = envelope.into_data()?;
        let total: usize = source.values().map(Vec::len).sum();
        if total > MAX_PUBLIC_KNOWLEDGE_ARTICLES {
            return Err(BusinessServiceError::InvalidResponse);
        }
        let mut groups = Vec::with_capacity(source.len());
        for (category, articles) in source {
            let category = category.trim();
            if category.len() > MAX_KNOWLEDGE_CATEGORY_BYTES
                || category.chars().any(char::is_control)
            {
                return Err(BusinessServiceError::InvalidResponse);
            }
            let mut summaries = Vec::with_capacity(articles.len());
            for article in &articles {
                let (title, updated_at) = decode_knowledge_article_meta(article)?;
                let _observed_body_size = article.body.len();
                summaries.push(KnowledgeArticleSummary {
                    article_id: article.id.to_string(),
                    title,
                    updated_at_unix_ms: updated_at,
                });
            }
            groups.push(KnowledgeGroup {
                category: category.to_owned(),
                articles: summaries,
            });
        }
        return Ok(KnowledgeListResponse {
            schema_version: BUSINESS_API_SCHEMA_VERSION,
            groups,
        });
    }
    serde_json::from_slice(&body).map_err(|_| BusinessServiceError::InvalidResponse)
}

fn decode_knowledge_detail_response(
    response: BusinessCommandResponse,
) -> Result<KnowledgeDetailResponse, BusinessServiceError> {
    let body = take_json_body(response)?;
    if let Ok(envelope) =
        serde_json::from_slice::<ProductionEnvelope<ProductionKnowledgeArticleData>>(&body)
    {
        let article = envelope.into_data()?;
        let (title, updated_at) = decode_knowledge_article_meta(&article)?;
        let body_html = article.body.trim();
        if body_html.is_empty() || body_html.len() > MAX_KNOWLEDGE_BODY_BYTES {
            return Err(BusinessServiceError::InvalidResponse);
        }
        let category = article.category.trim();
        if category.len() > MAX_KNOWLEDGE_CATEGORY_BYTES
            || category.chars().any(char::is_control)
        {
            return Err(BusinessServiceError::InvalidResponse);
        }
        return Ok(KnowledgeDetailResponse {
            schema_version: BUSINESS_API_SCHEMA_VERSION,
            article_id: article.id.to_string(),
            category: (!category.is_empty()).then(|| category.to_owned()),
            title,
            body_html: body_html.to_owned(),
            updated_at_unix_ms: updated_at,
        });
    }
    serde_json::from_slice(&body).map_err(|_| BusinessServiceError::InvalidResponse)
}

const MAX_GIFT_CARD_TEXT_BYTES: usize = 512;
const MAX_GIFT_CARD_REWARD_JSON_BYTES: usize = 8 * 1024;
const MAX_PUBLIC_GIFT_CARD_RECORDS: usize = 100;

fn valid_gift_card_text(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_GIFT_CARD_TEXT_BYTES
        && !value.chars().any(char::is_control)
}

fn reward_json_string(value: &Map<String, Value>) -> Option<String> {
    let json = serde_json::to_string(value).ok()?;
    (json.len() <= MAX_GIFT_CARD_REWARD_JSON_BYTES).then_some(json)
}

fn decode_gift_card_check_response(
    response: BusinessCommandResponse,
) -> Result<GiftCardCheckResponse, BusinessServiceError> {
    let body = take_json_body(response)?;
    if let Ok(envelope) =
        serde_json::from_slice::<ProductionEnvelope<ProductionGiftCardCheckData>>(&body)
    {
        let data = envelope.into_data()?;
        let reason = data
            .reason
            .map(|reason| reason.trim().to_owned())
            .filter(|reason| !reason.is_empty());
        if reason.as_ref().is_some_and(|reason| !valid_gift_card_text(reason)) {
            return Err(BusinessServiceError::InvalidResponse);
        }
        let card_name = data
            .code_info
            .template
            .name
            .map(|name| name.trim().to_owned())
            .filter(|name| !name.is_empty());
        if card_name
            .as_ref()
            .is_some_and(|name| !valid_gift_card_text(name))
        {
            return Err(BusinessServiceError::InvalidResponse);
        }
        let reward_preview_json = data
            .reward_preview
            .as_ref()
            .and_then(reward_json_string);
        return Ok(GiftCardCheckResponse {
            schema_version: BUSINESS_API_SCHEMA_VERSION,
            can_redeem: data.can_redeem,
            reason,
            card_name,
            reward_preview_json,
        });
    }
    serde_json::from_slice(&body).map_err(|_| BusinessServiceError::InvalidResponse)
}

fn decode_gift_card_redeem_response(
    response: BusinessCommandResponse,
) -> Result<GiftCardRedeemResponse, BusinessServiceError> {
    let body = take_json_body(response)?;
    if let Ok(envelope) =
        serde_json::from_slice::<ProductionEnvelope<ProductionGiftCardRedeemData>>(&body)
    {
        let data = envelope.into_data()?;
        let message = data.message.trim();
        if !valid_gift_card_text(message) {
            return Err(BusinessServiceError::InvalidResponse);
        }
        let template_name = data
            .template_name
            .map(|name| name.trim().to_owned())
            .filter(|name| !name.is_empty());
        if template_name
            .as_ref()
            .is_some_and(|name| !valid_gift_card_text(name))
        {
            return Err(BusinessServiceError::InvalidResponse);
        }
        let rewards_json = data.rewards.as_ref().and_then(reward_json_string);
        let _observed_invite_rewards = data.invite_rewards;
        return Ok(GiftCardRedeemResponse {
            schema_version: BUSINESS_API_SCHEMA_VERSION,
            message: message.to_owned(),
            template_name,
            rewards_json,
        });
    }
    serde_json::from_slice(&body).map_err(|_| BusinessServiceError::InvalidResponse)
}

fn decode_gift_card_history_response(
    response: BusinessCommandResponse,
) -> Result<GiftCardHistoryResponse, BusinessServiceError> {
    let body = take_json_body(response)?;
    let envelope: ProductionGiftCardHistoryEnvelope =
        serde_json::from_slice(&body).map_err(|_| BusinessServiceError::InvalidResponse)?;
    if envelope.data.len() > MAX_PUBLIC_GIFT_CARD_RECORDS
        || envelope.pagination.total < envelope.data.len() as u64
    {
        return Err(BusinessServiceError::InvalidResponse);
    }
    let _observed_pagination = (
        envelope.pagination.current_page,
        envelope.pagination.last_page,
        envelope.pagination.per_page,
    );
    let mut records = Vec::with_capacity(envelope.data.len());
    for record in envelope.data {
        let code = record.code.trim();
        if code.len() > 64 || code.chars().any(char::is_control) {
            return Err(BusinessServiceError::InvalidResponse);
        }
        let clean = |value: Option<String>| {
            value
                .map(|text| text.trim().to_owned())
                .filter(|text| !text.is_empty() && valid_gift_card_text(text))
        };
        let _observed_reward_details = (
            record.invite_rewards,
            record.multiplier_applied,
            record.rewards_given,
            record.template_type,
        );
        // The server emits a Unix timestamp integer; older payloads used a
        // formatted string. Accept both and normalize to text for display.
        let created_at = clean(record.created_at.and_then(|value| match value {
            Value::String(text) => Some(text),
            Value::Number(number) => Some(number.to_string()),
            _ => None,
        }));
        records.push(GiftCardHistoryRecord {
            record_id: record.id.to_string(),
            code: code.to_owned(),
            template_name: clean(record.template_name),
            template_type_name: clean(record.template_type_name),
            created_at,
        });
    }
    Ok(GiftCardHistoryResponse {
        schema_version: BUSINESS_API_SCHEMA_VERSION,
        records,
        total: SafeInteger::new(envelope.pagination.total)
            .ok_or(BusinessServiceError::InvalidResponse)?,
    })
}

fn is_safe_ticket_text(value: &str) -> bool {
    !value
        .chars()
        .any(|character| character.is_control() && !matches!(character, '\n' | '\r' | '\t'))
}

// Counted in characters, not bytes, to match the contract's `maxLength` and to
// stop CJK input from hitting a third of the advertised limit. Upstream stores
// the subject in a varchar(255) (counted in characters) and the message in a
// TEXT column, and imposes no length rule of its own, so this layer is what
// keeps the columns from overflowing.
const MAX_TICKET_SUBJECT_CHARS: usize = 200;
const MAX_TICKET_MESSAGE_CHARS: usize = 10_000;

fn valid_ticket_subject(value: &str) -> bool {
    !value.is_empty()
        && value.chars().count() <= MAX_TICKET_SUBJECT_CHARS
        && value.trim() == value
        && !value.chars().any(char::is_control)
}

fn valid_ticket_message(value: &str) -> bool {
    !value.is_empty()
        && value.chars().count() <= MAX_TICKET_MESSAGE_CHARS
        && value.trim() == value
        && is_safe_ticket_text(value)
}

fn production_unix_seconds(value: u64) -> Result<UnixMillis, BusinessServiceError> {
    value
        .checked_mul(1_000)
        .and_then(UnixMillis::new)
        .ok_or(BusinessServiceError::InvalidResponse)
}

fn production_optional_unix_seconds(
    value: Option<&Value>,
) -> Result<Option<UnixMillis>, BusinessServiceError> {
    let Some(value) = value else {
        return Ok(None);
    };
    let seconds = match value {
        Value::Null => return Ok(None),
        Value::Number(number) => number.as_u64(),
        Value::String(text) => text.parse().ok(),
        _ => None,
    }
    .ok_or(BusinessServiceError::InvalidResponse)?;
    production_unix_seconds(seconds).map(Some)
}

fn validate_production_config(config: &ProductionConfigData) -> Result<(), BusinessServiceError> {
    let binary_flags = [
        config.is_captcha,
        config.is_email_verify,
        config.is_invite_force,
        config.is_recaptcha,
    ];
    if binary_flags.into_iter().any(|value| value > 1)
        || config.app_description.len() > 16 * 1024
        || config.app_url.len() > 4 * 1024
        || config.captcha_type.len() > 64
        || config.email_whitelist_suffix.len() > 64
        || config
            .email_whitelist_suffix
            .iter()
            .any(|value| value.len() > 253 || !value.is_ascii())
        || !config.recaptcha_v3_score_threshold.is_finite()
        || !(0.0..=1.0).contains(&config.recaptcha_v3_score_threshold)
    {
        return Err(BusinessServiceError::InvalidResponse);
    }
    for value in [
        config.logo.as_deref(),
        config.recaptcha_site_key.as_deref(),
        config.recaptcha_v3_site_key.as_deref(),
        config.tos_url.as_deref(),
        config.turnstile_site_key.as_deref(),
    ]
    .into_iter()
    .flatten()
    {
        if value.len() > 4 * 1024 || value.chars().any(char::is_control) {
            return Err(BusinessServiceError::InvalidResponse);
        }
    }
    Ok(())
}

/// Normalizes the operator-configured email suffix whitelist into a form the
/// shell can render directly.
///
/// Xboard stores the whitelist either as an array or as a comma-separated
/// string (`Helper::getEmailSuffix`), and operators enter values inconsistently
/// -- with or without a leading `@`, in mixed case, with stray whitespace. The
/// values are already length- and ASCII-checked by `validate_production_config`;
/// here we canonicalize them and drop anything that cannot be a domain, so a
/// malformed entry degrades to a shorter list instead of failing startup.
fn normalize_email_suffixes(values: Vec<String>) -> Vec<String> {
    let mut suffixes: Vec<String> = Vec::with_capacity(values.len());
    for value in values {
        let candidate = value.trim().trim_start_matches('@').to_ascii_lowercase();
        if is_valid_email_suffix(&candidate) && !suffixes.iter().any(|kept| kept == &candidate) {
            suffixes.push(candidate);
        }
    }
    suffixes
}

/// Accepts a bare domain such as `gmail.com`: at least one dot, and only
/// characters that are legal in a hostname label.
fn is_valid_email_suffix(value: &str) -> bool {
    if value.is_empty() || value.len() > 253 || !value.contains('.') {
        return false;
    }
    value.split('.').all(|label| {
        !label.is_empty()
            && label.len() <= 63
            && !label.starts_with('-')
            && !label.ends_with('-')
            && label
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    })
}

fn validate_subscription_url(value: &str) -> Result<(), BusinessServiceError> {
    let url = Url::parse(value).map_err(|_| BusinessServiceError::InvalidResponse)?;
    if value.len() > 16 * 1024
        || url.scheme() != "https"
        || url.host_str().is_none()
        || url.port_or_known_default() != Some(443)
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
        || url.path().is_empty()
    {
        return Err(BusinessServiceError::InvalidResponse);
    }
    Ok(())
}

fn take_json_body(
    mut response: BusinessCommandResponse,
) -> Result<Zeroizing<Vec<u8>>, BusinessServiceError> {
    if !is_json_content_type(response.content_type()) {
        return Err(BusinessServiceError::InvalidContentType);
    }
    let body = Zeroizing::new(response.take_body());
    if body.is_empty() {
        return Err(BusinessServiceError::InvalidResponse);
    }
    Ok(body)
}

fn is_json_content_type(content_type: &str) -> bool {
    let mut parts = content_type.split(';');
    if !parts
        .next()
        .is_some_and(|value| value.trim().eq_ignore_ascii_case("application/json"))
    {
        return false;
    }
    let mut saw_charset = false;
    for parameter in parts {
        let parameter = parameter.trim();
        if saw_charset || !parameter.eq_ignore_ascii_case("charset=utf-8") {
            return false;
        }
        saw_charset = true;
    }
    true
}

fn validate_config_url(
    value: &str,
    allowed_hosts: &[&str],
    origin_only: bool,
) -> Result<(), BusinessServiceError> {
    let url = parse_config_url(value, origin_only)?;
    if !url
        .host_str()
        .is_some_and(|host| allowed_hosts.contains(&host))
    {
        return Err(BusinessServiceError::RejectedConfigUrl);
    }
    Ok(())
}

fn parse_config_url(value: &str, origin_only: bool) -> Result<Url, BusinessServiceError> {
    let url = Url::parse(value).map_err(|_| BusinessServiceError::RejectedConfigUrl)?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || url.port_or_known_default() != Some(443)
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || (origin_only && url.path() != "/")
    {
        return Err(BusinessServiceError::RejectedConfigUrl);
    }
    Ok(url)
}

fn validate_email(email: &str) -> Result<(), BusinessServiceError> {
    if email.len() < 3
        || email.len() > MAX_AUTH_EMAIL_BYTES
        || !email.is_ascii()
        || email.trim() != email
        || email.bytes().any(|byte| byte.is_ascii_control())
    {
        return Err(BusinessServiceError::InvalidEmail);
    }
    let Some((local, domain)) = email.split_once('@') else {
        return Err(BusinessServiceError::InvalidEmail);
    };
    if domain.contains('@')
        || local.is_empty()
        || local.len() > 64
        || domain.is_empty()
        || !domain.contains('.')
        || local.starts_with('.')
        || local.ends_with('.')
        || local.contains("..")
        || !local
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b".!#$%&'*+/=?^_`{|}~-".contains(&byte))
        || domain.split('.').any(|label| {
            label.is_empty()
                || label.starts_with('-')
                || label.ends_with('-')
                || !label
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        })
    {
        return Err(BusinessServiceError::InvalidEmail);
    }
    Ok(())
}

fn validate_password(password: &str) -> Result<(), BusinessServiceError> {
    if !(MIN_AUTH_PASSWORD_BYTES..=MAX_AUTH_PASSWORD_BYTES).contains(&password.len())
        || password.chars().any(char::is_control)
    {
        return Err(BusinessServiceError::InvalidPassword);
    }
    Ok(())
}

fn valid_email_verification_code(value: &str) -> bool {
    value.len() == 6 && value.bytes().all(|byte| byte.is_ascii_digit())
}

fn validate_invite_code(invite_code: &str) -> Result<(), BusinessServiceError> {
    if invite_code.is_empty()
        || invite_code.len() > MAX_INVITE_CODE_BYTES
        || !invite_code
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err(BusinessServiceError::InvalidInviteCode);
    }
    Ok(())
}

const fn is_offline_client_error(error: BusinessClientError) -> bool {
    matches!(
        error,
        BusinessClientError::Transport(
            BootstrapTransportError::Unavailable
                | BootstrapTransportError::Timeout
                | BootstrapTransportError::Cancelled
                | BootstrapTransportError::DnsFailure
                | BootstrapTransportError::TlsFailure
        )
    )
}

const fn map_client_error(error: BusinessClientError) -> ErrorCode {
    match error {
        BusinessClientError::InvalidRequest | BusinessClientError::RequestRejected => {
            ErrorCode::Validation
        }
        BusinessClientError::AuthenticationRequired | BusinessClientError::Unauthorized => {
            ErrorCode::Permission
        }
        BusinessClientError::SecretStore(SecretStoreError::PermissionDenied) => {
            ErrorCode::Permission
        }
        BusinessClientError::SecretStore(_) => ErrorCode::Internal,
        BusinessClientError::Transport(BootstrapTransportError::Unavailable) => {
            ErrorCode::Bootstrap
        }
        BusinessClientError::Transport(
            BootstrapTransportError::DnsFailure | BootstrapTransportError::TlsFailure,
        ) => ErrorCode::Network,
        BusinessClientError::Transport(BootstrapTransportError::Timeout) => ErrorCode::Timeout,
        BusinessClientError::Transport(BootstrapTransportError::Cancelled) => ErrorCode::Cancelled,
        BusinessClientError::Transport(
            BootstrapTransportError::InvalidRequest
            | BootstrapTransportError::InvalidResponse
            | BootstrapTransportError::ResponseTooLarge,
        )
        | BusinessClientError::RedirectDenied
        | BusinessClientError::RateLimited
        | BusinessClientError::ServiceUnavailable
        | BusinessClientError::InvalidResponse => ErrorCode::Service,
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn json_response(body: &str) -> BusinessCommandResponse {
        BusinessCommandResponse::for_test(200, "application/json", body.as_bytes().to_vec())
    }

    #[test]
    fn node_loads_decode_strict_server_shape() {
        let body = r#"{
            "status": "success",
            "message": "ok",
            "data": {
                "schema_version": 1,
                "generated_at": 1786428000,
                "ttl_seconds": 180,
                "nodes": [
                    {
                        "id": "xb-12",
                        "capacity_group": "m-3",
                        "load": 0.42,
                        "state": "idle",
                        "updated_at": 1786427970,
                        "selection_weight": 1.5
                    },
                    {
                        "id": "xb-13",
                        "capacity_group": "m-4",
                        "load": null,
                        "state": "unknown",
                        "updated_at": 1786427000,
                        "selection_weight": 1.0
                    }
                ]
            },
            "error": null
        }"#;
        let decoded = decode_node_loads_response(json_response(body))
            .expect("valid load payload must decode");
        assert_eq!(decoded.ttl_seconds, 180);
        assert_eq!(decoded.nodes.len(), 2);
        assert_eq!(decoded.nodes[0].capacity_group, "m-3");
        assert_eq!(decoded.nodes[0].load, Some(0.42));
        assert_eq!(decoded.nodes[1].state, NodeLoadState::Unknown);
    }

    #[test]
    fn node_loads_reject_duplicate_ids_and_state_mismatch() {
        let duplicate = r#"{
            "status":"success","message":"ok","error":null,
            "data":{"schema_version":1,"generated_at":1786428000,"ttl_seconds":180,"nodes":[
                {"id":"xb-12","capacity_group":"m-3","load":0.42,"state":"idle","updated_at":1786427970,"selection_weight":1.0},
                {"id":"xb-12","capacity_group":"m-4","load":0.50,"state":"normal","updated_at":1786427970,"selection_weight":1.0}
            ]}
        }"#;
        assert!(decode_node_loads_response(json_response(duplicate)).is_err());

        let mismatch = r#"{
            "status":"success","message":"ok","error":null,
            "data":{"schema_version":1,"generated_at":1786428000,"ttl_seconds":180,"nodes":[
                {"id":"xb-12","capacity_group":"m-3","load":0.42,"state":"unknown","updated_at":1786427970,"selection_weight":1.0}
            ]}
        }"#;
        assert!(decode_node_loads_response(json_response(mismatch)).is_err());
    }

    #[test]
    fn subscription_decode_tolerates_planless_account() {
        // Real Xboard payload for an account without a plan: `plan_id` is null,
        // `plan` is omitted entirely, and the reset/expiry columns are null.
        let body = r#"{
            "status": "success",
            "message": "ok",
            "data": {
                "d": 0,
                "device_limit": null,
                "email": "user@gmail.com",
                "expired_at": null,
                "next_reset_at": null,
                "plan_id": null,
                "reset_day": null,
                "speed_limit": null,
                "subscribe_url": "https://api.donghuyun.top/api/v1/client/subscribe?token=abc123",
                "token": "abc123",
                "transfer_enable": 0,
                "u": 0,
                "uuid": "9f1c2e10-0000-4000-8000-000000000001"
            },
            "error": null
        }"#;
        let wire = decode_subscription_response(json_response(body))
            .expect("plan-less server payload must decode");
        assert!(matches!(wire.status, SubscriptionStatus::None));
        assert!(wire.plan_id.is_none());
        assert!(wire.expires_at_unix_ms.is_none());
    }

    #[test]
    fn subscription_decode_active_account() {
        let body = r#"{
            "status": "success",
            "message": "ok",
            "data": {
                "d": 1073741824,
                "device_limit": 3,
                "email": "user@gmail.com",
                "expired_at": 1893456000,
                "next_reset_at": 1787904000,
                "plan": {"id": 5, "name": "basic", "transfer_enable": 100},
                "plan_id": 5,
                "reset_day": 12,
                "speed_limit": null,
                "subscribe_url": "https://api.donghuyun.top/api/v1/client/subscribe?token=abc123",
                "token": "abc123",
                "transfer_enable": 107374182400,
                "u": 0,
                "uuid": "9f1c2e10-0000-4000-8000-000000000001"
            },
            "error": null
        }"#;
        let wire = decode_subscription_response(json_response(body))
            .expect("active subscription must decode");
        assert!(matches!(wire.status, SubscriptionStatus::Active));
        assert_eq!(wire.plan_id.as_deref(), Some("5"));
        assert_eq!(wire.plan_name.as_deref(), Some("basic"));
        assert!(wire.expires_at_unix_ms.is_some());
    }

    #[test]
    fn subscription_decode_keeps_credential_when_plan_name_unusable() {
        // A blank/oversized/control-character name must only drop the name —
        // the subscription response also carries the data-plane credential.
        let body = r#"{
            "status": "success",
            "message": "ok",
            "data": {
                "d": 0,
                "device_limit": null,
                "email": "user@gmail.com",
                "expired_at": 1893456000,
                "next_reset_at": null,
                "plan": {"id": 5, "name": "   ", "transfer_enable": 100},
                "plan_id": 5,
                "reset_day": null,
                "speed_limit": null,
                "subscribe_url": "https://api.donghuyun.top/api/v1/client/subscribe?token=abc123",
                "token": "abc123",
                "transfer_enable": 107374182400,
                "u": 0,
                "uuid": "9f1c2e10-0000-4000-8000-000000000001"
            },
            "error": null
        }"#;
        let wire = decode_subscription_response(json_response(body))
            .expect("blank plan name must not fail the whole subscription");
        assert_eq!(wire.plan_id.as_deref(), Some("5"));
        assert!(wire.plan_name.is_none());
        assert!(!wire.subscription_credential.is_empty());
    }

    #[test]
    fn subscription_decode_rejects_missing_required_field() {
        let body = r#"{
            "status": "success",
            "message": "ok",
            "data": {
                "d": 0,
                "device_limit": null,
                "email": "user@gmail.com",
                "expired_at": null,
                "next_reset_at": null,
                "plan_id": null,
                "reset_day": null,
                "speed_limit": null,
                "subscribe_url": "https://api.donghuyun.top/api/v1/client/subscribe?token=abc123",
                "token": "abc123",
                "transfer_enable": 0,
                "u": 0
            },
            "error": null
        }"#;
        assert!(decode_subscription_response(json_response(body)).is_err());
    }

    #[test]
    fn account_decode_tolerates_nullable_server_fields() {
        // Xboard leaves expired_at/last_login_at/plan_id/remind_* null for
        // fresh accounts; those fields are only observed, never required.
        let body = r#"{
            "status": "success",
            "message": "ok",
            "data": {
                "avatar_url": "https://cdn.v2ex.com/gravatar/abc?s=64&d=identicon",
                "balance": 0,
                "banned": false,
                "commission_balance": 0,
                "commission_rate": null,
                "created_at": 1787904000,
                "discount": null,
                "email": "user@gmail.com",
                "expired_at": null,
                "last_login_at": null,
                "plan_id": null,
                "remind_expire": null,
                "remind_traffic": null,
                "telegram_id": null,
                "transfer_enable": 0,
                "uuid": "9f1c2e10-0000-4000-8000-000000000001"
            },
            "error": null
        }"#;
        let account = decode_account_response(json_response(body))
            .expect("nullable account fields must decode");
        assert!(matches!(account.user.status, AccountStatus::Active));
        assert_eq!(account.user.email, "user@gmail.com");
    }

    #[test]
    fn notices_decode_real_server_shape() {
        // The notice endpoint returns {data, total} WITHOUT the success
        // envelope, and each notice carries 8 fields with a boolean `show`.
        let body = r#"{
            "data": [
                {
                    "id": 1,
                    "title": "维护通知",
                    "content": "今晚升级",
                    "show": true,
                    "img_url": null,
                    "tags": [],
                    "created_at": 1787904000,
                    "updated_at": 1787904000,
                    "sort": null
                },
                {
                    "id": 2,
                    "title": "隐藏公告",
                    "content": "不展示",
                    "show": false,
                    "img_url": null,
                    "tags": [],
                    "created_at": 1787904000,
                    "updated_at": 1787904000,
                    "sort": 1
                }
            ],
            "total": 2
        }"#;
        let notices = decode_notices_response(json_response(body))
            .expect("real notice payload must decode");
        assert_eq!(notices.notices.len(), 1);
        assert_eq!(notices.notices[0].title, "维护通知");
    }

    #[test]
    fn plans_decode_real_server_shape() {
        // PlanResource emits boolean show/renew/sell, an int
        // reset_traffic_method, float-cent prices, a tags array, and a
        // translated string capacity_limit when sold out.
        let body = r#"{
            "status": "success",
            "message": "ok",
            "data": [
                {
                    "id": 5,
                    "group_id": 1,
                    "name": "基础套餐",
                    "tags": ["热门"],
                    "content": "<p>高速稳定线路</p><ul><li>支持多设备</li></ul>",
                    "month_price": 1999.0,
                    "quarter_price": null,
                    "half_year_price": null,
                    "year_price": 19999.0,
                    "two_year_price": null,
                    "three_year_price": null,
                    "onetime_price": null,
                    "reset_price": null,
                    "capacity_limit": "已售罄",
                    "transfer_enable": 100,
                    "speed_limit": null,
                    "device_limit": null,
                    "show": true,
                    "sell": true,
                    "renew": true,
                    "reset_traffic_method": 1,
                    "sort": 1,
                    "created_at": 1787904000,
                    "updated_at": 1787904000
                }
            ],
            "error": null
        }"#;
        let plans = decode_plans_response(json_response(body))
            .expect("real plan payload must decode");
        assert_eq!(plans.plans.len(), 2);
        assert_eq!(plans.plans[0].plan_id, "5:month_price");
        assert_eq!(
            plans.plans[0].description_html.as_deref(),
            Some("<p>高速稳定线路</p><ul><li>支持多设备</li></ul>")
        );
        assert_eq!(
            plans.plans[1].description_html,
            plans.plans[0].description_html
        );
    }

    #[test]
    fn plans_decode_empty_description_as_none() {
        let body = r#"{
            "status": "success",
            "message": "ok",
            "data": [
                {
                    "id": 5,
                    "group_id": 1,
                    "name": "基础套餐",
                    "tags": [],
                    "content": "   ",
                    "month_price": 1999.0,
                    "quarter_price": null,
                    "half_year_price": null,
                    "year_price": null,
                    "two_year_price": null,
                    "three_year_price": null,
                    "onetime_price": null,
                    "reset_price": null,
                    "capacity_limit": null,
                    "transfer_enable": 100,
                    "speed_limit": null,
                    "device_limit": null,
                    "show": true,
                    "sell": true,
                    "renew": true,
                    "reset_traffic_method": 1,
                    "sort": 1,
                    "created_at": 1787904000,
                    "updated_at": 1787904000
                }
            ],
            "error": null
        }"#;
        let plans = decode_plans_response(json_response(body))
            .expect("empty plan description must decode");
        assert_eq!(plans.plans.len(), 1);
        assert_eq!(plans.plans[0].description_html, None);
    }

    #[test]
    fn orders_decode_surplus_credit_field() {
        // The server renamed refund_amount to surplus_credit; strict decoding
        // must tolerate the new key.
        let body = r#"{
            "status": "success",
            "message": "ok",
            "data": [
                {
                    "id": 9,
                    "invite_user_id": null,
                    "user_id": 7,
                    "plan_id": 5,
                    "payment_id": null,
                    "coupon_id": null,
                    "commission_status": 0,
                    "commission_balance": 0,
                    "actual_commission_balance": null,
                    "trade_no": "ORD20260804000001",
                    "total_amount": 1999,
                    "handling_amount": null,
                    "discount_amount": null,
                    "surplus_amount": null,
                    "surplus_credit": null,
                    "balance_amount": null,
                    "paid_at": null,
                    "type": 1,
                    "status": 0,
                    "callback_no": null,
                    "surplus_order_ids": null,
                    "period": "month_price",
                    "plan": {"id": 5, "name": "基础套餐"},
                    "created_at": 1787904000,
                    "updated_at": 1787904000
                }
            ],
            "error": null
        }"#;
        let orders = decode_orders_response(json_response(body))
            .expect("order payload with surplus_credit must decode");
        assert_eq!(orders.orders.len(), 1);
    }

    #[test]
    fn order_detail_decode_payment_and_surplus_orders() {
        let body = r#"{
            "status": "success",
            "message": "ok",
            "data": {
                "id": 9,
                "invite_user_id": null,
                "user_id": 7,
                "plan_id": 5,
                "payment_id": 1,
                "coupon_id": null,
                "commission_status": 0,
                "commission_balance": 0,
                "actual_commission_balance": null,
                "trade_no": "ORD20260804000001",
                "total_amount": 1999,
                "handling_amount": null,
                "discount_amount": null,
                "surplus_amount": null,
                "surplus_credit": null,
                "balance_amount": null,
                "paid_at": 1787904100,
                "type": 1,
                "status": 3,
                "callback_no": null,
                "surplus_order_ids": [8],
                "period": "month_price",
                "plan": {"id": 5, "name": "基础套餐", "transfer_enable": 100},
                "payment": {"id": 1, "name": "支付宝", "payment": "alipay", "icon": null},
                "try_out_plan_id": 0,
                "surplus_orders": [],
                "created_at": 1787904000,
                "updated_at": 1787904100
            },
            "error": null
        }"#;
        let detail = decode_order_detail_response(json_response(body))
            .expect("order detail with payment must decode");
        assert_eq!(detail.order.order_id, "ORD20260804000001");
    }

    #[test]
    fn checkout_decode_raw_response_shapes() {
        // Free order: { "type": -1, "data": true } — no payment URL.
        let free = decode_checkout_order_response(
            json_response(r#"{"type": -1, "data": true}"#),
            "ORD1",
        )
                .expect("free checkout must decode");
        assert!(free.qr_code.is_none());

        let paid = decode_checkout_order_response(
            json_response(r#"{"type": 0, "data": "weixin://wxpay/bizpayurl?pr=abc"}"#),
            "ORD2",
        )
        .expect("QR checkout must decode");
        assert_eq!(
            paid.qr_code.as_deref(),
            Some("weixin://wxpay/bizpayurl?pr=abc")
        );

        let redirect = decode_checkout_order_response(
            json_response(r#"{"type": 1, "data": "https://pay.example/checkout/abc"}"#),
            "ORD3",
        );
        assert!(matches!(
            redirect,
            Err(BusinessServiceError::InvalidResponse)
        ));
    }

    #[test]
    fn active_sessions_decode_without_token_key() {
        // Sanctum hides the hashed token; the key is absent from every record.
        let body = r#"{
            "status": "success",
            "message": "ok",
            "data": [
                {
                    "id": 42,
                    "tokenable_type": "App\\Models\\User",
                    "tokenable_id": 7,
                    "name": "windows-desktop",
                    "abilities": ["*"],
                    "last_used_at": "2026-08-04T12:00:00.000000Z",
                    "created_at": "2026-08-01T08:00:00.000000Z",
                    "updated_at": "2026-08-04T12:00:00.000000Z",
                    "expires_at": "2027-08-01T08:00:00.000000Z"
                }
            ],
            "error": null
        }"#;
        let sessions = decode_active_sessions_response(json_response(body))
            .expect("sessions without token key must decode");
        assert_eq!(sessions.sessions.len(), 1);
        assert_eq!(sessions.sessions[0].session_id, "42");
    }

    #[test]
    fn invitation_decode_bool_status_and_five_stat_elements() {
        // InviteCodeResource casts status to boolean and the stat array has
        // five elements: [registered, confirmed, pending, rate, available].
        let body = r#"{
            "status": "success",
            "message": "ok",
            "data": {
                "codes": [
                    {
                        "code": "INVITE01",
                        "pv": 3,
                        "status": false,
                        "created_at": 1787904000,
                        "updated_at": 1787904000
                    }
                ],
                "stat": [10, 5000, 1200, 15, 4800]
            },
            "error": null
        }"#;
        let center = decode_invitation_center_response(json_response(body))
            .expect("invitation payload must decode");
        assert_eq!(center.codes.len(), 1);
        assert!(matches!(
            center.codes[0].status,
            InvitationCodeStatus::Available
        ));
    }

    #[test]
    fn gift_card_history_decode_unix_created_at() {
        // History records emit created_at as a Unix integer, not a string.
        let body = r#"{
            "data": [
                {
                    "id": 3,
                    "code": "ABCD1234****",
                    "template_name": "新人礼包",
                    "template_type": "1",
                    "template_type_name": "余额卡",
                    "rewards_given": {"balance": 1000},
                    "invite_rewards": null,
                    "multiplier_applied": 1.0,
                    "created_at": 1787904000
                }
            ],
            "pagination": {"current_page": 1, "last_page": 1, "per_page": 15, "total": 1}
        }"#;
        let history = decode_gift_card_history_response(json_response(body))
            .expect("history with unix created_at must decode");
        assert_eq!(history.records.len(), 1);
        assert_eq!(history.records[0].created_at.as_deref(), Some("1787904000"));
    }

    #[test]
    fn email_suffixes_are_canonicalized_and_deduplicated() {
        let normalized = normalize_email_suffixes(vec![
            "@gmail.com".to_owned(),
            "  QQ.com  ".to_owned(),
            "gmail.com".to_owned(),
            "@163.COM".to_owned(),
        ]);
        assert_eq!(normalized, vec!["gmail.com", "qq.com", "163.com"]);
    }

    #[test]
    fn malformed_email_suffixes_are_dropped_without_failing() {
        let normalized = normalize_email_suffixes(vec![
            "gmail.com".to_owned(),
            String::new(),
            "@".to_owned(),
            "localhost".to_owned(),
            "double..dot.com".to_owned(),
            "-leading.com".to_owned(),
            "trailing-.com".to_owned(),
            "has space.com".to_owned(),
            "under_score.com".to_owned(),
        ]);
        assert_eq!(normalized, vec!["gmail.com"]);
    }

    #[test]
    fn empty_email_whitelist_stays_empty() {
        assert!(normalize_email_suffixes(Vec::new()).is_empty());
    }

    /// The exact envelope Xboard's `ApiResponse::success` emits for
    /// `/guest/comm/config`, carrying a populated whitelist.
    fn production_config_body(email_whitelist_suffix: &str) -> String {
        format!(
            r#"{{
            "status": "success",
            "message": "ok",
            "data": {{
                "tos_url": null,
                "is_email_verify": 1,
                "is_invite_force": 0,
                "email_whitelist_suffix": {email_whitelist_suffix},
                "is_captcha": 0,
                "captcha_type": "recaptcha",
                "recaptcha_site_key": null,
                "recaptcha_v3_site_key": null,
                "recaptcha_v3_score_threshold": 0.5,
                "turnstile_site_key": null,
                "app_description": "",
                "app_url": "https://example.com",
                "logo": null,
                "is_recaptcha": 0
            }},
            "error": null
        }}"#
        )
    }

    #[test]
    fn production_config_decodes_email_whitelist() {
        let body = production_config_body(r#"["gmail.com", "@QQ.com", "163.com"]"#);
        let decoded = decode_config_response(json_response(&body))
            .expect("a populated whitelist must decode");
        let DecodedConfig::Production(config) = decoded else {
            panic!("an Xboard success envelope must decode as production");
        };
        assert_eq!(
            normalize_email_suffixes(config.email_whitelist_suffix),
            vec!["gmail.com", "qq.com", "163.com"],
            "the whitelist must survive decoding and normalization"
        );
    }

    /// Xboard returns `[]` when the operator has not enabled the whitelist, and
    /// the shell treats an empty list as "any suffix is accepted".
    #[test]
    fn production_config_decodes_disabled_whitelist_as_empty() {
        let body = production_config_body("[]");
        let decoded =
            decode_config_response(json_response(&body)).expect("an empty whitelist must decode");
        let DecodedConfig::Production(config) = decoded else {
            panic!("an Xboard success envelope must decode as production");
        };
        assert!(config.email_whitelist_suffix.is_empty());
    }
}

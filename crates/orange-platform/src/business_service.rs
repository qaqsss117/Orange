use std::{
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
    CreateOrderResponse, CreatePaymentRequest, CreateTicketRequest, CurrencyCode, ErrorCode,
    InvitationCenterResponse, InvitationCode, InvitationCodeStatus, InvitationStats, LoginRequest,
    Money, OrderDetail, OrderDetailResponse, OrderStatus, OrderSummary, OrdersResponse,
    PaymentMethod, PaymentMethodsResponse, PaymentPublicResponse, PaymentStatus,
    PaymentWireResponse, Plan, PlansResponse, RegisterRequest, ReplyTicketRequest, SafeInteger,
    SubscriptionPublicResponse, SubscriptionStatus, SubscriptionWireResponse, Ticket, TicketDetail,
    TicketDetailResponse, TicketMessage, TicketStatus, TicketsResponse, UnixMillis,
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
pub const MAX_PUBLIC_INVITATION_CODES: usize = 256;
pub const MAX_PUBLIC_TICKETS: usize = 256;
pub const MAX_PUBLIC_TICKET_MESSAGES: usize = 256;

const GIB_BYTES: u64 = 1024 * 1024 * 1024;

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
    expired_at: u64,
    last_login_at: u64,
    plan_id: u64,
    remind_expire: bool,
    remind_traffic: bool,
    telegram_id: Option<u64>,
    transfer_enable: u64,
    uuid: String,
}

#[derive(Deserialize, Zeroize, ZeroizeOnDrop)]
#[serde(deny_unknown_fields)]
struct ProductionSubscriptionData {
    #[zeroize(skip)]
    d: u64,
    #[zeroize(skip)]
    device_limit: u64,
    email: String,
    #[zeroize(skip)]
    expired_at: u64,
    #[zeroize(skip)]
    next_reset_at: u64,
    #[zeroize(skip)]
    plan: Map<String, Value>,
    #[zeroize(skip)]
    plan_id: u64,
    #[zeroize(skip)]
    reset_day: u64,
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
struct ProductionPlanData {
    capacity_limit: Option<u64>,
    content: Option<String>,
    created_at: Option<u64>,
    device_limit: Option<u64>,
    group_id: Option<u64>,
    half_year_price: Option<u64>,
    id: u64,
    month_price: Option<u64>,
    name: String,
    onetime_price: Option<u64>,
    quarter_price: Option<u64>,
    renew: Option<u64>,
    reset_price: Option<u64>,
    reset_traffic_method: Option<String>,
    show: Option<u64>,
    sort: Option<u64>,
    speed_limit: Option<u64>,
    three_year_price: Option<u64>,
    transfer_enable: Option<u64>,
    two_year_price: Option<u64>,
    updated_at: Option<u64>,
    year_price: Option<u64>,
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
    payment_id: Option<Value>,
    period: Option<String>,
    plan: Option<Map<String, Value>>,
    plan_id: Option<u64>,
    refund_amount: Option<Value>,
    site_id: Option<Value>,
    status: Option<u64>,
    surplus_amount: Option<Value>,
    surplus_order_ids: Option<Value>,
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
    coupon_code: &'static str,
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
    status: Option<u64>,
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
struct ProductionCheckoutOrderRequest<'a> {
    trade_no: &'a str,
    method: u64,
}

#[derive(Serialize)]
struct ProductionLoginRequest<'a> {
    email: &'a str,
    password: &'a str,
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
    InviteRequired,
    InvalidInviteCode,
    InvalidPlan,
    RegistrationUnavailable,
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
            Self::InviteRequired => "business-invite-required",
            Self::InvalidInviteCode => "business-invalid-invite-code",
            Self::InvalidPlan => "business-invalid-plan",
            Self::RegistrationUnavailable => "business-registration-unavailable",
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
            | Self::InviteRequired
            | Self::InvalidInviteCode
            | Self::InvalidPlan => ErrorCode::Validation,
            Self::RegistrationUnavailable => ErrorCode::Service,
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

pub struct PaymentCheckout {
    public_response: PaymentPublicResponse,
    payment_url: Zeroizing<String>,
}

impl PaymentCheckout {
    pub fn with_payment_url<R>(&self, consume: impl FnOnce(&str) -> R) -> R {
        consume(self.payment_url.as_str())
    }

    pub fn into_public_response(self) -> PaymentPublicResponse {
        self.public_response
    }
}

impl fmt::Debug for PaymentCheckout {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PaymentCheckout")
            .field("public_response", &self.public_response)
            .field("payment_url_bytes", &self.payment_url.len())
            .finish()
    }
}

#[derive(Clone)]
struct BusinessState {
    config: Option<ConfigResponse>,
    production_backend: bool,
    session: AuthSessionResponse,
    subscription: Option<SubscriptionPublicResponse>,
}

impl Default for BusinessState {
    fn default() -> Self {
        Self {
            config: None,
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

        let (config, production_backend) = match self.fetch_config() {
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

    pub fn register(
        &self,
        request: RegisterRequest,
    ) -> Result<AuthPublicResponse, BusinessServiceError> {
        validate_email(&request.email)?;
        validate_password(&request.password)?;
        if let Some(invite_code) = request.invite_code.as_deref() {
            validate_invite_code(invite_code)?;
        }
        let state = lock(&self.state);
        let registration_requires_invite = state
            .config
            .as_ref()
            .ok_or(BusinessServiceError::NotInitialized)?
            .registration_requires_invite;
        if state.production_backend {
            return Err(BusinessServiceError::RegistrationUnavailable);
        }
        drop(state);
        if registration_requires_invite && request.invite_code.is_none() {
            return Err(BusinessServiceError::InviteRequired);
        }
        let _operation = self.acquire_operation()?;
        self.authenticate(BusinessCommand::Register, &request)
    }

    pub fn session(&self) -> AuthSessionResponse {
        lock(&self.state).session.clone()
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

    pub fn refresh_subscription(&self) -> Result<SubscriptionPublicResponse, BusinessServiceError> {
        self.require_authenticated()?;
        let _operation = self.acquire_operation()?;
        let response = self.execute_authenticated(BusinessCommand::Subscription)?;
        let mut wire = decode_subscription_response(response)?;
        let credential = std::mem::take(&mut wire.subscription_credential);
        let mut public = SubscriptionPublicResponse {
            schema_version: wire.schema_version,
            status: wire.status,
            plan_id: std::mem::take(&mut wire.plan_id),
            expires_at_unix_ms: wire.expires_at_unix_ms,
            used_bytes: wire.used_bytes,
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
    ) -> Result<PaymentCheckout, BusinessServiceError> {
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
        let payment_url = wire
            .with_payment_url(|value| value.map(str::to_owned))
            .filter(|value| !value.is_empty())
            .ok_or(BusinessServiceError::InvalidResponse)?;
        let target_host = self.validate_payment_target(&payment_url)?;
        Ok(PaymentCheckout {
            public_response: PaymentPublicResponse {
                schema_version: BUSINESS_API_SCHEMA_VERSION,
                order_id: request.order_id,
                status: wire.status,
                available: true,
                target_host: Some(target_host),
                expires_at_unix_ms: wire.expires_at_unix_ms,
            },
            payment_url: Zeroizing::new(payment_url),
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
                coupon_code: "",
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

    fn fetch_config(&self) -> Result<(ConfigResponse, bool), BusinessServiceError> {
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
                    },
                    false,
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
                    },
                    true,
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

    fn validate_payment_target(&self, value: &str) -> Result<String, BusinessServiceError> {
        let url = Url::parse(value).map_err(|_| BusinessServiceError::InvalidResponse)?;
        if value.len() > 16 * 1024
            || url.scheme() != "https"
            || url.port_or_known_default() != Some(443)
            || !url.username().is_empty()
            || url.password().is_some()
            || url.fragment().is_some()
            || url.path().is_empty()
        {
            return Err(BusinessServiceError::InvalidResponse);
        }
        let host = url
            .host_str()
            .ok_or(BusinessServiceError::InvalidResponse)?
            .to_ascii_lowercase();
        let production_backend = lock(&self.state).production_backend;
        let allowed = if production_backend {
            self.client.is_control_api_host_allowed(&host)?
        } else {
            DEVELOPMENT_PAYMENT_URL_HOSTS.contains(&host.as_str())
        };
        if !allowed {
            return Err(BusinessServiceError::InvalidResponse);
        }
        Ok(host)
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
        let currency = CurrencyCode::new("CNY").ok_or(BusinessServiceError::InvalidResponse)?;
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
        let total =
            SafeInteger::new(data.transfer_enable).ok_or(BusinessServiceError::InvalidResponse)?;
        let expires_at_unix_ms = match data.expired_at {
            0 => None,
            seconds => Some(
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
            || data.plan.is_empty()
        {
            return Err(BusinessServiceError::InvalidResponse);
        }
        let _observed_metadata = (
            data.device_limit,
            data.next_reset_at,
            data.reset_day,
            data.speed_limit,
        );
        return Ok(SubscriptionWireResponse {
            schema_version: BUSINESS_API_SCHEMA_VERSION,
            status: if data.plan_id == 0 {
                SubscriptionStatus::None
            } else {
                SubscriptionStatus::Active
            },
            plan_id: (data.plan_id != 0).then(|| data.plan_id.to_string()),
            expires_at_unix_ms,
            used_bytes: used,
            total_bytes: Some(total),
            subscription_credential: std::mem::take(&mut data.subscribe_url),
        });
    }
    serde_json::from_slice(&body).map_err(|_| BusinessServiceError::InvalidResponse)
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
        let currency = CurrencyCode::new("CNY").ok_or(BusinessServiceError::InvalidResponse)?;
        let mut plans = Vec::new();
        for data in source {
            if data.id == 0 || data.show == Some(0) {
                continue;
            }
            let name = data.name.trim();
            if name.is_empty() || name.len() > 256 || name.chars().any(char::is_control) {
                return Err(BusinessServiceError::InvalidResponse);
            }
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
                let Some(price) = price.filter(|price| *price > 0) else {
                    continue;
                };
                if plans.len() >= MAX_PUBLIC_PLANS {
                    return Err(BusinessServiceError::InvalidResponse);
                }
                plans.push(Plan {
                    plan_id: format!("{}:{period}", data.id),
                    name: name.to_owned(),
                    price: Money {
                        minor_units: SafeInteger::new(price)
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
                data.content,
                data.created_at,
                data.device_limit,
                data.group_id,
                data.renew,
                data.reset_price,
                data.reset_traffic_method,
                data.sort,
                data.speed_limit,
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
        let currency = CurrencyCode::new("CNY").ok_or(BusinessServiceError::InvalidResponse)?;
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
                data.payment_id,
                data.refund_amount,
                data.site_id,
                data.surplus_amount,
                data.surplus_order_ids,
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
        let currency = CurrencyCode::new("CNY").ok_or(BusinessServiceError::InvalidResponse)?;
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

fn decode_checkout_order_response(
    response: BusinessCommandResponse,
    order_id: &str,
) -> Result<PaymentWireResponse, BusinessServiceError> {
    let body = take_json_body(response)?;
    if let Ok(envelope) = serde_json::from_slice::<ProductionEnvelope<String>>(&body) {
        let payment_url = envelope.into_data()?;
        if payment_url.is_empty() {
            return Err(BusinessServiceError::InvalidResponse);
        }
        return Ok(PaymentWireResponse {
            schema_version: BUSINESS_API_SCHEMA_VERSION,
            order_id: order_id.to_owned(),
            status: PaymentStatus::Ready,
            payment_url: Some(payment_url),
            expires_at_unix_ms: None,
        });
    }
    serde_json::from_slice(&body).map_err(|_| BusinessServiceError::InvalidResponse)
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
        let [
            registered_users,
            pending_commission,
            total_commission,
            commission_rate_percent,
        ] = stat.as_slice()
        else {
            return Err(BusinessServiceError::InvalidResponse);
        };
        let currency = CurrencyCode::new("CNY").ok_or(BusinessServiceError::InvalidResponse)?;
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
                Some(0) => InvitationCodeStatus::Available,
                Some(1) => InvitationCodeStatus::Used,
                Some(2) => InvitationCodeStatus::Disabled,
                _ => InvitationCodeStatus::Unknown,
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

fn is_safe_ticket_text(value: &str) -> bool {
    !value
        .chars()
        .any(|character| character.is_control() && !matches!(character, '\n' | '\r' | '\t'))
}

fn valid_ticket_subject(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value.trim() == value
        && !value.chars().any(char::is_control)
}

fn valid_ticket_message(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 4 * 1024
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
    let mut parts = email.split('@');
    let local = parts.next().unwrap_or_default();
    let domain = parts.next().unwrap_or_default();
    if parts.next().is_some()
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

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
    AuthWireResponse, BUSINESS_API_SCHEMA_VERSION, BusinessInitializationResponse, ConfigResponse,
    ConfigWireResponse, CurrencyCode, ErrorCode, LoginRequest, Money, RegisterRequest, SafeInteger,
    SubscriptionPublicResponse, SubscriptionStatus, SubscriptionWireResponse, UnixMillis,
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

const DEVELOPMENT_PAYMENT_URL_HOSTS: &[&str] = &["pay.orange.invalid"];
const DEVELOPMENT_SUPPORT_URL_HOSTS: &[&str] = &["support.orange.invalid"];
const DEVELOPMENT_BANNER_URL_HOSTS: &[&str] = &["assets.orange.invalid"];
const PRODUCTION_MINIMUM_SUPPORTED_VERSION: &str = "0.1.0";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProductionEnvelope<T> {
    data: T,
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
            | Self::InvalidInviteCode => ErrorCode::Validation,
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
        lock(&self.state).subscription.clone()
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

#[cfg(test)]
mod tests {
    use std::{
        collections::{HashMap, VecDeque},
        sync::{Condvar, Mutex, atomic::AtomicUsize},
        thread,
        time::Duration,
    };

    use orange_domain::{AuthSessionStatus, LoginRequest, RegisterRequest};
    use serde_json::{Value, json};
    use zeroize::Zeroizing;

    use super::*;
    use crate::{BootstrapTransportRequest, BootstrapTransportResponse, SecretKey};

    #[derive(Clone, Default)]
    struct MemorySecretBackend {
        values: Arc<Mutex<HashMap<SecretKey, Zeroizing<Vec<u8>>>>>,
        fail_store_once: Arc<Mutex<Option<SecretKey>>>,
        fail_delete_once: Arc<Mutex<Option<SecretKey>>>,
        events: Arc<Mutex<Vec<&'static str>>>,
    }

    impl MemorySecretBackend {
        fn with_authentication() -> Self {
            let backend = Self::default();
            lock(&backend.values).extend([
                (
                    SecretKey::AccessToken,
                    Zeroizing::new(b"old-access".to_vec()),
                ),
                (
                    SecretKey::RefreshToken,
                    Zeroizing::new(b"old-refresh".to_vec()),
                ),
                (
                    SecretKey::SubscriptionCredential,
                    Zeroizing::new(b"old-subscription".to_vec()),
                ),
            ]);
            backend
        }

        fn value(&self, key: SecretKey) -> Option<Vec<u8>> {
            lock(&self.values).get(&key).map(|value| value.to_vec())
        }

        fn is_empty(&self) -> bool {
            lock(&self.values).is_empty()
        }

        fn clear_events(&self) {
            lock(&self.events).clear();
        }
    }

    impl SecretStoreBackend for MemorySecretBackend {
        fn store(&self, key: SecretKey, value: &[u8]) -> Result<(), SecretStoreError> {
            if *lock(&self.fail_store_once) == Some(key) {
                *lock(&self.fail_store_once) = None;
                return Err(SecretStoreError::StorageFailure);
            }
            lock(&self.values).insert(key, Zeroizing::new(value.to_vec()));
            Ok(())
        }

        fn load(&self, key: SecretKey) -> Result<Option<SecretValue>, SecretStoreError> {
            lock(&self.values)
                .get(&key)
                .map(|value| SecretValue::new(value.to_vec()))
                .transpose()
        }

        fn delete(&self, key: SecretKey) -> Result<(), SecretStoreError> {
            lock(&self.events).push(match key {
                SecretKey::AccessToken => "delete-access",
                SecretKey::RefreshToken => "delete-refresh",
                SecretKey::SubscriptionCredential => "delete-subscription",
            });
            if *lock(&self.fail_delete_once) == Some(key) {
                *lock(&self.fail_delete_once) = None;
                return Err(SecretStoreError::StorageFailure);
            }
            lock(&self.values).remove(&key);
            Ok(())
        }
    }

    #[derive(Debug, Clone)]
    enum MockOutcome {
        Response {
            status: u16,
            content_type: &'static str,
            body: Vec<u8>,
        },
        Error(BootstrapTransportError),
    }

    impl MockOutcome {
        fn json(status: u16, value: Value) -> Self {
            Self::Response {
                status,
                content_type: "application/json; charset=utf-8",
                body: serde_json::to_vec(&value).unwrap(),
            }
        }
    }

    #[derive(Default)]
    struct BlockingGate {
        entered: (Mutex<bool>, Condvar),
        released: (Mutex<bool>, Condvar),
    }

    impl BlockingGate {
        fn block(&self) {
            let mut entered = lock(&self.entered.0);
            *entered = true;
            self.entered.1.notify_all();
            drop(entered);
            let mut released = lock(&self.released.0);
            while !*released {
                released = self
                    .released
                    .1
                    .wait(released)
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
            }
        }

        fn wait_until_entered(&self) {
            let entered = lock(&self.entered.0);
            let (entered, outcome) = self
                .entered
                .1
                .wait_timeout_while(entered, Duration::from_secs(2), |entered| !*entered)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            assert!(*entered && !outcome.timed_out());
        }

        fn release(&self) {
            *lock(&self.released.0) = true;
            self.released.1.notify_all();
        }
    }

    struct ScriptedTransport {
        ready: Result<(), BootstrapTransportError>,
        wait_calls: AtomicUsize,
        outcomes: Mutex<VecDeque<MockOutcome>>,
        commands: Mutex<Vec<BusinessCommand>>,
        block_command: Option<(BusinessCommand, Arc<BlockingGate>)>,
    }

    impl ScriptedTransport {
        fn new(outcomes: impl IntoIterator<Item = MockOutcome>) -> Self {
            Self {
                ready: Ok(()),
                wait_calls: AtomicUsize::new(0),
                outcomes: Mutex::new(outcomes.into_iter().collect()),
                commands: Mutex::new(Vec::new()),
                block_command: None,
            }
        }

        fn unavailable() -> Self {
            Self {
                ready: Err(BootstrapTransportError::Unavailable),
                ..Self::new([])
            }
        }

        fn with_login_gate(mut self, gate: Arc<BlockingGate>) -> Self {
            self.block_command = Some((BusinessCommand::Login, gate));
            self
        }

        fn with_command_gate(mut self, command: BusinessCommand, gate: Arc<BlockingGate>) -> Self {
            self.block_command = Some((command, gate));
            self
        }
    }

    impl BootstrapTransport for ScriptedTransport {
        fn wait_until_ready(&self) -> Result<(), BootstrapTransportError> {
            self.wait_calls.fetch_add(1, Ordering::Relaxed);
            self.ready
        }

        fn is_control_api_host_allowed(&self, host: &str) -> Result<bool, BootstrapTransportError> {
            Ok(host == "api.orange.invalid")
        }

        fn execute(
            &self,
            request: BootstrapTransportRequest<'_>,
        ) -> Result<BootstrapTransportResponse, BootstrapTransportError> {
            let command = request.route().command();
            lock(&self.commands).push(command);
            if let Some((blocked_command, gate)) = &self.block_command
                && command == *blocked_command
            {
                gate.block();
            }
            match lock(&self.outcomes)
                .pop_front()
                .expect("missing mock outcome")
            {
                MockOutcome::Response {
                    status,
                    content_type,
                    body,
                } => BootstrapTransportResponse::new(status, content_type, body),
                MockOutcome::Error(error) => Err(error),
            }
        }
    }

    #[derive(Debug, Clone, Copy)]
    struct FixedClock(u64);

    impl BusinessClock for FixedClock {
        fn now_unix_ms(&self) -> u64 {
            self.0
        }
    }

    struct MockLogoutDataPlane {
        events: Arc<Mutex<Vec<&'static str>>>,
        result: Result<(), PlatformVpnError>,
        gate: Option<Arc<BlockingGate>>,
    }

    impl MockLogoutDataPlane {
        fn ready(events: Arc<Mutex<Vec<&'static str>>>) -> Self {
            Self {
                events,
                result: Ok(()),
                gate: None,
            }
        }
    }

    impl LogoutDataPlane for MockLogoutDataPlane {
        fn stop_for_logout(&self) -> Result<(), PlatformVpnError> {
            lock(&self.events).push("stop-entered");
            if let Some(gate) = &self.gate {
                gate.block();
            }
            lock(&self.events).push("stop-complete");
            self.result
        }
    }

    type TestService = BusinessApiService<Arc<ScriptedTransport>, MemorySecretBackend, FixedClock>;

    fn service(
        transport: ScriptedTransport,
        backend: MemorySecretBackend,
    ) -> (Arc<TestService>, Arc<ScriptedTransport>) {
        let transport = Arc::new(transport);
        let client = Arc::new(BusinessCommandClient::new(Arc::clone(&transport), backend));
        (
            Arc::new(BusinessApiService::new(client, FixedClock(1_000))),
            transport,
        )
    }

    fn config(invite_required: bool) -> Value {
        json!({
            "schemaVersion": 1,
            "minimumSupportedVersion": "0.1.0",
            "maintenance": false,
            "notice": null,
            "registrationRequiresInvite": invite_required,
            "apiBaseUrl": "https://api.orange.invalid/",
            "paymentBaseUrl": "https://pay.orange.invalid/",
            "supportUrl": "https://support.orange.invalid/help",
            "bannerUrl": "https://assets.orange.invalid/banner.png"
        })
    }

    fn user(email: &str) -> Value {
        json!({
            "userId": "fixture-user",
            "email": email,
            "status": "active",
            "balance": { "minorUnits": 0, "currency": "CNY" }
        })
    }

    fn account(email: &str) -> Value {
        json!({ "schemaVersion": 1, "user": user(email) })
    }

    fn authentication(email: &str, expires_at: u64) -> Value {
        json!({
            "schemaVersion": 1,
            "credentials": {
                "accessToken": "new-access",
                "refreshToken": "new-refresh",
                "expiresAtUnixMs": expires_at
            },
            "user": user(email)
        })
    }

    fn subscription(
        status: &str,
        expires_at_unix_ms: Option<u64>,
        used_bytes: u64,
        total_bytes: Option<u64>,
        credential: &str,
    ) -> Value {
        json!({
            "schemaVersion": 1,
            "status": status,
            "planId": "fixture-plan",
            "expiresAtUnixMs": expires_at_unix_ms,
            "usedBytes": used_bytes,
            "totalBytes": total_bytes,
            "subscriptionCredential": credential
        })
    }

    fn production_envelope(data: Value) -> Value {
        json!({
            "data": data,
            "error": null,
            "message": "success",
            "status": "success"
        })
    }

    fn production_config(invite_required: u8) -> Value {
        production_envelope(json!({
            "app_description": "Orange",
            "app_url": "https://api.orange.invalid/",
            "captcha_type": "recaptcha",
            "email_whitelist_suffix": [],
            "is_captcha": 0,
            "is_email_verify": 0,
            "is_invite_force": invite_required,
            "is_recaptcha": 0,
            "logo": null,
            "recaptcha_site_key": null,
            "recaptcha_v3_score_threshold": 0.6,
            "recaptcha_v3_site_key": null,
            "tos_url": null,
            "turnstile_site_key": null
        }))
    }

    fn production_account(email: &str) -> Value {
        production_envelope(json!({
            "avatar_url": "https://assets.example.invalid/avatar.png",
            "balance": 25,
            "banned": false,
            "commission_balance": 0,
            "commission_rate": null,
            "created_at": 1,
            "discount": null,
            "email": email,
            "expired_at": 2,
            "last_login_at": 1,
            "plan_id": 7,
            "remind_expire": true,
            "remind_traffic": true,
            "telegram_id": null,
            "transfer_enable": 1_000,
            "uuid": "01234567-89ab-cdef-0123-456789abcdef"
        }))
    }

    fn production_subscription() -> Value {
        production_envelope(json!({
            "d": 300,
            "device_limit": 3,
            "email": "member@example.invalid",
            "expired_at": 2,
            "next_reset_at": 3,
            "plan": { "id": 7 },
            "plan_id": 7,
            "reset_day": 1,
            "speed_limit": null,
            "subscribe_url": "https://subscription.example.invalid/api/v1/client/subscribe?token=redacted-fixture",
            "token": "subscription-token-fixture",
            "transfer_enable": 1_000,
            "u": 100,
            "uuid": "01234567-89ab-cdef-0123-456789abcdef"
        }))
    }

    fn login_request() -> LoginRequest {
        LoginRequest {
            schema_version: 1,
            email: "member@example.invalid".to_owned(),
            password: "password-123".to_owned(),
        }
    }

    #[test]
    fn new_install_waits_for_ready_fetches_config_and_stays_signed_out() {
        let (service, transport) = service(
            ScriptedTransport::new([MockOutcome::json(200, config(true))]),
            MemorySecretBackend::default(),
        );
        let initialized = service.initialize().unwrap();
        assert_eq!(initialized.session.status, AuthSessionStatus::SignedOut);
        assert!(initialized.config.registration_requires_invite);
        assert_eq!(transport.wait_calls.load(Ordering::Relaxed), 1);
        assert_eq!(*lock(&transport.commands), vec![BusinessCommand::Config]);
    }

    #[test]
    fn valid_existing_authentication_is_checked_with_the_fixed_account_route() {
        let backend = MemorySecretBackend::with_authentication();
        let (service, transport) = service(
            ScriptedTransport::new([
                MockOutcome::json(200, config(false)),
                MockOutcome::json(200, account("member@example.invalid")),
            ]),
            backend,
        );
        let initialized = service.initialize().unwrap();
        assert_eq!(initialized.session.status, AuthSessionStatus::Authenticated);
        assert_eq!(
            *lock(&transport.commands),
            vec![BusinessCommand::Config, BusinessCommand::Account]
        );
    }

    #[test]
    fn expired_existing_authentication_is_cleared_after_account_unauthorized() {
        let backend = MemorySecretBackend::with_authentication();
        let inspection = backend.clone();
        let (service, _) = service(
            ScriptedTransport::new([
                MockOutcome::json(200, config(false)),
                MockOutcome::json(401, json!({ "error": "expired" })),
            ]),
            backend,
        );
        let initialized = service.initialize().unwrap();
        assert_eq!(initialized.session.status, AuthSessionStatus::SignedOut);
        assert!(inspection.is_empty());
    }

    #[test]
    fn unavailable_bootstrap_preserves_complete_authentication_as_unverified() {
        let backend = MemorySecretBackend::with_authentication();
        let inspection = backend.clone();
        let (service, transport) = service(ScriptedTransport::unavailable(), backend);
        assert_eq!(
            service.initialize().unwrap_err(),
            BusinessServiceError::Client(BusinessClientError::Transport(
                BootstrapTransportError::Unavailable
            ))
        );
        assert_eq!(service.session().status, AuthSessionStatus::Unverified);
        assert!(inspection.value(SecretKey::AccessToken).is_some());
        assert!(lock(&transport.commands).is_empty());
    }

    #[test]
    fn offline_account_check_preserves_credentials_and_returns_unverified_session() {
        let backend = MemorySecretBackend::with_authentication();
        let inspection = backend.clone();
        let (service, _) = service(
            ScriptedTransport::new([
                MockOutcome::json(200, config(false)),
                MockOutcome::Error(BootstrapTransportError::DnsFailure),
            ]),
            backend,
        );
        let initialized = service.initialize().unwrap();
        assert_eq!(initialized.session.status, AuthSessionStatus::Unverified);
        assert!(inspection.value(SecretKey::RefreshToken).is_some());
    }

    #[test]
    fn successful_login_stores_only_native_credentials_and_returns_public_user() {
        let backend = MemorySecretBackend::default();
        let inspection = backend.clone();
        let (service, _) = service(
            ScriptedTransport::new([
                MockOutcome::json(200, config(false)),
                MockOutcome::json(200, authentication("member@example.invalid", 2_000)),
            ]),
            backend,
        );
        service.initialize().unwrap();
        let response = service.login(login_request()).unwrap();
        assert!(response.authenticated);
        assert_eq!(
            inspection.value(SecretKey::AccessToken).unwrap(),
            b"new-access"
        );
        let public = serde_json::to_value(response).unwrap();
        assert!(public.get("credentials").is_none());
        assert!(
            inspection
                .value(SecretKey::SubscriptionCredential)
                .is_none()
        );
    }

    #[test]
    fn production_v2board_login_account_and_subscription_are_adapted_without_secret_exposure() {
        let backend = MemorySecretBackend::default();
        let inspection = backend.clone();
        let (service, transport) = service(
            ScriptedTransport::new([
                MockOutcome::json(200, production_config(0)),
                MockOutcome::json(
                    200,
                    production_envelope(json!({
                        "auth_data": "Bearer production-access-fixture",
                        "is_admin": false,
                        "token": "production-refresh-fixture"
                    })),
                ),
                MockOutcome::json(200, production_account("member@example.invalid")),
                MockOutcome::json(200, production_subscription()),
            ]),
            backend,
        );

        let initialized = service.initialize().unwrap();
        assert_eq!(initialized.session.status, AuthSessionStatus::SignedOut);
        assert!(!initialized.config.registration_requires_invite);
        assert_eq!(
            service
                .register(RegisterRequest {
                    schema_version: BUSINESS_API_SCHEMA_VERSION,
                    email: "new-member@example.invalid".to_owned(),
                    password: "password-123".to_owned(),
                    invite_code: None,
                })
                .unwrap_err(),
            BusinessServiceError::RegistrationUnavailable
        );
        assert_eq!(*lock(&transport.commands), vec![BusinessCommand::Config]);
        let authenticated = service.login(login_request()).unwrap();
        assert_eq!(authenticated.user.email, "member@example.invalid");
        assert_eq!(
            inspection.value(SecretKey::AccessToken).unwrap(),
            b"production-access-fixture"
        );
        assert_eq!(
            inspection.value(SecretKey::RefreshToken).unwrap(),
            b"production-refresh-fixture"
        );

        let subscription = service.refresh_subscription().unwrap();
        assert_eq!(subscription.status, SubscriptionStatus::Active);
        assert_eq!(subscription.used_bytes.get(), 400);
        assert_eq!(subscription.remaining_bytes().unwrap().get(), 600);
        assert!(
            serde_json::to_string(&subscription)
                .unwrap()
                .find("subscribe")
                .is_none()
        );
        assert_eq!(
            *lock(&transport.commands),
            vec![
                BusinessCommand::Config,
                BusinessCommand::Login,
                BusinessCommand::Account,
                BusinessCommand::Subscription,
            ]
        );
    }

    #[test]
    fn production_bearer_normalization_accepts_only_bare_or_exact_prefixed_tokens() {
        assert_eq!(
            normalize_production_bearer("access-token.fixture".to_owned()).unwrap(),
            "access-token.fixture"
        );
        assert_eq!(
            normalize_production_bearer("Bearer access-token.fixture".to_owned()).unwrap(),
            "access-token.fixture"
        );
        for invalid in [
            "",
            "Bearer ",
            "bearer access-token.fixture",
            "Bearer  access-token.fixture",
            "Bearer access token",
            "Bearer access-token\r\nfixture",
        ] {
            assert_eq!(
                normalize_production_bearer(invalid.to_owned()),
                Err(BusinessServiceError::InvalidResponse)
            );
        }
    }

    #[test]
    fn production_envelopes_reject_unknown_fields_and_unsafe_subscription_urls() {
        let mut config = production_config(0);
        config["data"]["unexpected"] = json!(true);
        let (invalid_config_service, _) = service(
            ScriptedTransport::new([MockOutcome::json(200, config)]),
            MemorySecretBackend::default(),
        );
        assert_eq!(
            invalid_config_service.initialize().unwrap_err(),
            BusinessServiceError::InvalidResponse
        );

        let mut subscription = production_subscription();
        subscription["data"]["subscribe_url"] = json!("http://127.0.0.1/private");
        let (service, _) = service(
            ScriptedTransport::new([
                MockOutcome::json(200, production_config(0)),
                MockOutcome::json(200, production_account("member@example.invalid")),
                MockOutcome::json(200, subscription),
            ]),
            MemorySecretBackend::with_authentication(),
        );
        service.initialize().unwrap();
        assert_eq!(
            service.refresh_subscription().unwrap_err(),
            BusinessServiceError::InvalidResponse
        );
    }

    #[test]
    fn account_and_subscription_refresh_use_fixed_routes_and_keep_credential_native() {
        let backend = MemorySecretBackend::with_authentication();
        let inspection = backend.clone();
        let (service, transport) = service(
            ScriptedTransport::new([
                MockOutcome::json(200, config(false)),
                MockOutcome::json(200, account("member@example.invalid")),
                MockOutcome::json(200, account("updated@example.invalid")),
                MockOutcome::json(
                    200,
                    subscription("active", Some(2_000), 400, Some(1_000), "new-subscription"),
                ),
            ]),
            backend,
        );
        service.initialize().unwrap();

        let account = service.refresh_account().unwrap();
        assert_eq!(account.user.email, "updated@example.invalid");
        assert_eq!(
            service.session().user.unwrap().email,
            "updated@example.invalid"
        );

        let subscription = service.refresh_subscription().unwrap();
        assert_eq!(subscription.status, SubscriptionStatus::Active);
        assert_eq!(subscription.remaining_bytes().unwrap().get(), 600);
        assert_eq!(service.cached_subscription(), Some(subscription.clone()));
        assert_eq!(
            inspection.value(SecretKey::SubscriptionCredential).unwrap(),
            b"new-subscription"
        );
        let public = serde_json::to_value(subscription).unwrap();
        assert!(public.get("subscriptionCredential").is_none());
        assert_eq!(
            *lock(&transport.commands),
            vec![
                BusinessCommand::Config,
                BusinessCommand::Account,
                BusinessCommand::Account,
                BusinessCommand::Subscription,
            ]
        );
    }

    #[test]
    fn expired_and_exhausted_subscription_refresh_delete_stale_credentials() {
        for (response, expected) in [
            (
                subscription("active", Some(1_000), 1, Some(100), "expired-secret"),
                SubscriptionStatus::Expired,
            ),
            (
                subscription("active", Some(2_000), 100, Some(100), "exhausted-secret"),
                SubscriptionStatus::Exhausted,
            ),
        ] {
            let backend = MemorySecretBackend::with_authentication();
            let inspection = backend.clone();
            let (service, _) = service(
                ScriptedTransport::new([
                    MockOutcome::json(200, config(false)),
                    MockOutcome::json(200, account("member@example.invalid")),
                    MockOutcome::json(200, response),
                ]),
                backend,
            );
            service.initialize().unwrap();
            assert_eq!(service.refresh_subscription().unwrap().status, expected);
            assert!(
                inspection
                    .value(SecretKey::SubscriptionCredential)
                    .is_none()
            );
        }
    }

    #[test]
    fn subscription_unauthorized_clears_session_cache_and_every_user_secret() {
        let backend = MemorySecretBackend::with_authentication();
        let inspection = backend.clone();
        let (service, _) = service(
            ScriptedTransport::new([
                MockOutcome::json(200, config(false)),
                MockOutcome::json(200, account("member@example.invalid")),
                MockOutcome::json(
                    200,
                    subscription("trial", Some(2_000), 0, None, "trial-subscription"),
                ),
                MockOutcome::json(401, json!({ "error": "expired" })),
            ]),
            backend,
        );
        service.initialize().unwrap();
        service.refresh_subscription().unwrap();
        assert!(service.cached_subscription().is_some());

        assert_eq!(
            service.refresh_account().unwrap_err(),
            BusinessServiceError::Client(BusinessClientError::Unauthorized)
        );
        assert_eq!(service.session().status, AuthSessionStatus::SignedOut);
        assert!(service.cached_subscription().is_none());
        assert!(inspection.is_empty());
    }

    #[test]
    fn logout_stops_data_plane_before_clearing_secrets_and_session_cache() {
        let backend = MemorySecretBackend::with_authentication();
        let inspection = backend.clone();
        let events = Arc::clone(&inspection.events);
        let (service, _) = service(
            ScriptedTransport::new([
                MockOutcome::json(200, config(false)),
                MockOutcome::json(200, account("member@example.invalid")),
                MockOutcome::json(
                    200,
                    subscription("active", Some(2_000), 0, None, "new-subscription"),
                ),
            ]),
            backend,
        );
        service.initialize().unwrap();
        service.refresh_subscription().unwrap();
        inspection.clear_events();

        let response = service
            .logout(&MockLogoutDataPlane::ready(Arc::clone(&events)))
            .unwrap();

        assert_eq!(response.status, AuthSessionStatus::SignedOut);
        assert_eq!(service.session(), response);
        assert!(service.cached_subscription().is_none());
        assert!(inspection.is_empty());
        assert_eq!(
            *lock(&events),
            vec![
                "stop-entered",
                "stop-complete",
                "delete-access",
                "delete-refresh",
                "delete-subscription",
            ]
        );
    }

    #[test]
    fn logout_stop_failure_preserves_credentials_and_cached_identity() {
        let backend = MemorySecretBackend::with_authentication();
        let inspection = backend.clone();
        let events = Arc::clone(&inspection.events);
        let (service, _) = service(
            ScriptedTransport::new([
                MockOutcome::json(200, config(false)),
                MockOutcome::json(200, account("member@example.invalid")),
                MockOutcome::json(
                    200,
                    subscription("active", Some(2_000), 0, None, "new-subscription"),
                ),
            ]),
            backend,
        );
        service.initialize().unwrap();
        service.refresh_subscription().unwrap();
        inspection.clear_events();
        let data_plane = MockLogoutDataPlane {
            events: Arc::clone(&events),
            result: Err(PlatformVpnError::CleanupFailed),
            gate: None,
        };

        assert_eq!(
            service.logout(&data_plane),
            Err(BusinessServiceError::DataPlane(
                PlatformVpnError::CleanupFailed
            ))
        );
        assert_eq!(service.session().status, AuthSessionStatus::Authenticated);
        assert!(service.cached_subscription().is_some());
        assert!(inspection.value(SecretKey::AccessToken).is_some());
        assert!(inspection.value(SecretKey::RefreshToken).is_some());
        assert!(
            inspection
                .value(SecretKey::SubscriptionCredential)
                .is_some()
        );
        assert_eq!(*lock(&events), vec!["stop-entered", "stop-complete"]);
    }

    #[test]
    fn logout_secret_failure_keeps_cache_and_retry_finishes_cleanup() {
        let backend = MemorySecretBackend::with_authentication();
        let inspection = backend.clone();
        let events = Arc::clone(&inspection.events);
        let (service, _) = service(
            ScriptedTransport::new([
                MockOutcome::json(200, config(false)),
                MockOutcome::json(200, account("member@example.invalid")),
                MockOutcome::json(
                    200,
                    subscription("active", Some(2_000), 0, None, "new-subscription"),
                ),
            ]),
            backend,
        );
        service.initialize().unwrap();
        service.refresh_subscription().unwrap();
        inspection.clear_events();
        *lock(&inspection.fail_delete_once) = Some(SecretKey::AccessToken);

        assert!(
            service
                .logout(&MockLogoutDataPlane::ready(Arc::clone(&events)))
                .is_err()
        );
        assert_eq!(service.session().status, AuthSessionStatus::Authenticated);
        assert!(service.cached_subscription().is_some());
        assert!(inspection.value(SecretKey::AccessToken).is_some());
        assert!(inspection.value(SecretKey::RefreshToken).is_none());
        assert!(
            inspection
                .value(SecretKey::SubscriptionCredential)
                .is_none()
        );

        let response = service
            .logout(&MockLogoutDataPlane::ready(Arc::clone(&events)))
            .unwrap();
        assert_eq!(response.status, AuthSessionStatus::SignedOut);
        assert!(inspection.is_empty());
        assert!(service.cached_subscription().is_none());
    }

    #[test]
    fn subscription_replacement_failure_restores_previous_native_credential() {
        let backend = MemorySecretBackend::with_authentication();
        let inspection = backend.clone();
        let (service, _) = service(
            ScriptedTransport::new([
                MockOutcome::json(200, config(false)),
                MockOutcome::json(200, account("member@example.invalid")),
                MockOutcome::json(
                    200,
                    subscription("active", Some(2_000), 0, None, "new-subscription"),
                ),
            ]),
            backend,
        );
        service.initialize().unwrap();
        *lock(&inspection.fail_store_once) = Some(SecretKey::SubscriptionCredential);

        assert!(service.refresh_subscription().is_err());
        assert_eq!(
            inspection.value(SecretKey::SubscriptionCredential).unwrap(),
            b"old-subscription"
        );
        assert!(service.cached_subscription().is_none());
    }

    #[test]
    fn expired_login_response_never_replaces_stored_authentication() {
        let backend = MemorySecretBackend::with_authentication();
        let inspection = backend.clone();
        let (service, _) = service(
            ScriptedTransport::new([
                MockOutcome::json(200, config(false)),
                MockOutcome::json(200, account("member@example.invalid")),
                MockOutcome::json(200, authentication("member@example.invalid", 1_000)),
            ]),
            backend,
        );
        service.initialize().unwrap();
        assert_eq!(
            service.login(login_request()).unwrap_err(),
            BusinessServiceError::ExpiredCredentials
        );
        assert_eq!(
            inspection.value(SecretKey::AccessToken).unwrap(),
            b"old-access"
        );
    }

    #[test]
    fn registration_enforces_server_required_invite_and_bounded_fields() {
        let (service, transport) = service(
            ScriptedTransport::new([MockOutcome::json(200, config(true))]),
            MemorySecretBackend::default(),
        );
        service.initialize().unwrap();
        let error = service
            .register(RegisterRequest {
                schema_version: 1,
                email: "member@example.invalid".to_owned(),
                password: "password-123".to_owned(),
                invite_code: None,
            })
            .unwrap_err();
        assert_eq!(error, BusinessServiceError::InviteRequired);
        assert_eq!(*lock(&transport.commands), vec![BusinessCommand::Config]);

        assert_eq!(
            service
                .login(LoginRequest {
                    schema_version: 1,
                    email: "invalid".to_owned(),
                    password: "password-123".to_owned(),
                })
                .unwrap_err(),
            BusinessServiceError::InvalidEmail
        );
    }

    #[test]
    fn concurrent_authentication_submission_is_rejected_without_second_request() {
        let gate = Arc::new(BlockingGate::default());
        let (service, transport) = service(
            ScriptedTransport::new([
                MockOutcome::json(200, config(false)),
                MockOutcome::json(200, authentication("member@example.invalid", 2_000)),
            ])
            .with_login_gate(Arc::clone(&gate)),
            MemorySecretBackend::default(),
        );
        service.initialize().unwrap();
        let running_service = Arc::clone(&service);
        let running = thread::spawn(move || running_service.login(login_request()));
        gate.wait_until_entered();
        assert_eq!(
            service.login(login_request()).unwrap_err(),
            BusinessServiceError::SubmissionInProgress
        );
        gate.release();
        assert!(running.join().unwrap().is_ok());
        assert_eq!(
            lock(&transport.commands)
                .iter()
                .filter(|command| **command == BusinessCommand::Login)
                .count(),
            1
        );
    }

    #[test]
    fn logout_holds_the_shared_operation_guard_while_data_plane_stops() {
        let gate = Arc::new(BlockingGate::default());
        let backend = MemorySecretBackend::with_authentication();
        let events = Arc::clone(&backend.events);
        let (service, transport) = service(
            ScriptedTransport::new([
                MockOutcome::json(200, config(false)),
                MockOutcome::json(200, account("member@example.invalid")),
            ]),
            backend,
        );
        service.initialize().unwrap();
        let data_plane = Arc::new(MockLogoutDataPlane {
            events,
            result: Ok(()),
            gate: Some(Arc::clone(&gate)),
        });
        let running_service = Arc::clone(&service);
        let running_data_plane = Arc::clone(&data_plane);
        let running = thread::spawn(move || running_service.logout(running_data_plane.as_ref()));
        gate.wait_until_entered();

        assert_eq!(
            service.login(login_request()).unwrap_err(),
            BusinessServiceError::SubmissionInProgress
        );
        gate.release();
        assert_eq!(
            running.join().unwrap().unwrap().status,
            AuthSessionStatus::SignedOut
        );
        assert_eq!(
            *lock(&transport.commands),
            vec![BusinessCommand::Config, BusinessCommand::Account]
        );
    }

    #[test]
    fn concurrent_subscription_refresh_is_rejected_without_second_request() {
        let gate = Arc::new(BlockingGate::default());
        let (service, transport) = service(
            ScriptedTransport::new([
                MockOutcome::json(200, config(false)),
                MockOutcome::json(200, account("member@example.invalid")),
                MockOutcome::json(
                    200,
                    subscription("active", Some(2_000), 0, None, "new-subscription"),
                ),
            ])
            .with_command_gate(BusinessCommand::Subscription, Arc::clone(&gate)),
            MemorySecretBackend::with_authentication(),
        );
        service.initialize().unwrap();
        let running_service = Arc::clone(&service);
        let running = thread::spawn(move || running_service.refresh_subscription());
        gate.wait_until_entered();
        assert_eq!(
            service.refresh_subscription().unwrap_err(),
            BusinessServiceError::SubmissionInProgress
        );
        gate.release();
        assert!(running.join().unwrap().is_ok());
        assert_eq!(
            lock(&transport.commands)
                .iter()
                .filter(|command| **command == BusinessCommand::Subscription)
                .count(),
            1
        );
    }

    #[test]
    fn authentication_replacement_rolls_back_every_user_secret_on_partial_failure() {
        let backend = MemorySecretBackend::with_authentication();
        *lock(&backend.fail_store_once) = Some(SecretKey::RefreshToken);
        let client =
            BusinessCommandClient::new(Arc::new(ScriptedTransport::new([])), backend.clone());
        let mut access = SecretValue::new(b"new-access".to_vec()).unwrap();
        let mut refresh = SecretValue::new(b"new-refresh".to_vec()).unwrap();
        assert!(
            client
                .replace_authentication(&mut access, &mut refresh)
                .is_err()
        );
        assert!(access.is_cleared());
        assert!(refresh.is_cleared());
        assert_eq!(
            backend.value(SecretKey::AccessToken).unwrap(),
            b"old-access"
        );
        assert_eq!(
            backend.value(SecretKey::RefreshToken).unwrap(),
            b"old-refresh"
        );
        assert_eq!(
            backend.value(SecretKey::SubscriptionCredential).unwrap(),
            b"old-subscription"
        );
    }

    #[test]
    fn config_urls_reject_scheme_host_credentials_port_query_fragment_and_origin_paths() {
        let invalid_urls = [
            ("apiBaseUrl", "http://api.orange.invalid/"),
            ("apiBaseUrl", "https://api.orange.invalid/v1"),
            ("paymentBaseUrl", "https://evil.invalid/"),
            ("paymentBaseUrl", "https://pay.orange.invalid:444/"),
            ("supportUrl", "https://user@support.orange.invalid/help"),
            ("bannerUrl", "https://assets.orange.invalid/banner.png?v=1"),
            ("bannerUrl", "https://assets.orange.invalid/banner.png#top"),
        ];
        for (field, value) in invalid_urls {
            let mut body = config(false);
            body[field] = json!(value);
            let (service, _) = service(
                ScriptedTransport::new([MockOutcome::json(200, body)]),
                MemorySecretBackend::default(),
            );
            assert_eq!(
                service.initialize().unwrap_err(),
                BusinessServiceError::RejectedConfigUrl,
                "accepted unsafe {field}"
            );
        }
    }

    #[test]
    fn response_requires_json_content_type_and_strict_schema() {
        for outcome in [
            MockOutcome::Response {
                status: 200,
                content_type: "text/plain",
                body: serde_json::to_vec(&config(false)).unwrap(),
            },
            MockOutcome::json(200, json!({ "schemaVersion": 1 })),
        ] {
            let (service, _) = service(
                ScriptedTransport::new([outcome]),
                MemorySecretBackend::default(),
            );
            assert!(matches!(
                service.initialize(),
                Err(BusinessServiceError::InvalidContentType
                    | BusinessServiceError::InvalidResponse)
            ));
        }
    }

    #[test]
    fn hardcoded_url_rules_match_the_machine_readable_security_policy() {
        let policy: Value =
            serde_json::from_str(include_str!("../../../security/control-endpoints.yml")).unwrap();
        let rules = &policy["dynamic_config_url_policy"];
        assert_eq!(rules["scheme"], "https");
        assert_eq!(rules["port"], 443);
        assert_eq!(rules["allow_credentials"], false);
        assert_eq!(rules["allow_query"], false);
        assert_eq!(rules["allow_fragment"], false);
        assert_eq!(rules["api_hosts"], json!(["api.orange.invalid"]));
        assert_eq!(rules["payment_hosts"], json!(DEVELOPMENT_PAYMENT_URL_HOSTS));
        assert_eq!(rules["support_hosts"], json!(DEVELOPMENT_SUPPORT_URL_HOSTS));
        assert_eq!(rules["banner_hosts"], json!(DEVELOPMENT_BANNER_URL_HOSTS));
    }
}

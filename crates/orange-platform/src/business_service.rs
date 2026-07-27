use std::{
    fmt,
    sync::{
        Arc, Mutex, MutexGuard,
        atomic::{AtomicBool, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};

use orange_domain::{
    AccountResponse, AuthPublicResponse, AuthSessionResponse, AuthWireResponse,
    BUSINESS_API_SCHEMA_VERSION, BusinessInitializationResponse, ConfigResponse,
    ConfigWireResponse, ErrorCode, LoginRequest, RegisterRequest,
};
use serde::de::DeserializeOwned;
use url::Url;
use zeroize::Zeroizing;

use crate::{
    AuthenticationSecretState, BootstrapTransport, BootstrapTransportError, BusinessClientError,
    BusinessCommand, BusinessCommandClient, BusinessCommandRequest, BusinessCommandResponse,
    SecretStoreBackend, SecretStoreError, SecretValue,
};

pub const MAX_AUTH_EMAIL_BYTES: usize = 254;
pub const MIN_AUTH_PASSWORD_BYTES: usize = 8;
pub const MAX_AUTH_PASSWORD_BYTES: usize = 128;
pub const MAX_INVITE_CODE_BYTES: usize = 64;

const DEVELOPMENT_PAYMENT_URL_HOSTS: &[&str] = &["pay.orange.invalid"];
const DEVELOPMENT_SUPPORT_URL_HOSTS: &[&str] = &["support.orange.invalid"];
const DEVELOPMENT_BANNER_URL_HOSTS: &[&str] = &["assets.orange.invalid"];

pub trait BusinessClock: Send + Sync {
    fn now_unix_ms(&self) -> u64;
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
    NotInitialized,
    SubmissionInProgress,
    InvalidContentType,
    InvalidResponse,
    RejectedConfigUrl,
    ExpiredCredentials,
    Client(BusinessClientError),
}

impl BusinessServiceError {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidEmail => "business-invalid-email",
            Self::InvalidPassword => "business-invalid-password",
            Self::InviteRequired => "business-invite-required",
            Self::InvalidInviteCode => "business-invalid-invite-code",
            Self::NotInitialized => "business-not-initialized",
            Self::SubmissionInProgress => "business-submission-in-progress",
            Self::InvalidContentType => "business-invalid-content-type",
            Self::InvalidResponse => "business-invalid-response",
            Self::RejectedConfigUrl => "business-config-url-rejected",
            Self::ExpiredCredentials => "business-expired-credentials",
            Self::Client(error) => error.as_str(),
        }
    }

    pub const fn public_error_code(self) -> ErrorCode {
        match self {
            Self::InvalidEmail
            | Self::InvalidPassword
            | Self::InviteRequired
            | Self::InvalidInviteCode => ErrorCode::Validation,
            Self::NotInitialized | Self::RejectedConfigUrl => ErrorCode::Bootstrap,
            Self::SubmissionInProgress => ErrorCode::Cancelled,
            Self::InvalidContentType | Self::InvalidResponse | Self::ExpiredCredentials => {
                ErrorCode::Service
            }
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

#[derive(Clone)]
struct BusinessState {
    config: Option<ConfigResponse>,
    session: AuthSessionResponse,
}

impl Default for BusinessState {
    fn default() -> Self {
        Self {
            config: None,
            session: AuthSessionResponse::signed_out(),
        }
    }
}

pub struct BusinessApiService<T, B, C = SystemClock> {
    client: Arc<BusinessCommandClient<T, B>>,
    clock: C,
    state: Mutex<BusinessState>,
    auth_submission_in_flight: AtomicBool,
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
            auth_submission_in_flight: AtomicBool::new(false),
        }
    }

    pub fn initialize(&self) -> Result<BusinessInitializationResponse, BusinessServiceError> {
        if let Err(error) = self.client.wait_until_ready() {
            self.reconcile_unverified_session()?;
            return Err(error.into());
        }

        let config = match self.fetch_config() {
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
        state.session = session;
        Ok(response)
    }

    pub fn login(&self, request: LoginRequest) -> Result<AuthPublicResponse, BusinessServiceError> {
        validate_email(&request.email)?;
        validate_password(&request.password)?;
        self.require_config()?;
        let _submission = self.acquire_auth_submission()?;
        self.authenticate(BusinessCommand::Login, &request)
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
        let registration_requires_invite = self.require_config()?.registration_requires_invite;
        if registration_requires_invite && request.invite_code.is_none() {
            return Err(BusinessServiceError::InviteRequired);
        }
        let _submission = self.acquire_auth_submission()?;
        self.authenticate(BusinessCommand::Register, &request)
    }

    pub fn session(&self) -> AuthSessionResponse {
        lock(&self.state).session.clone()
    }

    fn fetch_config(&self) -> Result<ConfigResponse, BusinessServiceError> {
        let request = BusinessCommandRequest::without_body(BusinessCommand::Config)?;
        let response = self.client.execute(request)?;
        let wire: ConfigWireResponse = decode_json_response(response)?;
        self.validate_config_urls(&wire)?;
        Ok(ConfigResponse {
            schema_version: wire.schema_version,
            minimum_supported_version: wire.minimum_supported_version.clone(),
            maintenance: wire.maintenance,
            notice: wire.notice.clone(),
            registration_requires_invite: wire.registration_requires_invite,
        })
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
                        let account: AccountResponse = decode_json_response(response)?;
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
        lock(&self.state).session = session;
        Ok(())
    }

    fn require_config(&self) -> Result<ConfigResponse, BusinessServiceError> {
        lock(&self.state)
            .config
            .clone()
            .ok_or(BusinessServiceError::NotInitialized)
    }

    fn acquire_auth_submission(&self) -> Result<AuthSubmission<'_>, BusinessServiceError> {
        self.auth_submission_in_flight
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| BusinessServiceError::SubmissionInProgress)?;
        Ok(AuthSubmission {
            in_flight: &self.auth_submission_in_flight,
        })
    }

    fn authenticate(
        &self,
        command: BusinessCommand,
        request: &impl serde::Serialize,
    ) -> Result<AuthPublicResponse, BusinessServiceError> {
        let request = BusinessCommandRequest::json(command, request)?;
        let response = self.client.execute(request)?;
        let mut wire: AuthWireResponse = decode_json_response(response)?;
        if wire.credentials.expires_at_unix_ms.get() <= self.clock.now_unix_ms() {
            return Err(BusinessServiceError::ExpiredCredentials);
        }

        let mut access =
            SecretValue::new(std::mem::take(&mut wire.credentials.access_token).into_bytes())
                .map_err(BusinessClientError::from)?;
        let mut refresh =
            SecretValue::new(std::mem::take(&mut wire.credentials.refresh_token).into_bytes())
                .map_err(BusinessClientError::from)?;
        self.client
            .replace_authentication(&mut access, &mut refresh)?;

        let response = AuthPublicResponse {
            schema_version: wire.schema_version,
            authenticated: true,
            user: wire.user,
        };
        lock(&self.state).session = AuthSessionResponse::authenticated(response.user.clone());
        Ok(response)
    }
}

struct AuthSubmission<'a> {
    in_flight: &'a AtomicBool,
}

impl Drop for AuthSubmission<'_> {
    fn drop(&mut self) {
        self.in_flight.store(false, Ordering::Release);
    }
}

fn decode_json_response<T: DeserializeOwned>(
    mut response: BusinessCommandResponse,
) -> Result<T, BusinessServiceError> {
    if !is_json_content_type(response.content_type()) {
        return Err(BusinessServiceError::InvalidContentType);
    }
    let body = Zeroizing::new(response.take_body());
    if body.is_empty() {
        return Err(BusinessServiceError::InvalidResponse);
    }
    serde_json::from_slice(&body).map_err(|_| BusinessServiceError::InvalidResponse)
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
        block_login: Option<Arc<BlockingGate>>,
    }

    impl ScriptedTransport {
        fn new(outcomes: impl IntoIterator<Item = MockOutcome>) -> Self {
            Self {
                ready: Ok(()),
                wait_calls: AtomicUsize::new(0),
                outcomes: Mutex::new(outcomes.into_iter().collect()),
                commands: Mutex::new(Vec::new()),
                block_login: None,
            }
        }

        fn unavailable() -> Self {
            Self {
                ready: Err(BootstrapTransportError::Unavailable),
                ..Self::new([])
            }
        }

        fn with_login_gate(mut self, gate: Arc<BlockingGate>) -> Self {
            self.block_login = Some(gate);
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
            if command == BusinessCommand::Login
                && let Some(gate) = &self.block_login
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

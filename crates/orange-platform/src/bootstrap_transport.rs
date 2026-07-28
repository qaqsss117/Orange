use std::{fmt, sync::Arc};

use serde::Serialize;
use url::Url;
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

use crate::{
    AuthenticationSecretState, SecretKey, SecretStorage, SecretStoreBackend, SecretStoreError,
    SecretValue,
};

pub const BOOTSTRAP_TRANSPORT_SCHEMA_VERSION: u16 = 1;
pub const MAX_BUSINESS_REQUEST_BYTES: usize = 1 << 20;
pub const MAX_BUSINESS_RESPONSE_BYTES: usize = 1 << 20;
const MAX_CONTENT_TYPE_BYTES: usize = 256;
const MAX_SUBSCRIPTION_TARGET_BYTES: usize = 8192;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BusinessCommand {
    Login,
    Register,
    Config,
    Subscription,
    Account,
    Plans,
    Orders,
    Invite,
    Tickets,
    Update,
}

impl BusinessCommand {
    pub const ALL: [Self; 10] = [
        Self::Login,
        Self::Register,
        Self::Config,
        Self::Subscription,
        Self::Account,
        Self::Plans,
        Self::Orders,
        Self::Invite,
        Self::Tickets,
        Self::Update,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Login => "login",
            Self::Register => "register",
            Self::Config => "config",
            Self::Subscription => "subscription",
            Self::Account => "account",
            Self::Plans => "plans",
            Self::Orders => "orders",
            Self::Invite => "invite",
            Self::Tickets => "tickets",
            Self::Update => "update",
        }
    }

    pub const fn route(self) -> BusinessRoute {
        match self {
            Self::Login => BusinessRoute::post(
                self,
                "/api/v1/passport/auth/login",
                BusinessAuthentication::None,
            ),
            Self::Register => BusinessRoute::post(
                self,
                "/v1/development/auth/register",
                BusinessAuthentication::None,
            ),
            Self::Config => BusinessRoute::get(
                self,
                "/api/v1/guest/comm/config",
                BusinessAuthentication::None,
            ),
            Self::Subscription => BusinessRoute::get(
                self,
                "/api/v1/user/getSubscribe",
                BusinessAuthentication::RustToken,
            ),
            Self::Account => {
                BusinessRoute::get(self, "/api/v1/user/info", BusinessAuthentication::RustToken)
            }
            Self::Plans => {
                BusinessRoute::get(self, "/v1/development/plans", BusinessAuthentication::None)
            }
            Self::Orders => BusinessRoute::post(
                self,
                "/v1/development/orders",
                BusinessAuthentication::RustToken,
            ),
            Self::Invite => BusinessRoute::get(
                self,
                "/v1/development/invite",
                BusinessAuthentication::RustToken,
            ),
            Self::Tickets => BusinessRoute::post(
                self,
                "/v1/development/tickets",
                BusinessAuthentication::RustToken,
            ),
            Self::Update => {
                BusinessRoute::get(self, "/v1/development/update", BusinessAuthentication::None)
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum BusinessMethod {
    Get,
    Post,
}

impl BusinessMethod {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Post => "POST",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BusinessAuthentication {
    None,
    RustToken,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BusinessTarget {
    BootstrapPrimaryApi,
}

impl BusinessAuthentication {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::RustToken => "rust_token",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BusinessRoute {
    command: BusinessCommand,
    method: BusinessMethod,
    target: BusinessTarget,
    path: &'static str,
    authentication: BusinessAuthentication,
    content_type: Option<&'static str>,
}

impl BusinessRoute {
    const fn get(
        command: BusinessCommand,
        path: &'static str,
        authentication: BusinessAuthentication,
    ) -> Self {
        Self {
            command,
            method: BusinessMethod::Get,
            target: BusinessTarget::BootstrapPrimaryApi,
            path,
            authentication,
            content_type: None,
        }
    }

    const fn post(
        command: BusinessCommand,
        path: &'static str,
        authentication: BusinessAuthentication,
    ) -> Self {
        Self {
            command,
            method: BusinessMethod::Post,
            target: BusinessTarget::BootstrapPrimaryApi,
            path,
            authentication,
            content_type: Some("application/json"),
        }
    }

    pub const fn command(self) -> BusinessCommand {
        self.command
    }

    pub const fn method(self) -> BusinessMethod {
        self.method
    }

    pub const fn target(self) -> BusinessTarget {
        self.target
    }

    pub const fn path(self) -> &'static str {
        self.path
    }

    pub const fn authentication(self) -> BusinessAuthentication {
        self.authentication
    }

    pub const fn content_type(self) -> Option<&'static str> {
        self.content_type
    }
}

#[derive(Zeroize, ZeroizeOnDrop)]
pub struct BusinessCommandRequest {
    #[zeroize(skip)]
    command: BusinessCommand,
    body: Vec<u8>,
}

impl BusinessCommandRequest {
    pub fn without_body(command: BusinessCommand) -> Result<Self, BusinessClientError> {
        if command.route().method() != BusinessMethod::Get {
            return Err(BusinessClientError::InvalidRequest);
        }
        Ok(Self {
            command,
            body: Vec::new(),
        })
    }

    pub fn json(
        command: BusinessCommand,
        value: &impl Serialize,
    ) -> Result<Self, BusinessClientError> {
        if command.route().method() != BusinessMethod::Post {
            return Err(BusinessClientError::InvalidRequest);
        }
        let mut body =
            serde_json::to_vec(value).map_err(|_| BusinessClientError::InvalidRequest)?;
        if body.is_empty() || body.len() > MAX_BUSINESS_REQUEST_BYTES {
            body.zeroize();
            return Err(BusinessClientError::InvalidRequest);
        }
        Ok(Self { command, body })
    }

    pub const fn command(&self) -> BusinessCommand {
        self.command
    }
}

impl fmt::Debug for BusinessCommandRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BusinessCommandRequest")
            .field("command", &self.command)
            .field("body_bytes", &self.body.len())
            .finish()
    }
}

pub struct BootstrapTransportRequest<'a> {
    route: BusinessRoute,
    body: &'a [u8],
    access_token: Option<&'a [u8]>,
}

impl BootstrapTransportRequest<'_> {
    pub const fn route(&self) -> BusinessRoute {
        self.route
    }

    pub const fn body(&self) -> &[u8] {
        self.body
    }

    pub const fn access_token(&self) -> Option<&[u8]> {
        self.access_token
    }
}

impl fmt::Debug for BootstrapTransportRequest<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BootstrapTransportRequest")
            .field("command", &self.route.command)
            .field("method", &self.route.method)
            .field("target", &self.route.target)
            .field("path", &self.route.path)
            .field("body_bytes", &self.body.len())
            .field("authenticated", &self.access_token.is_some())
            .finish()
    }
}

pub struct BootstrapSubscriptionRequest<'a> {
    host: &'a str,
    path_and_query: &'a str,
}

impl BootstrapSubscriptionRequest<'_> {
    pub const fn host(&self) -> &str {
        self.host
    }

    pub const fn path_and_query(&self) -> &str {
        self.path_and_query
    }
}

impl fmt::Debug for BootstrapSubscriptionRequest<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BootstrapSubscriptionRequest")
            .field("host", &"<allowlisted>")
            .field("path_bytes", &self.path_and_query.len())
            .finish()
    }
}

#[derive(Zeroize, ZeroizeOnDrop)]
pub struct BootstrapTransportResponse {
    #[zeroize(skip)]
    status_code: u16,
    content_type: String,
    body: Vec<u8>,
}

impl BootstrapTransportResponse {
    pub fn new(
        status_code: u16,
        content_type: impl Into<String>,
        body: impl Into<Vec<u8>>,
    ) -> Result<Self, BootstrapTransportError> {
        let mut content_type = content_type.into();
        let mut body = body.into();
        if !(100..=599).contains(&status_code)
            || content_type.len() > MAX_CONTENT_TYPE_BYTES
            || content_type.chars().any(char::is_control)
            || body.len() > MAX_BUSINESS_RESPONSE_BYTES
        {
            content_type.zeroize();
            body.zeroize();
            return Err(BootstrapTransportError::InvalidResponse);
        }
        Ok(Self {
            status_code,
            content_type,
            body,
        })
    }
}

impl fmt::Debug for BootstrapTransportResponse {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BootstrapTransportResponse")
            .field("status_code", &self.status_code)
            .field("content_type", &self.content_type)
            .field("body_bytes", &self.body.len())
            .finish()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootstrapTransportError {
    InvalidRequest,
    InvalidResponse,
    Unavailable,
    Timeout,
    Cancelled,
    DnsFailure,
    TlsFailure,
    ResponseTooLarge,
}

impl BootstrapTransportError {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidRequest => "transport-invalid-request",
            Self::InvalidResponse => "transport-invalid-response",
            Self::Unavailable => "transport-unavailable",
            Self::Timeout => "transport-timeout",
            Self::Cancelled => "transport-cancelled",
            Self::DnsFailure => "transport-dns-failure",
            Self::TlsFailure => "transport-tls-failure",
            Self::ResponseTooLarge => "transport-response-too-large",
        }
    }
}

impl fmt::Display for BootstrapTransportError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl std::error::Error for BootstrapTransportError {}

pub trait BootstrapTransport: Send + Sync {
    fn wait_until_ready(&self) -> Result<(), BootstrapTransportError> {
        Ok(())
    }

    fn is_control_api_host_allowed(&self, _host: &str) -> Result<bool, BootstrapTransportError> {
        Ok(false)
    }

    fn execute(
        &self,
        request: BootstrapTransportRequest<'_>,
    ) -> Result<BootstrapTransportResponse, BootstrapTransportError>;

    fn download_subscription(
        &self,
        _request: BootstrapSubscriptionRequest<'_>,
    ) -> Result<BootstrapTransportResponse, BootstrapTransportError> {
        Err(BootstrapTransportError::InvalidRequest)
    }
}

impl<T: BootstrapTransport + ?Sized> BootstrapTransport for Arc<T> {
    fn wait_until_ready(&self) -> Result<(), BootstrapTransportError> {
        (**self).wait_until_ready()
    }

    fn is_control_api_host_allowed(&self, host: &str) -> Result<bool, BootstrapTransportError> {
        (**self).is_control_api_host_allowed(host)
    }

    fn execute(
        &self,
        request: BootstrapTransportRequest<'_>,
    ) -> Result<BootstrapTransportResponse, BootstrapTransportError> {
        (**self).execute(request)
    }

    fn download_subscription(
        &self,
        request: BootstrapSubscriptionRequest<'_>,
    ) -> Result<BootstrapTransportResponse, BootstrapTransportError> {
        (**self).download_subscription(request)
    }
}

#[derive(Zeroize, ZeroizeOnDrop)]
pub struct BusinessCommandResponse {
    #[zeroize(skip)]
    status_code: u16,
    content_type: String,
    body: Vec<u8>,
}

impl BusinessCommandResponse {
    pub const fn status_code(&self) -> u16 {
        self.status_code
    }

    pub fn content_type(&self) -> &str {
        &self.content_type
    }

    pub fn body(&self) -> &[u8] {
        &self.body
    }

    pub fn take_body(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.body)
    }
}

impl fmt::Debug for BusinessCommandResponse {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BusinessCommandResponse")
            .field("status_code", &self.status_code)
            .field("content_type", &self.content_type)
            .field("body_bytes", &self.body.len())
            .finish()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BusinessClientError {
    InvalidRequest,
    AuthenticationRequired,
    SecretStore(SecretStoreError),
    Transport(BootstrapTransportError),
    RedirectDenied,
    Unauthorized,
    RequestRejected,
    RateLimited,
    ServiceUnavailable,
    InvalidResponse,
}

impl BusinessClientError {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidRequest => "business-invalid-request",
            Self::AuthenticationRequired => "business-authentication-required",
            Self::SecretStore(_) => "business-secret-store-failure",
            Self::Transport(error) => error.as_str(),
            Self::RedirectDenied => "business-redirect-denied",
            Self::Unauthorized => "business-unauthorized",
            Self::RequestRejected => "business-request-rejected",
            Self::RateLimited => "business-rate-limited",
            Self::ServiceUnavailable => "business-service-unavailable",
            Self::InvalidResponse => "business-invalid-response",
        }
    }
}

impl fmt::Display for BusinessClientError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl std::error::Error for BusinessClientError {}

impl From<SecretStoreError> for BusinessClientError {
    fn from(error: SecretStoreError) -> Self {
        Self::SecretStore(error)
    }
}

impl From<BootstrapTransportError> for BusinessClientError {
    fn from(error: BootstrapTransportError) -> Self {
        Self::Transport(error)
    }
}

pub struct BusinessCommandClient<T, B> {
    transport: T,
    secrets: SecretStorage<B>,
}

impl<T, B> BusinessCommandClient<T, B>
where
    T: BootstrapTransport,
    B: SecretStoreBackend,
{
    pub const fn new(transport: T, secret_backend: B) -> Self {
        Self {
            transport,
            secrets: SecretStorage::new(secret_backend),
        }
    }

    pub fn execute(
        &self,
        request: BusinessCommandRequest,
    ) -> Result<BusinessCommandResponse, BusinessClientError> {
        let route = request.command.route();
        let access_token = match route.authentication {
            BusinessAuthentication::None => None,
            BusinessAuthentication::RustToken => self
                .secrets
                .load(SecretKey::AccessToken)?
                .ok_or(BusinessClientError::AuthenticationRequired)?
                .into(),
        };

        let response = match access_token.as_ref() {
            Some(access_token) => access_token.with_bytes(|token| {
                self.transport.execute(BootstrapTransportRequest {
                    route,
                    body: &request.body,
                    access_token: Some(token),
                })
            }),
            None => self.transport.execute(BootstrapTransportRequest {
                route,
                body: &request.body,
                access_token: None,
            }),
        }?;
        let result = map_response(response);
        if route.authentication == BusinessAuthentication::RustToken
            && matches!(&result, Err(BusinessClientError::Unauthorized))
        {
            self.clear_authentication()?;
        }
        result
    }

    pub fn wait_until_ready(&self) -> Result<(), BusinessClientError> {
        self.transport.wait_until_ready().map_err(Into::into)
    }

    pub fn authentication_state(&self) -> Result<AuthenticationSecretState, BusinessClientError> {
        self.secrets.authentication_state().map_err(Into::into)
    }

    pub fn is_control_api_host_allowed(&self, host: &str) -> Result<bool, BusinessClientError> {
        self.transport
            .is_control_api_host_allowed(host)
            .map_err(Into::into)
    }

    pub fn download_subscription(&self) -> Result<Zeroizing<Vec<u8>>, BusinessClientError> {
        let credential = self
            .secrets
            .load(SecretKey::SubscriptionCredential)?
            .ok_or(BusinessClientError::AuthenticationRequired)?;
        let target = credential.with_bytes(parse_subscription_target)?;
        if !self
            .transport
            .is_control_api_host_allowed(target.host.as_str())?
        {
            return Err(BusinessClientError::InvalidRequest);
        }

        let response = self
            .transport
            .download_subscription(BootstrapSubscriptionRequest {
                host: target.host.as_str(),
                path_and_query: target.path_and_query.as_str(),
            })?;
        let mut response = map_response(response)?;
        Ok(Zeroizing::new(response.take_body()))
    }

    pub fn replace_authentication(
        &self,
        access: &mut SecretValue,
        refresh: &mut SecretValue,
    ) -> Result<(), BusinessClientError> {
        self.secrets
            .replace_authentication(access, refresh)
            .map_err(Into::into)
    }

    pub fn clear_authentication(&self) -> Result<(), BusinessClientError> {
        self.secrets.logout().map_err(Into::into)
    }

    pub fn replace_subscription_credential(
        &self,
        credential: &mut SecretValue,
    ) -> Result<(), BusinessClientError> {
        self.secrets
            .replace_subscription_credential(credential)
            .map_err(Into::into)
    }

    pub fn clear_subscription_credential(&self) -> Result<(), BusinessClientError> {
        self.secrets
            .clear_subscription_credential()
            .map_err(Into::into)
    }
}

struct SubscriptionTarget {
    host: Zeroizing<String>,
    path_and_query: Zeroizing<String>,
}

fn parse_subscription_target(bytes: &[u8]) -> Result<SubscriptionTarget, BusinessClientError> {
    let text = std::str::from_utf8(bytes).map_err(|_| BusinessClientError::InvalidRequest)?;
    let url = Url::parse(text).map_err(|_| BusinessClientError::InvalidRequest)?;
    let valid = url.scheme() == "https"
        && !url.cannot_be_a_base()
        && url.username().is_empty()
        && url.password().is_none()
        && url.port_or_known_default() == Some(443)
        && url.fragment().is_none();
    if !valid {
        let mut serialized = String::from(url);
        serialized.zeroize();
        return Err(BusinessClientError::InvalidRequest);
    }

    let host = url.host_str().map(str::to_ascii_lowercase);
    let mut path_and_query = url.path().to_owned();
    if let Some(query) = url.query() {
        path_and_query.push('?');
        path_and_query.push_str(query);
    }
    let mut serialized = String::from(url);
    serialized.zeroize();
    let host = host.ok_or(BusinessClientError::InvalidRequest)?;

    if path_and_query.is_empty()
        || path_and_query.len() > MAX_SUBSCRIPTION_TARGET_BYTES
        || path_and_query.starts_with("//")
        || path_and_query.contains('#')
        || path_and_query.contains("://")
        || path_and_query.chars().any(char::is_control)
    {
        path_and_query.zeroize();
        return Err(BusinessClientError::InvalidRequest);
    }

    Ok(SubscriptionTarget {
        host: Zeroizing::new(host),
        path_and_query: Zeroizing::new(path_and_query),
    })
}

fn map_response(
    mut response: BootstrapTransportResponse,
) -> Result<BusinessCommandResponse, BusinessClientError> {
    match response.status_code {
        200..=299 => Ok(BusinessCommandResponse {
            status_code: response.status_code,
            content_type: std::mem::take(&mut response.content_type),
            body: std::mem::take(&mut response.body),
        }),
        300..=399 => Err(BusinessClientError::RedirectDenied),
        401 | 403 => Err(BusinessClientError::Unauthorized),
        429 => Err(BusinessClientError::RateLimited),
        400..=499 => Err(BusinessClientError::RequestRejected),
        500..=599 => Err(BusinessClientError::ServiceUnavailable),
        _ => Err(BusinessClientError::InvalidResponse),
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::HashMap,
        sync::{
            Arc, Mutex, MutexGuard,
            atomic::{AtomicUsize, Ordering},
        },
    };

    use serde::Deserialize;
    use serde_json::{Value, json};
    use zeroize::Zeroizing;

    use super::*;

    const ROUTE_FIXTURE: &str =
        include_str!("../../../contracts/control-plane/fixtures/business-command-routes.v1.json");
    const ERROR_FIXTURE: &str = include_str!(
        "../../../contracts/control-plane/fixtures/bootstrap-transport-errors.v1.json"
    );
    const ENDPOINT_POLICY: &str = include_str!("../../../security/control-endpoints.yml");

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct RouteDocument {
        schema_version: u16,
        routes: Vec<RouteFixture>,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct RouteFixture {
        command: String,
        method: String,
        host: String,
        path: String,
        authentication: String,
        content_type: Option<String>,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct ErrorDocument {
        schema_version: u16,
        cases: Vec<ErrorFixture>,
    }

    #[derive(Deserialize)]
    #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
    enum ErrorFixture {
        Http {
            #[serde(rename = "statusCode")]
            status_code: u16,
            expected: String,
        },
        Transport {
            source: String,
            expected: String,
        },
    }

    #[derive(Default)]
    struct MemorySecretBackend {
        values: Mutex<HashMap<SecretKey, Zeroizing<Vec<u8>>>>,
        access_loads: Arc<AtomicUsize>,
    }

    impl MemorySecretBackend {
        fn with_access_token(value: &[u8]) -> Self {
            Self {
                values: Mutex::new(HashMap::from([(
                    SecretKey::AccessToken,
                    Zeroizing::new(value.to_vec()),
                )])),
                access_loads: Arc::new(AtomicUsize::new(0)),
            }
        }

        fn with_subscription(value: &[u8]) -> Self {
            Self {
                values: Mutex::new(HashMap::from([(
                    SecretKey::SubscriptionCredential,
                    Zeroizing::new(value.to_vec()),
                )])),
                access_loads: Arc::new(AtomicUsize::new(0)),
            }
        }
    }

    impl SecretStoreBackend for MemorySecretBackend {
        fn store(&self, key: SecretKey, value: &[u8]) -> Result<(), SecretStoreError> {
            lock(&self.values).insert(key, Zeroizing::new(value.to_vec()));
            Ok(())
        }

        fn load(&self, key: SecretKey) -> Result<Option<crate::SecretValue>, SecretStoreError> {
            if key == SecretKey::AccessToken {
                self.access_loads.fetch_add(1, Ordering::Relaxed);
            }
            lock(&self.values)
                .get(&key)
                .map(|value| crate::SecretValue::new(value.to_vec()))
                .transpose()
        }

        fn delete(&self, key: SecretKey) -> Result<(), SecretStoreError> {
            lock(&self.values).remove(&key);
            Ok(())
        }
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct ObservedRequest {
        command: BusinessCommand,
        method: BusinessMethod,
        target: BusinessTarget,
        path: String,
        authenticated: bool,
        body_bytes: usize,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct ObservedSubscriptionRequest {
        host: String,
        path_and_query: String,
    }

    #[derive(Debug, Clone)]
    enum MockOutcome {
        Response(u16),
        Error(BootstrapTransportError),
    }

    struct MockTransport {
        requests: Mutex<Vec<ObservedRequest>>,
        subscription_requests: Mutex<Vec<ObservedSubscriptionRequest>>,
        outcome: Mutex<MockOutcome>,
    }

    impl Default for MockTransport {
        fn default() -> Self {
            Self {
                requests: Mutex::new(Vec::new()),
                subscription_requests: Mutex::new(Vec::new()),
                outcome: Mutex::new(MockOutcome::Response(204)),
            }
        }
    }

    impl BootstrapTransport for MockTransport {
        fn is_control_api_host_allowed(&self, host: &str) -> Result<bool, BootstrapTransportError> {
            Ok(host == "subscriptions.example")
        }

        fn execute(
            &self,
            request: BootstrapTransportRequest<'_>,
        ) -> Result<BootstrapTransportResponse, BootstrapTransportError> {
            lock(&self.requests).push(ObservedRequest {
                command: request.route.command,
                method: request.route.method,
                target: request.route.target,
                path: request.route.path.to_owned(),
                authenticated: request.access_token.is_some(),
                body_bytes: request.body.len(),
            });
            match lock(&self.outcome).clone() {
                MockOutcome::Response(status_code) => {
                    BootstrapTransportResponse::new(status_code, "application/json", b"{}".to_vec())
                }
                MockOutcome::Error(error) => Err(error),
            }
        }

        fn download_subscription(
            &self,
            request: BootstrapSubscriptionRequest<'_>,
        ) -> Result<BootstrapTransportResponse, BootstrapTransportError> {
            lock(&self.subscription_requests).push(ObservedSubscriptionRequest {
                host: request.host.to_owned(),
                path_and_query: request.path_and_query.to_owned(),
            });
            match lock(&self.outcome).clone() {
                MockOutcome::Response(status_code) => BootstrapTransportResponse::new(
                    status_code,
                    "text/plain",
                    b"subscription.fixture".to_vec(),
                ),
                MockOutcome::Error(error) => Err(error),
            }
        }
    }

    #[test]
    fn all_routes_match_the_security_policy_and_contract_fixture() {
        let document: RouteDocument = serde_json::from_str(ROUTE_FIXTURE).unwrap();
        assert_eq!(document.schema_version, BOOTSTRAP_TRANSPORT_SCHEMA_VERSION);
        assert_eq!(document.routes.len(), BusinessCommand::ALL.len());

        let policy: Value = serde_json::from_str(ENDPOINT_POLICY).unwrap();
        assert_eq!(policy["schema_version"], BOOTSTRAP_TRANSPORT_SCHEMA_VERSION);
        assert_eq!(policy["transport"]["scheme"], "https");
        assert_eq!(policy["transport"]["port"], 443);
        assert_eq!(policy["transport"]["redirect_policy"], "deny");
        assert_eq!(policy["transport"]["max_request_attempts"], 1);
        assert_eq!(
            policy["transport"]["max_request_bytes"],
            MAX_BUSINESS_REQUEST_BYTES
        );
        assert_eq!(
            policy["transport"]["max_response_bytes"],
            MAX_BUSINESS_RESPONSE_BYTES
        );

        for (command, fixture) in BusinessCommand::ALL.into_iter().zip(document.routes) {
            let route = command.route();
            assert_eq!(fixture.command, command.as_str());
            assert_eq!(fixture.method, route.method.as_str());
            assert_eq!(route.target, BusinessTarget::BootstrapPrimaryApi);
            assert_eq!(fixture.path, route.path);
            assert_eq!(fixture.authentication, route.authentication.as_str());
            assert_eq!(fixture.content_type.as_deref(), route.content_type);

            let policy_command = policy["commands"]
                .as_array()
                .unwrap()
                .iter()
                .find(|entry| entry["name"] == command.as_str())
                .unwrap();
            assert_eq!(policy_command["method"], route.method.as_str());
            assert_eq!(policy_command["path"], route.path);
            assert_eq!(
                policy_command["authentication"],
                route.authentication.as_str()
            );
            assert_eq!(policy_command["content_type"].as_str(), route.content_type);
            assert!(
                policy["hosts"]
                    .as_array()
                    .unwrap()
                    .contains(&json!(fixture.host))
            );
        }
    }

    #[test]
    fn every_business_command_uses_one_transport_and_rust_injects_tokens() {
        let transport = Arc::new(MockTransport::default());
        let backend = MemorySecretBackend::with_access_token(b"access-token.fixture");
        let access_loads = Arc::clone(&backend.access_loads);
        let client = BusinessCommandClient::new(Arc::clone(&transport), backend);

        for command in BusinessCommand::ALL {
            let request = match command.route().method {
                BusinessMethod::Get => BusinessCommandRequest::without_body(command).unwrap(),
                BusinessMethod::Post => {
                    BusinessCommandRequest::json(command, &json!({ "fixture": true })).unwrap()
                }
            };
            assert_eq!(client.execute(request).unwrap().status_code(), 204);
        }

        let requests = lock(&transport.requests);
        assert_eq!(requests.len(), BusinessCommand::ALL.len());
        for request in requests.iter() {
            let route = request.command.route();
            assert_eq!(request.method, route.method);
            assert_eq!(request.target, route.target);
            assert_eq!(request.path, route.path);
            assert_eq!(
                request.authenticated,
                route.authentication == BusinessAuthentication::RustToken
            );
            assert_eq!(request.body_bytes > 0, route.method == BusinessMethod::Post);
        }
        assert_eq!(access_loads.load(Ordering::Relaxed), 5);
    }

    #[test]
    fn authenticated_routes_fail_before_transport_when_the_token_is_missing() {
        let transport = Arc::new(MockTransport::default());
        let client =
            BusinessCommandClient::new(Arc::clone(&transport), MemorySecretBackend::default());
        let request = BusinessCommandRequest::without_body(BusinessCommand::Account).unwrap();
        assert_eq!(
            client.execute(request).unwrap_err(),
            BusinessClientError::AuthenticationRequired
        );
        assert!(lock(&transport.requests).is_empty());
    }

    #[test]
    fn request_shape_and_debug_output_fail_closed() {
        assert_eq!(
            BusinessCommandRequest::without_body(BusinessCommand::Login).unwrap_err(),
            BusinessClientError::InvalidRequest
        );
        assert_eq!(
            BusinessCommandRequest::json(BusinessCommand::Config, &json!({})).unwrap_err(),
            BusinessClientError::InvalidRequest
        );

        let request = BusinessCommandRequest::json(
            BusinessCommand::Login,
            &json!({ "password": "do-not-print" }),
        )
        .unwrap();
        let debug = format!("{request:?}");
        assert!(!debug.contains("do-not-print"));
        assert!(debug.contains("body_bytes"));
    }

    #[test]
    fn subscription_download_uses_only_the_allowlisted_control_plane_target() {
        let transport = Arc::new(MockTransport::default());
        let backend = MemorySecretBackend::with_subscription(
            b"https://subscriptions.example:443/client/subscribe?token=fixture",
        );
        let client = BusinessCommandClient::new(Arc::clone(&transport), backend);

        let body = client.download_subscription().unwrap();
        assert_eq!(body.as_slice(), b"subscription.fixture");
        assert!(lock(&transport.requests).is_empty());
        assert_eq!(
            lock(&transport.subscription_requests).as_slice(),
            &[ObservedSubscriptionRequest {
                host: "subscriptions.example".to_owned(),
                path_and_query: "/client/subscribe?token=fixture".to_owned(),
            }]
        );

        let debug = format!(
            "{:?}",
            BootstrapSubscriptionRequest {
                host: "subscriptions.example",
                path_and_query: "/client/subscribe?token=do-not-print",
            }
        );
        assert!(!debug.contains("do-not-print"));
        assert!(!debug.contains("/client/subscribe"));
    }

    #[test]
    fn subscription_download_fails_before_transport_for_unsafe_or_missing_targets() {
        let invalid_targets: &[&[u8]] = &[
            b"http://subscriptions.example/client/subscribe?token=fixture",
            b"https://subscriptions.example:8443/client/subscribe?token=fixture",
            b"https://user@subscriptions.example/client/subscribe?token=fixture",
            b"https://subscriptions.example/client/subscribe?token=fixture#fragment",
            b"https://other.example/client/subscribe?token=fixture",
            b"https://subscriptions.example//client/subscribe?token=fixture",
            b"https://subscriptions.example/client/subscribe?next=https://other.example",
            b"not-a-url",
            b"\xff\xfe",
        ];

        for target in invalid_targets {
            let transport = Arc::new(MockTransport::default());
            let backend = MemorySecretBackend::with_subscription(target);
            let client = BusinessCommandClient::new(Arc::clone(&transport), backend);
            assert_eq!(
                client.download_subscription().unwrap_err(),
                BusinessClientError::InvalidRequest
            );
            assert!(lock(&transport.subscription_requests).is_empty());
        }

        let transport = Arc::new(MockTransport::default());
        let client =
            BusinessCommandClient::new(Arc::clone(&transport), MemorySecretBackend::default());
        assert_eq!(
            client.download_subscription().unwrap_err(),
            BusinessClientError::AuthenticationRequired
        );
        assert!(lock(&transport.subscription_requests).is_empty());
    }

    #[test]
    fn subscription_download_preserves_http_and_transport_error_mapping() {
        let transport = Arc::new(MockTransport::default());
        let backend = MemorySecretBackend::with_subscription(
            b"https://subscriptions.example/client/subscribe?token=fixture",
        );
        let client = BusinessCommandClient::new(Arc::clone(&transport), backend);

        *lock(&transport.outcome) = MockOutcome::Response(302);
        assert_eq!(
            client.download_subscription().unwrap_err(),
            BusinessClientError::RedirectDenied
        );
        *lock(&transport.outcome) = MockOutcome::Error(BootstrapTransportError::Timeout);
        assert_eq!(
            client.download_subscription().unwrap_err(),
            BusinessClientError::Transport(BootstrapTransportError::Timeout)
        );
    }

    #[test]
    fn fixture_covers_http_and_transport_error_mapping() {
        let document: ErrorDocument = serde_json::from_str(ERROR_FIXTURE).unwrap();
        assert_eq!(document.schema_version, BOOTSTRAP_TRANSPORT_SCHEMA_VERSION);
        let transport = Arc::new(MockTransport::default());
        let client =
            BusinessCommandClient::new(Arc::clone(&transport), MemorySecretBackend::default());

        for case in document.cases {
            let expected = match &case {
                ErrorFixture::Http { expected, .. } | ErrorFixture::Transport { expected, .. } => {
                    expected.clone()
                }
            };
            *lock(&transport.outcome) = match case {
                ErrorFixture::Http { status_code, .. } => MockOutcome::Response(status_code),
                ErrorFixture::Transport { source, .. } => {
                    MockOutcome::Error(transport_error(&source))
                }
            };
            let request =
                BusinessCommandRequest::json(BusinessCommand::Login, &json!({ "fixture": true }))
                    .unwrap();
            assert_eq!(client.execute(request).unwrap_err().as_str(), expected);
        }
    }

    fn transport_error(source: &str) -> BootstrapTransportError {
        match source {
            "invalid_request" => BootstrapTransportError::InvalidRequest,
            "invalid_response" => BootstrapTransportError::InvalidResponse,
            "unavailable" => BootstrapTransportError::Unavailable,
            "timeout" => BootstrapTransportError::Timeout,
            "cancelled" => BootstrapTransportError::Cancelled,
            "dns_failure" => BootstrapTransportError::DnsFailure,
            "tls_failure" => BootstrapTransportError::TlsFailure,
            "response_too_large" => BootstrapTransportError::ResponseTooLarge,
            _ => panic!("unknown transport error fixture"),
        }
    }

    fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
        mutex
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

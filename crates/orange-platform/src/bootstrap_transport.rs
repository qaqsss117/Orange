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
const MAX_BUSINESS_PATH_BYTES: usize = 8192;
const MAX_SUBSCRIPTION_TARGET_BYTES: usize = 8192;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BusinessCommand {
    Login,
    Register,
    SendEmailVerification,
    ResetPassword,
    Config,
    Subscription,
    Account,
    Notices,
    Plans,
    Orders,
    OrderDetail,
    PaymentMethods,
    CheckoutOrder,
    CancelOrder,
    CreateOrder,
    InvitationCenter,
    GenerateInvitationCode,
    Tickets,
    TicketDetail,
    CreateTicket,
    ReplyTicket,
    CloseTicket,
    ResetSubscription,
    GiftCardCheck,
    GiftCardRedeem,
    GiftCardHistory,
    CommissionConfig,
    WithdrawCommission,
    TransferCommission,
    ActiveSessions,
    RemoveActiveSession,
    TelegramBotInfo,
    KnowledgeFetch,
    Update,
}

impl BusinessCommand {
    pub const ALL: [Self; 34] = [
        Self::Login,
        Self::Register,
        Self::SendEmailVerification,
        Self::ResetPassword,
        Self::Config,
        Self::Subscription,
        Self::Account,
        Self::Notices,
        Self::Plans,
        Self::Orders,
        Self::OrderDetail,
        Self::PaymentMethods,
        Self::CheckoutOrder,
        Self::CancelOrder,
        Self::CreateOrder,
        Self::InvitationCenter,
        Self::GenerateInvitationCode,
        Self::Tickets,
        Self::TicketDetail,
        Self::CreateTicket,
        Self::ReplyTicket,
        Self::CloseTicket,
        Self::ResetSubscription,
        Self::GiftCardCheck,
        Self::GiftCardRedeem,
        Self::GiftCardHistory,
        Self::CommissionConfig,
        Self::WithdrawCommission,
        Self::TransferCommission,
        Self::ActiveSessions,
        Self::RemoveActiveSession,
        Self::TelegramBotInfo,
        Self::KnowledgeFetch,
        Self::Update,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Login => "login",
            Self::Register => "register",
            Self::SendEmailVerification => "send_email_verification",
            Self::ResetPassword => "reset_password",
            Self::Config => "config",
            Self::Subscription => "subscription",
            Self::Account => "account",
            Self::Notices => "notices",
            Self::Plans => "plans",
            Self::Orders => "orders",
            Self::OrderDetail => "order_detail",
            Self::PaymentMethods => "payment_methods",
            Self::CheckoutOrder => "checkout_order",
            Self::CancelOrder => "cancel_order",
            Self::CreateOrder => "create_order",
            Self::InvitationCenter => "invitation_center",
            Self::GenerateInvitationCode => "generate_invitation_code",
            Self::Tickets => "tickets",
            Self::TicketDetail => "ticket_detail",
            Self::CreateTicket => "create_ticket",
            Self::ReplyTicket => "reply_ticket",
            Self::CloseTicket => "close_ticket",
            Self::ResetSubscription => "reset_subscription",
            Self::GiftCardCheck => "gift_card_check",
            Self::GiftCardRedeem => "gift_card_redeem",
            Self::GiftCardHistory => "gift_card_history",
            Self::CommissionConfig => "commission_config",
            Self::WithdrawCommission => "withdraw_commission",
            Self::TransferCommission => "transfer_commission",
            Self::ActiveSessions => "active_sessions",
            Self::RemoveActiveSession => "remove_active_session",
            Self::TelegramBotInfo => "telegram_bot_info",
            Self::KnowledgeFetch => "knowledge_fetch",
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
                "/api/v1/passport/auth/register",
                BusinessAuthentication::None,
            ),
            Self::SendEmailVerification => BusinessRoute::post(
                self,
                "/api/v1/passport/comm/sendEmailVerify",
                BusinessAuthentication::None,
            ),
            Self::ResetPassword => BusinessRoute::post(
                self,
                "/api/v1/passport/auth/forget",
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
            Self::Notices => BusinessRoute::get(
                self,
                "/api/v1/user/notice/fetch",
                BusinessAuthentication::RustToken,
            ),
            Self::Plans => BusinessRoute::get(
                self,
                "/api/v1/user/plan/fetch",
                BusinessAuthentication::RustToken,
            ),
            Self::Orders => BusinessRoute::get(
                self,
                "/api/v1/user/order/fetch",
                BusinessAuthentication::RustToken,
            ),
            Self::OrderDetail => BusinessRoute::get(
                self,
                "/api/v1/user/order/detail",
                BusinessAuthentication::RustToken,
            ),
            Self::PaymentMethods => BusinessRoute::get(
                self,
                "/api/v1/user/order/getPaymentMethod",
                BusinessAuthentication::RustToken,
            ),
            Self::CheckoutOrder => BusinessRoute::post(
                self,
                "/api/v1/user/order/checkout",
                BusinessAuthentication::RustToken,
            ),
            Self::CancelOrder => BusinessRoute::post(
                self,
                "/api/v1/user/order/cancel",
                BusinessAuthentication::RustToken,
            ),
            Self::CreateOrder => BusinessRoute::post(
                self,
                "/api/v1/user/order/save",
                BusinessAuthentication::RustToken,
            ),
            Self::InvitationCenter => BusinessRoute::get(
                self,
                "/api/v1/user/invite/fetch",
                BusinessAuthentication::RustToken,
            ),
            Self::GenerateInvitationCode => BusinessRoute::get(
                self,
                "/api/v1/user/invite/save",
                BusinessAuthentication::RustToken,
            ),
            Self::Tickets => BusinessRoute::get(
                self,
                "/api/v1/user/ticket/fetch",
                BusinessAuthentication::RustToken,
            ),
            Self::TicketDetail => BusinessRoute::get(
                self,
                "/api/v1/user/ticket/fetch",
                BusinessAuthentication::RustToken,
            ),
            Self::CreateTicket => BusinessRoute::post(
                self,
                "/api/v1/user/ticket/save",
                BusinessAuthentication::RustToken,
            ),
            Self::ReplyTicket => BusinessRoute::post(
                self,
                "/api/v1/user/ticket/reply",
                BusinessAuthentication::RustToken,
            ),
            Self::CloseTicket => BusinessRoute::post(
                self,
                "/api/v1/user/ticket/close",
                BusinessAuthentication::RustToken,
            ),
            Self::ResetSubscription => BusinessRoute::get(
                self,
                "/api/v1/user/resetSecurity",
                BusinessAuthentication::RustToken,
            ),
            Self::GiftCardCheck => BusinessRoute::post(
                self,
                "/api/v1/user/gift-card/check",
                BusinessAuthentication::RustToken,
            ),
            Self::GiftCardRedeem => BusinessRoute::post(
                self,
                "/api/v1/user/gift-card/redeem",
                BusinessAuthentication::RustToken,
            ),
            Self::GiftCardHistory => BusinessRoute::get(
                self,
                "/api/v1/user/gift-card/history",
                BusinessAuthentication::RustToken,
            ),
            Self::CommissionConfig => BusinessRoute::get(
                self,
                "/api/v1/user/comm/config",
                BusinessAuthentication::RustToken,
            ),
            Self::WithdrawCommission => BusinessRoute::post(
                self,
                "/api/v1/user/ticket/withdraw",
                BusinessAuthentication::RustToken,
            ),
            Self::TransferCommission => BusinessRoute::post(
                self,
                "/api/v1/user/transfer",
                BusinessAuthentication::RustToken,
            ),
            Self::ActiveSessions => BusinessRoute::get(
                self,
                "/api/v1/user/getActiveSession",
                BusinessAuthentication::RustToken,
            ),
            Self::RemoveActiveSession => BusinessRoute::post(
                self,
                "/api/v1/user/removeActiveSession",
                BusinessAuthentication::RustToken,
            ),
            Self::TelegramBotInfo => BusinessRoute::get(
                self,
                "/api/v1/user/telegram/getBotInfo",
                BusinessAuthentication::RustToken,
            ),
            Self::KnowledgeFetch => BusinessRoute::get(
                self,
                "/api/v1/user/knowledge/fetch",
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
    path_and_query: String,
    body: Vec<u8>,
}

impl BusinessCommandRequest {
    pub fn without_body(command: BusinessCommand) -> Result<Self, BusinessClientError> {
        if command.route().method() != BusinessMethod::Get {
            return Err(BusinessClientError::InvalidRequest);
        }
        Ok(Self {
            command,
            path_and_query: command.route().path().to_owned(),
            body: Vec::new(),
        })
    }

    pub fn with_query_parameter(
        command: BusinessCommand,
        name: &str,
        value: &str,
    ) -> Result<Self, BusinessClientError> {
        Self::query_parameter(command, BusinessMethod::Get, name, value)
    }

    pub fn post_with_query_parameter(
        command: BusinessCommand,
        name: &str,
        value: &str,
    ) -> Result<Self, BusinessClientError> {
        Self::query_parameter(command, BusinessMethod::Post, name, value)
    }

    pub fn post_with_query_parameters(
        command: BusinessCommand,
        parameters: &[(&str, &str)],
    ) -> Result<Self, BusinessClientError> {
        Self::query_parameters(command, BusinessMethod::Post, parameters)
    }

    fn query_parameter(
        command: BusinessCommand,
        expected_method: BusinessMethod,
        name: &str,
        value: &str,
    ) -> Result<Self, BusinessClientError> {
        Self::query_parameters(command, expected_method, &[(name, value)])
    }

    fn query_parameters(
        command: BusinessCommand,
        expected_method: BusinessMethod,
        parameters: &[(&str, &str)],
    ) -> Result<Self, BusinessClientError> {
        let route = command.route();
        if route.method() != expected_method
            || parameters.is_empty()
            || parameters.len() > 8
            || parameters.iter().enumerate().any(|(index, (name, _))| {
                name.is_empty()
                    || name.len() > 64
                    || !name
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
                    || parameters[..index]
                        .iter()
                        .any(|(previous, _)| previous == name)
            })
        {
            return Err(BusinessClientError::InvalidRequest);
        }
        let mut serializer = url::form_urlencoded::Serializer::new(String::new());
        for (name, value) in parameters {
            serializer.append_pair(name, value);
        }
        let query = serializer.finish();
        let path_and_query = format!("{}?{query}", route.path());
        if path_and_query.len() > MAX_BUSINESS_PATH_BYTES {
            return Err(BusinessClientError::InvalidRequest);
        }
        Ok(Self {
            command,
            path_and_query,
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
        Ok(Self {
            command,
            path_and_query: command.route().path().to_owned(),
            body,
        })
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
            .field("path_bytes", &self.path_and_query.len())
            .field("body_bytes", &self.body.len())
            .finish()
    }
}

pub struct BootstrapTransportRequest<'a> {
    route: BusinessRoute,
    path_and_query: &'a str,
    body: &'a [u8],
    access_token: Option<&'a [u8]>,
}

impl BootstrapTransportRequest<'_> {
    pub const fn route(&self) -> BusinessRoute {
        self.route
    }

    pub const fn path_and_query(&self) -> &str {
        self.path_and_query
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
            .field("path_bytes", &self.path_and_query.len())
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
                    path_and_query: &request.path_and_query,
                    body: &request.body,
                    access_token: Some(token),
                })
            }),
            None => self.transport.execute(BootstrapTransportRequest {
                route,
                path_and_query: &request.path_and_query,
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

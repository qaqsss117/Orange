use std::fmt;

use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::{
    BUSINESS_API_SCHEMA_VERSION, CommandError, ConnectionMode, ControlPlaneState,
    CreateOrderRequest, CreatePaymentRequest, CreateTicketRequest, DOMAIN_SCHEMA_VERSION,
    DataPlaneState, ErrorCode, LoginRequest, NodeLoadState, RegisterRequest, ReplyTicketRequest,
    ResetPasswordRequest, RoutingMode, SendEmailVerificationRequest, SubscriptionPublicResponse,
};

pub const GET_PLANE_STATE_COMMAND: &str = "get_plane_state";
pub const GET_RUNTIME_INFO_COMMAND: &str = "get_runtime_info";
pub const GET_DATA_PLANE_EVENT_SNAPSHOT_COMMAND: &str = "get_data_plane_event_snapshot";
pub const CONTROL_DATA_PLANE_COMMAND: &str = "control_data_plane";
pub const GET_CONNECTION_MODE_COMMAND: &str = "get_connection_mode";
pub const SET_CONNECTION_MODE_COMMAND: &str = "set_connection_mode";
pub const GET_ROUTING_MODE_COMMAND: &str = "get_routing_mode";
pub const SET_ROUTING_MODE_COMMAND: &str = "set_routing_mode";
pub const OPEN_NETWORK_TOOL_COMMAND: &str = "open_network_tool";
pub const OPEN_LEGAL_DOCUMENT_COMMAND: &str = "open_legal_document";
pub const OPEN_SERVICE_PORTAL_COMMAND: &str = "open_service_portal";
pub const GET_LAUNCH_ON_STARTUP_COMMAND: &str = "get_launch_on_startup";
pub const SET_LAUNCH_ON_STARTUP_COMMAND: &str = "set_launch_on_startup";
pub const INITIALIZE_BUSINESS_COMMAND: &str = "initialize_business";
pub const LOGIN_COMMAND: &str = "login";
pub const REGISTER_COMMAND: &str = "register";
pub const SEND_EMAIL_VERIFICATION_COMMAND: &str = "send_email_verification";
pub const RESET_PASSWORD_COMMAND: &str = "reset_password";
pub const GET_AUTH_SESSION_COMMAND: &str = "get_auth_session";
pub const LOGOUT_COMMAND: &str = "logout";
pub const REFRESH_ACCOUNT_COMMAND: &str = "refresh_account";
pub const FETCH_NOTICES_COMMAND: &str = "fetch_notices";
pub const FETCH_PLANS_COMMAND: &str = "fetch_plans";
pub const FETCH_ORDERS_COMMAND: &str = "fetch_orders";
pub const FETCH_ORDER_DETAIL_COMMAND: &str = "fetch_order_detail";
pub const FETCH_PAYMENT_METHODS_COMMAND: &str = "fetch_payment_methods";
pub const CHECKOUT_ORDER_COMMAND: &str = "checkout_order";
pub const CANCEL_ORDER_COMMAND: &str = "cancel_order";
pub const CREATE_ORDER_COMMAND: &str = "create_order";
pub const FETCH_INVITATION_CENTER_COMMAND: &str = "fetch_invitation_center";
pub const GENERATE_INVITATION_CODE_COMMAND: &str = "generate_invitation_code";
pub const FETCH_TICKETS_COMMAND: &str = "fetch_tickets";
pub const FETCH_TICKET_DETAIL_COMMAND: &str = "fetch_ticket_detail";
pub const CREATE_TICKET_COMMAND: &str = "create_ticket";
pub const REPLY_TICKET_COMMAND: &str = "reply_ticket";
pub const CLOSE_TICKET_COMMAND: &str = "close_ticket";
pub const REFRESH_SUBSCRIPTION_COMMAND: &str = "refresh_subscription";
pub const GET_SUBSCRIPTION_SNAPSHOT_COMMAND: &str = "get_subscription_snapshot";
pub const GET_NODE_CATALOG_COMMAND: &str = "get_node_catalog";
pub const SELECT_NODE_COMMAND: &str = "select_node";
pub const SET_NODE_SELECTION_MODE_COMMAND: &str = "set_node_selection_mode";
pub const TEST_NODE_DELAYS_COMMAND: &str = "test_node_delays";
pub const BASE_COMMANDS: &[&str] = &[GET_PLANE_STATE_COMMAND, GET_RUNTIME_INFO_COMMAND];
pub const DESKTOP_OBSERVABILITY_COMMANDS: &[&str] = &[GET_DATA_PLANE_EVENT_SNAPSHOT_COMMAND];
pub const DESKTOP_SETTINGS_COMMANDS: &[&str] = &[
    GET_LAUNCH_ON_STARTUP_COMMAND,
    SET_LAUNCH_ON_STARTUP_COMMAND,
    OPEN_NETWORK_TOOL_COMMAND,
    OPEN_LEGAL_DOCUMENT_COMMAND,
];
pub const DESKTOP_DATA_PLANE_COMMANDS: &[&str] = &[
    CONTROL_DATA_PLANE_COMMAND,
    GET_CONNECTION_MODE_COMMAND,
    SET_CONNECTION_MODE_COMMAND,
    GET_ROUTING_MODE_COMMAND,
    SET_ROUTING_MODE_COMMAND,
    GET_NODE_CATALOG_COMMAND,
    SELECT_NODE_COMMAND,
    SET_NODE_SELECTION_MODE_COMMAND,
    TEST_NODE_DELAYS_COMMAND,
];
pub const DESKTOP_BUSINESS_COMMANDS: &[&str] = &[
    INITIALIZE_BUSINESS_COMMAND,
    OPEN_SERVICE_PORTAL_COMMAND,
    LOGIN_COMMAND,
    REGISTER_COMMAND,
    SEND_EMAIL_VERIFICATION_COMMAND,
    RESET_PASSWORD_COMMAND,
    GET_AUTH_SESSION_COMMAND,
    LOGOUT_COMMAND,
    REFRESH_ACCOUNT_COMMAND,
    FETCH_NOTICES_COMMAND,
    FETCH_PLANS_COMMAND,
    FETCH_ORDERS_COMMAND,
    FETCH_ORDER_DETAIL_COMMAND,
    FETCH_PAYMENT_METHODS_COMMAND,
    CHECKOUT_ORDER_COMMAND,
    CANCEL_ORDER_COMMAND,
    CREATE_ORDER_COMMAND,
    FETCH_INVITATION_CENTER_COMMAND,
    GENERATE_INVITATION_CODE_COMMAND,
    FETCH_TICKETS_COMMAND,
    FETCH_TICKET_DETAIL_COMMAND,
    CREATE_TICKET_COMMAND,
    REPLY_TICKET_COMMAND,
    CLOSE_TICKET_COMMAND,
    REFRESH_SUBSCRIPTION_COMMAND,
    GET_SUBSCRIPTION_SNAPSHOT_COMMAND,
];
pub const REGISTERED_COMMANDS: &[&str] = &[
    GET_PLANE_STATE_COMMAND,
    GET_RUNTIME_INFO_COMMAND,
    GET_DATA_PLANE_EVENT_SNAPSHOT_COMMAND,
    CONTROL_DATA_PLANE_COMMAND,
    GET_CONNECTION_MODE_COMMAND,
    SET_CONNECTION_MODE_COMMAND,
    GET_ROUTING_MODE_COMMAND,
    SET_ROUTING_MODE_COMMAND,
    GET_LAUNCH_ON_STARTUP_COMMAND,
    SET_LAUNCH_ON_STARTUP_COMMAND,
    OPEN_NETWORK_TOOL_COMMAND,
    OPEN_LEGAL_DOCUMENT_COMMAND,
    INITIALIZE_BUSINESS_COMMAND,
    OPEN_SERVICE_PORTAL_COMMAND,
    LOGIN_COMMAND,
    REGISTER_COMMAND,
    SEND_EMAIL_VERIFICATION_COMMAND,
    RESET_PASSWORD_COMMAND,
    GET_AUTH_SESSION_COMMAND,
    LOGOUT_COMMAND,
    REFRESH_ACCOUNT_COMMAND,
    FETCH_NOTICES_COMMAND,
    FETCH_PLANS_COMMAND,
    FETCH_ORDERS_COMMAND,
    FETCH_ORDER_DETAIL_COMMAND,
    FETCH_PAYMENT_METHODS_COMMAND,
    CHECKOUT_ORDER_COMMAND,
    CANCEL_ORDER_COMMAND,
    CREATE_ORDER_COMMAND,
    FETCH_INVITATION_CENTER_COMMAND,
    GENERATE_INVITATION_CODE_COMMAND,
    FETCH_TICKETS_COMMAND,
    FETCH_TICKET_DETAIL_COMMAND,
    CREATE_TICKET_COMMAND,
    REPLY_TICKET_COMMAND,
    CLOSE_TICKET_COMMAND,
    REFRESH_SUBSCRIPTION_COMMAND,
    GET_SUBSCRIPTION_SNAPSHOT_COMMAND,
    GET_NODE_CATALOG_COMMAND,
    SELECT_NODE_COMMAND,
    SET_NODE_SELECTION_MODE_COMMAND,
    TEST_NODE_DELAYS_COMMAND,
];

pub fn is_registered_command(command: &str) -> bool {
    REGISTERED_COMMANDS.contains(&command)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NetworkTool {
    IpLookup,
    SpeedTest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OpenNetworkToolRequest {
    pub schema_version: u16,
    pub tool: NetworkTool,
}

impl OpenNetworkToolRequest {
    pub const fn current(tool: NetworkTool) -> Self {
        Self {
            schema_version: DOMAIN_SCHEMA_VERSION,
            tool,
        }
    }

    pub fn validate(self) -> Result<Self, CommandError> {
        validate_schema_version(self.schema_version)?;
        Ok(self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenNetworkToolResponse {
    pub schema_version: u16,
    pub tool: NetworkTool,
}

impl OpenNetworkToolResponse {
    pub const fn opened(tool: NetworkTool) -> Self {
        Self {
            schema_version: DOMAIN_SCHEMA_VERSION,
            tool,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LegalDocument {
    TermsOfService,
    PrivacyPolicy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OpenLegalDocumentRequest {
    pub schema_version: u16,
    pub document: LegalDocument,
}

impl OpenLegalDocumentRequest {
    pub const fn current(document: LegalDocument) -> Self {
        Self {
            schema_version: DOMAIN_SCHEMA_VERSION,
            document,
        }
    }

    pub fn validate(self) -> Result<Self, CommandError> {
        validate_schema_version(self.schema_version)?;
        Ok(self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenLegalDocumentResponse {
    pub schema_version: u16,
    pub document: LegalDocument,
}

impl OpenLegalDocumentResponse {
    pub const fn opened(document: LegalDocument) -> Self {
        Self {
            schema_version: DOMAIN_SCHEMA_VERSION,
            document,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InitializeBusinessRequest {
    pub schema_version: u16,
}

impl InitializeBusinessRequest {
    pub const fn current() -> Self {
        Self {
            schema_version: DOMAIN_SCHEMA_VERSION,
        }
    }

    pub fn validate(self) -> Result<Self, CommandError> {
        validate_schema_version(self.schema_version)?;
        Ok(self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OpenServicePortalRequest {
    pub schema_version: u16,
}

impl OpenServicePortalRequest {
    pub const fn current() -> Self {
        Self {
            schema_version: DOMAIN_SCHEMA_VERSION,
        }
    }

    pub fn validate(self) -> Result<Self, CommandError> {
        validate_schema_version(self.schema_version)?;
        Ok(self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenServicePortalResponse {
    pub schema_version: u16,
    pub opened: bool,
}

impl OpenServicePortalResponse {
    pub const fn opened() -> Self {
        Self {
            schema_version: DOMAIN_SCHEMA_VERSION,
            opened: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ServicePortalUrlResponse {
    pub schema_version: u16,
    pub url: String,
}

impl ServicePortalUrlResponse {
    pub fn new(url: String) -> Self {
        Self {
            schema_version: DOMAIN_SCHEMA_VERSION,
            url,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AuthSessionRequest {
    pub schema_version: u16,
}

impl AuthSessionRequest {
    pub const fn current() -> Self {
        Self {
            schema_version: DOMAIN_SCHEMA_VERSION,
        }
    }

    pub fn validate(self) -> Result<Self, CommandError> {
        validate_schema_version(self.schema_version)?;
        Ok(self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LogoutRequest {
    pub schema_version: u16,
}

impl LogoutRequest {
    pub const fn current() -> Self {
        Self {
            schema_version: DOMAIN_SCHEMA_VERSION,
        }
    }

    pub fn validate(self) -> Result<Self, CommandError> {
        validate_schema_version(self.schema_version)?;
        Ok(self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AccountRefreshRequest {
    pub schema_version: u16,
}

impl AccountRefreshRequest {
    pub const fn current() -> Self {
        Self {
            schema_version: DOMAIN_SCHEMA_VERSION,
        }
    }

    pub fn validate(self) -> Result<Self, CommandError> {
        validate_schema_version(self.schema_version)?;
        Ok(self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NoticesRequest {
    pub schema_version: u16,
}

impl NoticesRequest {
    pub const fn current() -> Self {
        Self {
            schema_version: DOMAIN_SCHEMA_VERSION,
        }
    }

    pub fn validate(self) -> Result<Self, CommandError> {
        validate_schema_version(self.schema_version)?;
        Ok(self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PlansRequest {
    pub schema_version: u16,
}

impl PlansRequest {
    pub const fn current() -> Self {
        Self {
            schema_version: DOMAIN_SCHEMA_VERSION,
        }
    }

    pub fn validate(self) -> Result<Self, CommandError> {
        validate_schema_version(self.schema_version)?;
        Ok(self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OrdersRequest {
    pub schema_version: u16,
}

impl OrdersRequest {
    pub const fn current() -> Self {
        Self {
            schema_version: DOMAIN_SCHEMA_VERSION,
        }
    }

    pub fn validate(self) -> Result<Self, CommandError> {
        validate_schema_version(self.schema_version)?;
        Ok(self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OrderDetailCommandRequest {
    pub schema_version: u16,
    pub order_id: String,
}

impl OrderDetailCommandRequest {
    pub fn validate(self) -> Result<String, CommandError> {
        validate_schema_version(self.schema_version)?;
        if !valid_order_id(&self.order_id) {
            return Err(CommandError::from_code(ErrorCode::Validation));
        }
        Ok(self.order_id)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PaymentMethodsRequest {
    pub schema_version: u16,
}

impl PaymentMethodsRequest {
    pub const fn current() -> Self {
        Self {
            schema_version: DOMAIN_SCHEMA_VERSION,
        }
    }

    pub fn validate(self) -> Result<Self, CommandError> {
        validate_schema_version(self.schema_version)?;
        Ok(self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InvitationCenterRequest {
    pub schema_version: u16,
}

impl InvitationCenterRequest {
    pub const fn current() -> Self {
        Self {
            schema_version: DOMAIN_SCHEMA_VERSION,
        }
    }

    pub fn validate(self) -> Result<Self, CommandError> {
        validate_schema_version(self.schema_version)?;
        Ok(self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TicketsRequest {
    pub schema_version: u16,
}

impl TicketsRequest {
    pub const fn current() -> Self {
        Self {
            schema_version: DOMAIN_SCHEMA_VERSION,
        }
    }

    pub fn validate(self) -> Result<Self, CommandError> {
        validate_schema_version(self.schema_version)?;
        Ok(self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TicketDetailCommandRequest {
    pub schema_version: u16,
    pub ticket_id: String,
}

impl TicketDetailCommandRequest {
    pub fn validate(self) -> Result<String, CommandError> {
        validate_schema_version(self.schema_version)?;
        if !valid_ticket_id(&self.ticket_id) {
            return Err(CommandError::from_code(ErrorCode::Validation));
        }
        Ok(self.ticket_id)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateTicketCommandRequest {
    pub schema_version: u16,
    pub subject: String,
    pub message: String,
}

impl CreateTicketCommandRequest {
    pub fn validate(self) -> Result<CreateTicketRequest, CommandError> {
        validate_schema_version(self.schema_version)?;
        let subject = self.subject.trim().to_owned();
        let message = self.message.trim().to_owned();
        if !valid_ticket_subject(&subject) || !valid_ticket_message(&message) {
            return Err(CommandError::from_code(ErrorCode::Validation));
        }
        Ok(CreateTicketRequest {
            schema_version: BUSINESS_API_SCHEMA_VERSION,
            subject,
            message,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReplyTicketCommandRequest {
    pub schema_version: u16,
    pub ticket_id: String,
    pub message: String,
}

impl ReplyTicketCommandRequest {
    pub fn validate(self) -> Result<ReplyTicketRequest, CommandError> {
        validate_schema_version(self.schema_version)?;
        let message = self.message.trim().to_owned();
        if !valid_ticket_id(&self.ticket_id) || !valid_ticket_message(&message) {
            return Err(CommandError::from_code(ErrorCode::Validation));
        }
        Ok(ReplyTicketRequest {
            schema_version: BUSINESS_API_SCHEMA_VERSION,
            ticket_id: self.ticket_id,
            message,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CloseTicketCommandRequest {
    pub schema_version: u16,
    pub ticket_id: String,
}

impl CloseTicketCommandRequest {
    pub fn validate(self) -> Result<String, CommandError> {
        validate_schema_version(self.schema_version)?;
        if !valid_ticket_id(&self.ticket_id) {
            return Err(CommandError::from_code(ErrorCode::Validation));
        }
        Ok(self.ticket_id)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CheckoutOrderCommandRequest {
    pub schema_version: u16,
    pub order_id: String,
    pub payment_method: String,
}

impl CheckoutOrderCommandRequest {
    pub fn validate(self) -> Result<CreatePaymentRequest, CommandError> {
        validate_schema_version(self.schema_version)?;
        if !valid_order_id(&self.order_id)
            || self.payment_method.is_empty()
            || self.payment_method.len() > 20
            || !self
                .payment_method
                .bytes()
                .all(|byte| byte.is_ascii_digit())
            || self.payment_method.starts_with('0')
        {
            return Err(CommandError::from_code(ErrorCode::Validation));
        }
        Ok(CreatePaymentRequest {
            schema_version: BUSINESS_API_SCHEMA_VERSION,
            order_id: self.order_id,
            payment_method: self.payment_method,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CancelOrderCommandRequest {
    pub schema_version: u16,
    pub order_id: String,
}

impl CancelOrderCommandRequest {
    pub fn validate(self) -> Result<String, CommandError> {
        validate_schema_version(self.schema_version)?;
        if !valid_order_id(&self.order_id) {
            return Err(CommandError::from_code(ErrorCode::Validation));
        }
        Ok(self.order_id)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateOrderCommandRequest {
    pub schema_version: u16,
    pub plan_id: String,
    #[serde(default)]
    pub coupon_code: Option<String>,
}

impl CreateOrderCommandRequest {
    pub fn validate(self) -> Result<CreateOrderRequest, CommandError> {
        validate_schema_version(self.schema_version)?;
        if self.plan_id.is_empty()
            || self.plan_id.len() > 64
            || !self.plan_id.is_ascii()
            || self.plan_id.bytes().any(|byte| byte.is_ascii_control())
        {
            return Err(CommandError::from_code(ErrorCode::Validation));
        }
        let coupon_code = self
            .coupon_code
            .map(|code| code.trim().to_owned())
            .filter(|code| !code.is_empty());
        if coupon_code.as_ref().is_some_and(|code| {
            code.len() > 64 || !code.is_ascii() || code.bytes().any(|byte| byte.is_ascii_control())
        }) {
            return Err(CommandError::from_code(ErrorCode::Validation));
        }
        Ok(CreateOrderRequest {
            schema_version: BUSINESS_API_SCHEMA_VERSION,
            plan_id: self.plan_id,
            coupon_code,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct KnowledgeListCommandRequest {
    pub schema_version: u16,
    #[serde(default)]
    pub keyword: Option<String>,
}

impl KnowledgeListCommandRequest {
    pub fn validate(self) -> Result<Option<String>, CommandError> {
        validate_schema_version(self.schema_version)?;
        let keyword = self
            .keyword
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty());
        if keyword
            .as_ref()
            .is_some_and(|value| value.len() > 128 || value.chars().any(char::is_control))
        {
            return Err(CommandError::from_code(ErrorCode::Validation));
        }
        Ok(keyword)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct KnowledgeDetailCommandRequest {
    pub schema_version: u16,
    pub article_id: String,
}

impl KnowledgeDetailCommandRequest {
    pub fn validate(self) -> Result<String, CommandError> {
        validate_schema_version(self.schema_version)?;
        let article_id = self.article_id.trim().to_owned();
        if article_id.is_empty()
            || article_id.len() > 32
            || !article_id.bytes().all(|byte| byte.is_ascii_digit())
        {
            return Err(CommandError::from_code(ErrorCode::Validation));
        }
        Ok(article_id)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ActiveSessionsRequest {
    pub schema_version: u16,
}

impl ActiveSessionsRequest {
    pub const fn current() -> Self {
        Self {
            schema_version: DOMAIN_SCHEMA_VERSION,
        }
    }

    pub fn validate(self) -> Result<Self, CommandError> {
        validate_schema_version(self.schema_version)?;
        Ok(self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RemoveActiveSessionCommandRequest {
    pub schema_version: u16,
    pub session_id: String,
}

impl RemoveActiveSessionCommandRequest {
    pub fn validate(self) -> Result<String, CommandError> {
        validate_schema_version(self.schema_version)?;
        let session_id = self.session_id.trim().to_owned();
        if session_id.is_empty()
            || session_id.len() > 32
            || !session_id.bytes().all(|byte| byte.is_ascii_digit())
        {
            return Err(CommandError::from_code(ErrorCode::Validation));
        }
        Ok(session_id)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CommissionConfigRequest {
    pub schema_version: u16,
}

impl CommissionConfigRequest {
    pub const fn current() -> Self {
        Self {
            schema_version: DOMAIN_SCHEMA_VERSION,
        }
    }

    pub fn validate(self) -> Result<Self, CommandError> {
        validate_schema_version(self.schema_version)?;
        Ok(self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WithdrawCommissionCommandRequest {
    pub schema_version: u16,
    pub withdraw_method: String,
    pub withdraw_account: String,
}

impl WithdrawCommissionCommandRequest {
    pub fn validate(self) -> Result<(String, String), CommandError> {
        validate_schema_version(self.schema_version)?;
        let method = self.withdraw_method.trim().to_owned();
        let account = self.withdraw_account.trim().to_owned();
        if method.is_empty()
            || method.len() > 64
            || method.chars().any(char::is_control)
            || account.is_empty()
            || account.len() > 512
            || account.chars().any(char::is_control)
        {
            return Err(CommandError::from_code(ErrorCode::Validation));
        }
        Ok((method, account))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TransferCommissionCommandRequest {
    pub schema_version: u16,
    pub amount_minor: u64,
}

impl TransferCommissionCommandRequest {
    pub fn validate(self) -> Result<u64, CommandError> {
        validate_schema_version(self.schema_version)?;
        if self.amount_minor == 0 {
            return Err(CommandError::from_code(ErrorCode::Validation));
        }
        Ok(self.amount_minor)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GiftCardCodeCommandRequest {
    pub schema_version: u16,
    pub code: String,
}

impl GiftCardCodeCommandRequest {
    pub fn validate(self) -> Result<String, CommandError> {
        validate_schema_version(self.schema_version)?;
        let code = self.code.trim().to_owned();
        if code.len() < 8
            || code.len() > 64
            || !code.is_ascii()
            || code.bytes().any(|byte| byte.is_ascii_control())
        {
            return Err(CommandError::from_code(ErrorCode::Validation));
        }
        Ok(code)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GiftCardHistoryRequest {
    pub schema_version: u16,
}

impl GiftCardHistoryRequest {
    pub const fn current() -> Self {
        Self {
            schema_version: DOMAIN_SCHEMA_VERSION,
        }
    }

    pub fn validate(self) -> Result<Self, CommandError> {
        validate_schema_version(self.schema_version)?;
        Ok(self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SubscriptionRefreshRequest {
    pub schema_version: u16,
}

impl SubscriptionRefreshRequest {
    pub const fn current() -> Self {
        Self {
            schema_version: DOMAIN_SCHEMA_VERSION,
        }
    }

    pub fn validate(self) -> Result<Self, CommandError> {
        validate_schema_version(self.schema_version)?;
        Ok(self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SubscriptionSnapshotRequest {
    pub schema_version: u16,
}

impl SubscriptionSnapshotRequest {
    pub const fn current() -> Self {
        Self {
            schema_version: DOMAIN_SCHEMA_VERSION,
        }
    }

    pub fn validate(self) -> Result<Self, CommandError> {
        validate_schema_version(self.schema_version)?;
        Ok(self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubscriptionSnapshotResponse {
    pub schema_version: u16,
    pub subscription: Option<SubscriptionPublicResponse>,
    pub local_revision: Option<u64>,
}

impl SubscriptionSnapshotResponse {
    pub const fn new(
        subscription: Option<SubscriptionPublicResponse>,
        local_revision: Option<u64>,
    ) -> Self {
        Self {
            schema_version: DOMAIN_SCHEMA_VERSION,
            subscription,
            local_revision,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NodeCatalogRequest {
    pub schema_version: u16,
}

impl NodeCatalogRequest {
    pub const fn current() -> Self {
        Self {
            schema_version: DOMAIN_SCHEMA_VERSION,
        }
    }

    pub fn validate(self) -> Result<Self, CommandError> {
        validate_schema_version(self.schema_version)?;
        Ok(self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PublicNodeProtocol {
    Shadowsocks,
    Trojan,
    Hysteria2,
    Vless,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeSelectionMode {
    Auto,
    Manual,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PublicNode {
    pub id: String,
    pub name: String,
    pub protocol: PublicNodeProtocol,
    pub load_state: NodeLoadState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PublicNodeGroup {
    pub id: String,
    pub selected_node_id: String,
    pub nodes: Vec<PublicNode>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeCatalogResponse {
    pub schema_version: u16,
    pub revision: Option<u64>,
    pub selection_mode: NodeSelectionMode,
    pub groups: Vec<PublicNodeGroup>,
}

impl NodeCatalogResponse {
    pub const fn new(
        revision: Option<u64>,
        selection_mode: NodeSelectionMode,
        groups: Vec<PublicNodeGroup>,
    ) -> Self {
        Self {
            schema_version: DOMAIN_SCHEMA_VERSION,
            revision,
            selection_mode,
            groups,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SetNodeSelectionModeRequest {
    pub schema_version: u16,
    pub mode: NodeSelectionMode,
}

impl SetNodeSelectionModeRequest {
    pub fn validate(self) -> Result<Self, CommandError> {
        validate_schema_version(self.schema_version)?;
        Ok(self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeSelectionModeResponse {
    pub schema_version: u16,
    pub mode: NodeSelectionMode,
}

impl NodeSelectionModeResponse {
    pub const fn new(mode: NodeSelectionMode) -> Self {
        Self {
            schema_version: DOMAIN_SCHEMA_VERSION,
            mode,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SelectNodeRequest {
    pub schema_version: u16,
    pub selector_id: String,
    pub node_id: String,
}

impl SelectNodeRequest {
    pub fn validate(self) -> Result<Self, CommandError> {
        validate_schema_version(self.schema_version)?;
        if !valid_public_node_id(&self.selector_id) || !valid_public_node_id(&self.node_id) {
            return Err(CommandError::from_code(ErrorCode::Validation));
        }
        Ok(self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectNodeResponse {
    pub schema_version: u16,
    pub selector_id: String,
    pub node_id: String,
    /// True when the selection was only persisted locally because the data plane
    /// core is not running; it takes effect on the next successful connect.
    #[serde(default)]
    pub pending: bool,
}

impl SelectNodeResponse {
    pub fn new(selector_id: impl Into<String>, node_id: impl Into<String>) -> Self {
        Self {
            schema_version: DOMAIN_SCHEMA_VERSION,
            selector_id: selector_id.into(),
            node_id: node_id.into(),
            pending: false,
        }
    }

    pub const fn with_pending(mut self, pending: bool) -> Self {
        self.pending = pending;
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NodeDelayTestRequest {
    pub schema_version: u16,
}

impl NodeDelayTestRequest {
    pub const fn current() -> Self {
        Self {
            schema_version: DOMAIN_SCHEMA_VERSION,
        }
    }

    pub fn validate(self) -> Result<Self, CommandError> {
        validate_schema_version(self.schema_version)?;
        Ok(self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum PublicNodeDelay {
    Available {
        #[serde(rename = "delayMs")]
        delay_ms: u32,
    },
    TimedOut,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PublicNodeDelayResult {
    pub selector_id: String,
    pub node_id: String,
    pub result: PublicNodeDelay,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeDelayTestResponse {
    pub schema_version: u16,
    pub results: Vec<PublicNodeDelayResult>,
}

impl NodeDelayTestResponse {
    pub const fn new(results: Vec<PublicNodeDelayResult>) -> Self {
        Self {
            schema_version: DOMAIN_SCHEMA_VERSION,
            results,
        }
    }
}

fn valid_public_node_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && !value.starts_with("orange-")
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

#[derive(PartialEq, Eq, Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LoginCommandRequest {
    #[zeroize(skip)]
    pub schema_version: u16,
    pub email: String,
    pub password: String,
}

impl LoginCommandRequest {
    pub fn validate(mut self) -> Result<LoginRequest, CommandError> {
        validate_schema_version(self.schema_version)?;
        Ok(LoginRequest {
            schema_version: BUSINESS_API_SCHEMA_VERSION,
            email: std::mem::take(&mut self.email),
            password: std::mem::take(&mut self.password),
        })
    }
}

impl fmt::Debug for LoginCommandRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("LoginCommandRequest")
            .field("schema_version", &self.schema_version)
            .field("email_bytes", &self.email.len())
            .field("password_bytes", &self.password.len())
            .finish()
    }
}

#[derive(PartialEq, Eq, Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SendEmailVerificationCommandRequest {
    #[zeroize(skip)]
    pub schema_version: u16,
    pub email: String,
}

impl SendEmailVerificationCommandRequest {
    pub fn validate(mut self) -> Result<SendEmailVerificationRequest, CommandError> {
        validate_schema_version(self.schema_version)?;
        Ok(SendEmailVerificationRequest {
            schema_version: BUSINESS_API_SCHEMA_VERSION,
            email: std::mem::take(&mut self.email),
        })
    }
}

impl fmt::Debug for SendEmailVerificationCommandRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SendEmailVerificationCommandRequest")
            .field("schema_version", &self.schema_version)
            .field("email_bytes", &self.email.len())
            .finish()
    }
}

#[derive(PartialEq, Eq, Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RegisterCommandRequest {
    #[zeroize(skip)]
    pub schema_version: u16,
    pub email: String,
    pub password: String,
    pub email_code: Option<String>,
    pub invite_code: Option<String>,
}

impl RegisterCommandRequest {
    pub fn validate(mut self) -> Result<RegisterRequest, CommandError> {
        validate_schema_version(self.schema_version)?;
        if self
            .email_code
            .as_deref()
            .is_some_and(|value| !valid_email_verification_code(value))
        {
            return Err(CommandError::from_code(ErrorCode::Validation));
        }
        Ok(RegisterRequest {
            schema_version: BUSINESS_API_SCHEMA_VERSION,
            email: std::mem::take(&mut self.email),
            password: std::mem::take(&mut self.password),
            email_code: std::mem::take(&mut self.email_code),
            invite_code: std::mem::take(&mut self.invite_code),
        })
    }
}

impl fmt::Debug for RegisterCommandRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RegisterCommandRequest")
            .field("schema_version", &self.schema_version)
            .field("email_bytes", &self.email.len())
            .field("password_bytes", &self.password.len())
            .field("has_email_code", &self.email_code.is_some())
            .field("has_invite_code", &self.invite_code.is_some())
            .finish()
    }
}

#[derive(PartialEq, Eq, Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResetPasswordCommandRequest {
    #[zeroize(skip)]
    pub schema_version: u16,
    pub email: String,
    pub password: String,
    pub email_code: String,
}

impl ResetPasswordCommandRequest {
    pub fn validate(mut self) -> Result<ResetPasswordRequest, CommandError> {
        validate_schema_version(self.schema_version)?;
        if !valid_email_verification_code(&self.email_code) {
            return Err(CommandError::from_code(ErrorCode::Validation));
        }
        Ok(ResetPasswordRequest {
            schema_version: BUSINESS_API_SCHEMA_VERSION,
            email: std::mem::take(&mut self.email),
            password: std::mem::take(&mut self.password),
            email_code: std::mem::take(&mut self.email_code),
        })
    }
}

impl fmt::Debug for ResetPasswordCommandRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ResetPasswordCommandRequest")
            .field("schema_version", &self.schema_version)
            .field("email_bytes", &self.email.len())
            .field("password_bytes", &self.password.len())
            .field("email_code_bytes", &self.email_code.len())
            .finish()
    }
}

fn validate_schema_version(schema_version: u16) -> Result<(), CommandError> {
    if schema_version != DOMAIN_SCHEMA_VERSION {
        return Err(CommandError::from_code(ErrorCode::Validation));
    }
    Ok(())
}

fn valid_email_verification_code(value: &str) -> bool {
    value.len() == 6 && value.bytes().all(|byte| byte.is_ascii_digit())
}

fn valid_order_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

fn valid_ticket_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 20
        && !value.starts_with('0')
        && value.bytes().all(|byte| byte.is_ascii_digit())
}

fn valid_ticket_subject(value: &str) -> bool {
    !value.is_empty() && value.len() <= 128 && !value.chars().any(char::is_control)
}

fn valid_ticket_message(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 4 * 1024
        && !value
            .chars()
            .any(|character| character.is_control() && !matches!(character, '\n' | '\r' | '\t'))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeInfoRequest {
    pub schema_version: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DataPlaneEventSnapshotRequest {
    pub schema_version: u16,
}

impl DataPlaneEventSnapshotRequest {
    pub const fn current() -> Self {
        Self {
            schema_version: DOMAIN_SCHEMA_VERSION,
        }
    }

    pub fn validate(self) -> Result<Self, CommandError> {
        validate_schema_version(self.schema_version)?;
        Ok(self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DataPlaneControlAction {
    Status,
    Start,
    Stop,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DataPlaneControlRequest {
    pub schema_version: u16,
    pub action: DataPlaneControlAction,
}

impl DataPlaneControlRequest {
    pub const fn current(action: DataPlaneControlAction) -> Self {
        Self {
            schema_version: DOMAIN_SCHEMA_VERSION,
            action,
        }
    }

    pub fn validate(self) -> Result<Self, CommandError> {
        validate_schema_version(self.schema_version)?;
        Ok(self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DataPlaneControlResponse {
    pub schema_version: u16,
    pub control_plane: ControlPlaneState,
    pub data_plane: DataPlaneState,
    pub can_start: bool,
    pub can_stop: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConnectionModeRequest {
    pub schema_version: u16,
}

impl ConnectionModeRequest {
    pub const fn current() -> Self {
        Self {
            schema_version: DOMAIN_SCHEMA_VERSION,
        }
    }

    pub fn validate(self) -> Result<Self, CommandError> {
        validate_schema_version(self.schema_version)?;
        Ok(self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SetConnectionModeRequest {
    pub schema_version: u16,
    pub mode: ConnectionMode,
}

impl SetConnectionModeRequest {
    pub const fn current(mode: ConnectionMode) -> Self {
        Self {
            schema_version: DOMAIN_SCHEMA_VERSION,
            mode,
        }
    }

    pub fn validate(self) -> Result<Self, CommandError> {
        validate_schema_version(self.schema_version)?;
        Ok(self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionModeResponse {
    pub schema_version: u16,
    pub mode: ConnectionMode,
}

impl ConnectionModeResponse {
    pub const fn new(mode: ConnectionMode) -> Self {
        Self {
            schema_version: DOMAIN_SCHEMA_VERSION,
            mode,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RoutingModeRequest {
    pub schema_version: u16,
}

impl RoutingModeRequest {
    pub const fn current() -> Self {
        Self {
            schema_version: DOMAIN_SCHEMA_VERSION,
        }
    }

    pub fn validate(self) -> Result<Self, CommandError> {
        validate_schema_version(self.schema_version)?;
        Ok(self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SetRoutingModeRequest {
    pub schema_version: u16,
    pub mode: RoutingMode,
}

impl SetRoutingModeRequest {
    pub const fn current(mode: RoutingMode) -> Self {
        Self {
            schema_version: DOMAIN_SCHEMA_VERSION,
            mode,
        }
    }

    pub fn validate(self) -> Result<Self, CommandError> {
        validate_schema_version(self.schema_version)?;
        Ok(self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoutingModeResponse {
    pub schema_version: u16,
    pub mode: RoutingMode,
}

impl RoutingModeResponse {
    pub const fn new(mode: RoutingMode) -> Self {
        Self {
            schema_version: DOMAIN_SCHEMA_VERSION,
            mode,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LaunchOnStartupRequest {
    pub schema_version: u16,
}

impl LaunchOnStartupRequest {
    pub const fn current() -> Self {
        Self {
            schema_version: DOMAIN_SCHEMA_VERSION,
        }
    }

    pub fn validate(self) -> Result<Self, CommandError> {
        validate_schema_version(self.schema_version)?;
        Ok(self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SetLaunchOnStartupRequest {
    pub schema_version: u16,
    pub enabled: bool,
}

impl SetLaunchOnStartupRequest {
    pub const fn current(enabled: bool) -> Self {
        Self {
            schema_version: DOMAIN_SCHEMA_VERSION,
            enabled,
        }
    }

    pub fn validate(self) -> Result<Self, CommandError> {
        validate_schema_version(self.schema_version)?;
        Ok(self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchOnStartupResponse {
    pub schema_version: u16,
    pub enabled: bool,
}

impl LaunchOnStartupResponse {
    pub const fn new(enabled: bool) -> Self {
        Self {
            schema_version: DOMAIN_SCHEMA_VERSION,
            enabled,
        }
    }
}

impl DataPlaneControlResponse {
    pub const fn new(
        control_plane: ControlPlaneState,
        data_plane: DataPlaneState,
        can_start: bool,
        can_stop: bool,
    ) -> Self {
        Self {
            schema_version: DOMAIN_SCHEMA_VERSION,
            control_plane,
            data_plane,
            can_start,
            can_stop,
        }
    }
}

impl RuntimeInfoRequest {
    pub const fn current() -> Self {
        Self {
            schema_version: DOMAIN_SCHEMA_VERSION,
        }
    }

    pub fn validate(self) -> Result<Self, CommandError> {
        validate_schema_version(self.schema_version)?;
        Ok(self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeInfoResponse {
    pub schema_version: u16,
    pub product_name: String,
    pub product_version: String,
}

impl RuntimeInfoResponse {
    pub fn new(product_version: impl Into<String>) -> Self {
        Self {
            schema_version: DOMAIN_SCHEMA_VERSION,
            product_name: "Orange".to_owned(),
            product_version: product_version.into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PlaneStateRequest {
    pub schema_version: u16,
}

impl PlaneStateRequest {
    pub const fn current() -> Self {
        Self {
            schema_version: DOMAIN_SCHEMA_VERSION,
        }
    }

    pub fn validate(self) -> Result<Self, CommandError> {
        validate_schema_version(self.schema_version)?;
        Ok(self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaneStateResponse {
    pub schema_version: u16,
    pub control_plane: ControlPlaneState,
    pub data_plane: DataPlaneState,
}

impl PlaneStateResponse {
    pub const fn new(control_plane: ControlPlaneState, data_plane: DataPlaneState) -> Self {
        Self {
            schema_version: DOMAIN_SCHEMA_VERSION,
            control_plane,
            data_plane,
        }
    }
}

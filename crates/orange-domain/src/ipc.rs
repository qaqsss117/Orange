use std::fmt;

use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::{
    BUSINESS_API_SCHEMA_VERSION, CommandError, ConnectionMode, ControlPlaneState,
    CreateOrderRequest, CreatePaymentRequest, CreateTicketRequest, DOMAIN_SCHEMA_VERSION,
    DataPlaneState, ErrorCode, LoginRequest, RegisterRequest, ReplyTicketRequest,
    SubscriptionPublicResponse,
};

pub const GET_PLANE_STATE_COMMAND: &str = "get_plane_state";
pub const GET_RUNTIME_INFO_COMMAND: &str = "get_runtime_info";
pub const GET_DATA_PLANE_EVENT_SNAPSHOT_COMMAND: &str = "get_data_plane_event_snapshot";
pub const CONTROL_DATA_PLANE_COMMAND: &str = "control_data_plane";
pub const GET_CONNECTION_MODE_COMMAND: &str = "get_connection_mode";
pub const SET_CONNECTION_MODE_COMMAND: &str = "set_connection_mode";
pub const INITIALIZE_BUSINESS_COMMAND: &str = "initialize_business";
pub const LOGIN_COMMAND: &str = "login";
pub const REGISTER_COMMAND: &str = "register";
pub const GET_AUTH_SESSION_COMMAND: &str = "get_auth_session";
pub const LOGOUT_COMMAND: &str = "logout";
pub const REFRESH_ACCOUNT_COMMAND: &str = "refresh_account";
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
pub const REFRESH_SUBSCRIPTION_COMMAND: &str = "refresh_subscription";
pub const GET_SUBSCRIPTION_SNAPSHOT_COMMAND: &str = "get_subscription_snapshot";
pub const GET_NODE_CATALOG_COMMAND: &str = "get_node_catalog";
pub const SELECT_NODE_COMMAND: &str = "select_node";
pub const TEST_NODE_DELAYS_COMMAND: &str = "test_node_delays";
pub const BASE_COMMANDS: &[&str] = &[GET_PLANE_STATE_COMMAND, GET_RUNTIME_INFO_COMMAND];
pub const DESKTOP_OBSERVABILITY_COMMANDS: &[&str] = &[GET_DATA_PLANE_EVENT_SNAPSHOT_COMMAND];
pub const DESKTOP_DATA_PLANE_COMMANDS: &[&str] = &[
    CONTROL_DATA_PLANE_COMMAND,
    GET_CONNECTION_MODE_COMMAND,
    SET_CONNECTION_MODE_COMMAND,
    GET_NODE_CATALOG_COMMAND,
    SELECT_NODE_COMMAND,
    TEST_NODE_DELAYS_COMMAND,
];
pub const DESKTOP_BUSINESS_COMMANDS: &[&str] = &[
    INITIALIZE_BUSINESS_COMMAND,
    LOGIN_COMMAND,
    REGISTER_COMMAND,
    GET_AUTH_SESSION_COMMAND,
    LOGOUT_COMMAND,
    REFRESH_ACCOUNT_COMMAND,
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
    INITIALIZE_BUSINESS_COMMAND,
    LOGIN_COMMAND,
    REGISTER_COMMAND,
    GET_AUTH_SESSION_COMMAND,
    LOGOUT_COMMAND,
    REFRESH_ACCOUNT_COMMAND,
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
    REFRESH_SUBSCRIPTION_COMMAND,
    GET_SUBSCRIPTION_SNAPSHOT_COMMAND,
    GET_NODE_CATALOG_COMMAND,
    SELECT_NODE_COMMAND,
    TEST_NODE_DELAYS_COMMAND,
];

pub fn is_registered_command(command: &str) -> bool {
    REGISTERED_COMMANDS.contains(&command)
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
        Ok(CreateOrderRequest {
            schema_version: BUSINESS_API_SCHEMA_VERSION,
            plan_id: self.plan_id,
        })
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PublicNode {
    pub id: String,
    pub protocol: PublicNodeProtocol,
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
    pub groups: Vec<PublicNodeGroup>,
}

impl NodeCatalogResponse {
    pub const fn new(revision: Option<u64>, groups: Vec<PublicNodeGroup>) -> Self {
        Self {
            schema_version: DOMAIN_SCHEMA_VERSION,
            revision,
            groups,
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
}

impl SelectNodeResponse {
    pub fn new(selector_id: impl Into<String>, node_id: impl Into<String>) -> Self {
        Self {
            schema_version: DOMAIN_SCHEMA_VERSION,
            selector_id: selector_id.into(),
            node_id: node_id.into(),
        }
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
pub struct RegisterCommandRequest {
    #[zeroize(skip)]
    pub schema_version: u16,
    pub email: String,
    pub password: String,
    pub invite_code: Option<String>,
}

impl RegisterCommandRequest {
    pub fn validate(mut self) -> Result<RegisterRequest, CommandError> {
        validate_schema_version(self.schema_version)?;
        Ok(RegisterRequest {
            schema_version: BUSINESS_API_SCHEMA_VERSION,
            email: std::mem::take(&mut self.email),
            password: std::mem::take(&mut self.password),
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
            .field("has_invite_code", &self.invite_code.is_some())
            .finish()
    }
}

fn validate_schema_version(schema_version: u16) -> Result<(), CommandError> {
    if schema_version != DOMAIN_SCHEMA_VERSION {
        return Err(CommandError::from_code(ErrorCode::Validation));
    }
    Ok(())
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

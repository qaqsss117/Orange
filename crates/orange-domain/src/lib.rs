#![forbid(unsafe_code)]

mod business_api;
mod error;
mod ipc;
mod state;

pub use business_api::{
    AccountResponse, AccountStatus, AuthPublicResponse, AuthSessionResponse, AuthSessionStatus,
    AuthWireResponse, BUSINESS_API_SCHEMA_VERSION, BusinessInitializationResponse, ConfigResponse,
    ConfigWireResponse, CreateOrderRequest, CreateOrderResponse, CreatePaymentRequest,
    CreateTicketRequest, CredentialBundle, CurrencyCode, InviteResponse, LoginRequest, Money,
    Order, OrderDetail, OrderDetailResponse, OrderResponse, OrderStatus, OrderSummary,
    OrdersResponse, PaymentPublicResponse, PaymentStatus, PaymentWireResponse, Plan, PlansResponse,
    RegisterRequest, SafeInteger, SubscriptionPublicResponse, SubscriptionStatus,
    SubscriptionWireResponse, Ticket, TicketStatus, TicketsResponse, UnixMillis, UpdateResponse,
    UserProfile,
};
pub use error::{CommandError, ErrorCode};
pub use ipc::{
    AccountRefreshRequest, AuthSessionRequest, BASE_COMMANDS, CONTROL_DATA_PLANE_COMMAND,
    CREATE_ORDER_COMMAND, ConnectionModeRequest, ConnectionModeResponse, CreateOrderCommandRequest,
    DESKTOP_BUSINESS_COMMANDS, DESKTOP_DATA_PLANE_COMMANDS, DESKTOP_OBSERVABILITY_COMMANDS,
    DataPlaneControlAction, DataPlaneControlRequest, DataPlaneControlResponse,
    DataPlaneEventSnapshotRequest, FETCH_ORDER_DETAIL_COMMAND, FETCH_ORDERS_COMMAND,
    FETCH_PLANS_COMMAND, GET_AUTH_SESSION_COMMAND, GET_CONNECTION_MODE_COMMAND,
    GET_DATA_PLANE_EVENT_SNAPSHOT_COMMAND, GET_NODE_CATALOG_COMMAND, GET_PLANE_STATE_COMMAND,
    GET_RUNTIME_INFO_COMMAND, GET_SUBSCRIPTION_SNAPSHOT_COMMAND, INITIALIZE_BUSINESS_COMMAND,
    InitializeBusinessRequest, LOGIN_COMMAND, LOGOUT_COMMAND, LoginCommandRequest, LogoutRequest,
    NodeCatalogRequest, NodeCatalogResponse, NodeDelayTestRequest, NodeDelayTestResponse,
    OrderDetailCommandRequest, OrdersRequest, PlaneStateRequest, PlaneStateResponse, PlansRequest,
    PublicNode, PublicNodeDelay, PublicNodeDelayResult, PublicNodeGroup, PublicNodeProtocol,
    REFRESH_ACCOUNT_COMMAND, REFRESH_SUBSCRIPTION_COMMAND, REGISTER_COMMAND, REGISTERED_COMMANDS,
    RegisterCommandRequest, RuntimeInfoRequest, RuntimeInfoResponse, SELECT_NODE_COMMAND,
    SET_CONNECTION_MODE_COMMAND, SelectNodeRequest, SelectNodeResponse, SetConnectionModeRequest,
    SubscriptionRefreshRequest, SubscriptionSnapshotRequest, SubscriptionSnapshotResponse,
    TEST_NODE_DELAYS_COMMAND, is_registered_command,
};
pub use state::{
    ConnectionMode, ControlPlaneState, ControlPlaneStateMachine, DataPlaneState,
    DataPlaneStateMachine, StateTransitionError, TransitionOutcome,
};

pub const DOMAIN_SCHEMA_VERSION: u16 = 2;

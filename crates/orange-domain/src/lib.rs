#![forbid(unsafe_code)]

mod business_api;
mod error;
mod ipc;
mod state;

pub use business_api::{
    AccountResponse, AccountStatus, AuthPublicResponse, AuthSessionResponse, AuthSessionStatus,
    AuthWireResponse, BUSINESS_API_SCHEMA_VERSION, BusinessInitializationResponse,
    CancelOrderResponse, ConfigResponse, ConfigWireResponse, CreateOrderRequest,
    CreateOrderResponse, CreatePaymentRequest, CreateTicketRequest, CredentialBundle, CurrencyCode,
    EmailVerificationResponse, InvitationCenterResponse, InvitationCode, InvitationCodeStatus,
    InvitationStats, LoginRequest, Money, Notice, NoticesResponse, Order, OrderDetail,
    OrderDetailResponse, OrderResponse, OrderStatus, OrderSummary, OrdersResponse,
    PasswordResetResponse, PaymentMethod, PaymentMethodsResponse, PaymentPublicResponse,
    PaymentStatus, PaymentWireResponse, Plan, PlansResponse, RegisterRequest, ReplyTicketRequest,
    ResetPasswordRequest, SafeInteger, SendEmailVerificationRequest, SubscriptionLinkResponse,
    SubscriptionPublicResponse,
    SubscriptionStatus, SubscriptionWireResponse, Ticket, TicketDetail, TicketDetailResponse,
    TicketMessage, TicketStatus, TicketsResponse, UnixMillis, UpdateResponse, UserProfile,
};
pub use error::{CommandError, ErrorCode};
pub use ipc::{
    AccountRefreshRequest, AuthSessionRequest, BASE_COMMANDS, CANCEL_ORDER_COMMAND,
    CHECKOUT_ORDER_COMMAND, CLOSE_TICKET_COMMAND, CONTROL_DATA_PLANE_COMMAND, CREATE_ORDER_COMMAND,
    CREATE_TICKET_COMMAND, CancelOrderCommandRequest, CheckoutOrderCommandRequest,
    CloseTicketCommandRequest, ConnectionModeRequest, ConnectionModeResponse,
    CreateOrderCommandRequest, CreateTicketCommandRequest, DESKTOP_BUSINESS_COMMANDS,
    DESKTOP_DATA_PLANE_COMMANDS, DESKTOP_OBSERVABILITY_COMMANDS, DESKTOP_SETTINGS_COMMANDS,
    DataPlaneControlAction, DataPlaneControlRequest, DataPlaneControlResponse,
    DataPlaneEventSnapshotRequest, FETCH_INVITATION_CENTER_COMMAND, FETCH_NOTICES_COMMAND,
    FETCH_ORDER_DETAIL_COMMAND, FETCH_ORDERS_COMMAND, FETCH_PAYMENT_METHODS_COMMAND,
    FETCH_PLANS_COMMAND, FETCH_TICKET_DETAIL_COMMAND, FETCH_TICKETS_COMMAND,
    GENERATE_INVITATION_CODE_COMMAND, GET_AUTH_SESSION_COMMAND, GET_CONNECTION_MODE_COMMAND,
    GET_DATA_PLANE_EVENT_SNAPSHOT_COMMAND, GET_LAUNCH_ON_STARTUP_COMMAND, GET_NODE_CATALOG_COMMAND,
    GET_PLANE_STATE_COMMAND, GET_ROUTING_MODE_COMMAND, GET_RUNTIME_INFO_COMMAND,
    GET_SUBSCRIPTION_SNAPSHOT_COMMAND, INITIALIZE_BUSINESS_COMMAND, InitializeBusinessRequest,
    InvitationCenterRequest, LOGIN_COMMAND, LOGOUT_COMMAND, LaunchOnStartupRequest,
    LaunchOnStartupResponse, LegalDocument, LoginCommandRequest, LogoutRequest, NetworkTool,
    NodeCatalogRequest, NodeCatalogResponse, NodeDelayTestRequest, NodeDelayTestResponse,
    NoticesRequest, OPEN_LEGAL_DOCUMENT_COMMAND, OPEN_NETWORK_TOOL_COMMAND,
    OPEN_SERVICE_PORTAL_COMMAND, OpenLegalDocumentRequest, OpenLegalDocumentResponse,
    OpenNetworkToolRequest, OpenNetworkToolResponse, OpenServicePortalRequest,
    OpenServicePortalResponse, OrderDetailCommandRequest, OrdersRequest, PaymentMethodsRequest,
    PlaneStateRequest, PlaneStateResponse, PlansRequest, PublicNode, PublicNodeDelay,
    PublicNodeDelayResult, PublicNodeGroup, PublicNodeProtocol, REFRESH_ACCOUNT_COMMAND,
    REFRESH_SUBSCRIPTION_COMMAND, REGISTER_COMMAND, REGISTERED_COMMANDS, REPLY_TICKET_COMMAND,
    RESET_PASSWORD_COMMAND, RegisterCommandRequest, ReplyTicketCommandRequest,
    ResetPasswordCommandRequest, RoutingModeRequest, RoutingModeResponse, RuntimeInfoRequest,
    RuntimeInfoResponse, SELECT_NODE_COMMAND, SEND_EMAIL_VERIFICATION_COMMAND,
    SET_CONNECTION_MODE_COMMAND, SET_LAUNCH_ON_STARTUP_COMMAND, SET_ROUTING_MODE_COMMAND,
    SelectNodeRequest, SelectNodeResponse, SendEmailVerificationCommandRequest,
    SetConnectionModeRequest, SetLaunchOnStartupRequest, SetRoutingModeRequest,
    SubscriptionRefreshRequest, SubscriptionSnapshotRequest, SubscriptionSnapshotResponse,
    TEST_NODE_DELAYS_COMMAND, TicketDetailCommandRequest, TicketsRequest, is_registered_command,
};
pub use state::{
    ConnectionMode, ControlPlaneState, ControlPlaneStateMachine, DataPlaneState,
    DataPlaneStateMachine, RoutingMode, StateTransitionError, TransitionOutcome,
};

pub const DOMAIN_SCHEMA_VERSION: u16 = 2;

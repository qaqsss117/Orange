import { invoke } from "@tauri-apps/api/core";
import {
  parseDataPlaneEventSnapshot,
  type DataPlaneEventSnapshot,
} from "./events";
import {
  CONTROL_PLANE_STATES,
  DATA_PLANE_STATES,
  type ControlPlaneState,
  type DataPlaneState,
} from "./planeStates";
import {
  type AccountResponse,
  type AuthPublicResponse,
  type AuthSessionResponse,
  type BusinessInitializationResponse,
  type CancelOrderResponse,
  type CreateOrderResponse,
  type EmailVerificationResponse,
  type InvitationCenterResponse,
  type NoticesResponse,
  type OrderDetailResponse,
  type OrdersResponse,
  type PasswordResetResponse,
  type PaymentMethodsResponse,
  type PaymentPublicResponse,
  type PlansResponse,
  type ActiveSessionsResponse,
  type KnowledgeDetailResponse,
  type KnowledgeListResponse,
  type CommissionConfigResponse,
  type CommissionOperationResponse,
  type GiftCardCheckResponse,
  type GiftCardHistoryResponse,
  type GiftCardRedeemResponse,
  type SubscriptionLinkResponse,
  type SubscriptionPublicResponse,
  type TicketsResponse,
  type TicketDetailResponse,
  parseAccountResponse,
  parseAuthPublicResponse,
  parseAuthSessionResponse,
  parseBusinessInitializationResponse,
  parseCancelOrderResponse,
  parseCreateOrderResponse,
  parseEmailVerificationResponse,
  parseInvitationCenterResponse,
  parseNoticesResponse,
  parseOrderDetailResponse,
  parseOrdersResponse,
  parsePasswordResetResponse,
  parsePaymentMethodsResponse,
  parsePaymentResponse,
  parsePlansResponse,
  parseActiveSessionsResponse,
  parseKnowledgeDetailResponse,
  parseKnowledgeListResponse,
  parseCommissionConfigResponse,
  parseCommissionOperationResponse,
  parseGiftCardCheckResponse,
  parseGiftCardHistoryResponse,
  parseGiftCardRedeemResponse,
  parseSubscriptionLinkResponse,
  parseSubscriptionResponse,
  parseTicketsResponse,
  parseTicketDetailResponse,
} from "./businessApi";

export const IPC_SCHEMA_VERSION = 2 as const;

export const DATA_PLANE_CONTROL_ACTIONS = ["status", "start", "stop"] as const;
export type DataPlaneControlAction =
  (typeof DATA_PLANE_CONTROL_ACTIONS)[number];

export const CONNECTION_MODES = ["system_proxy", "tun"] as const;
export type ConnectionMode = (typeof CONNECTION_MODES)[number];

export const ROUTING_MODES = ["smart", "global", "direct"] as const;
export type RoutingMode = (typeof ROUTING_MODES)[number];

export const NETWORK_TOOLS = ["ip_lookup", "speed_test"] as const;
export type NetworkTool = (typeof NETWORK_TOOLS)[number];

export const LEGAL_DOCUMENTS = ["terms_of_service", "privacy_policy"] as const;
export type LegalDocument = (typeof LEGAL_DOCUMENTS)[number];

export const COMMANDS = {
  getPlaneState: "get_plane_state",
  getRuntimeInfo: "get_runtime_info",
  getDataPlaneEventSnapshot: "get_data_plane_event_snapshot",
  controlDataPlane: "control_data_plane",
  getConnectionMode: "get_connection_mode",
  setConnectionMode: "set_connection_mode",
  getRoutingMode: "get_routing_mode",
  setRoutingMode: "set_routing_mode",
  getLaunchOnStartup: "get_launch_on_startup",
  setLaunchOnStartup: "set_launch_on_startup",
  openNetworkTool: "open_network_tool",
  openLegalDocument: "open_legal_document",
  initializeBusiness: "initialize_business",
  openServicePortal: "open_service_portal",
  getServicePortalUrl: "get_service_portal_url",
  openTelegramBot: "open_telegram_bot",
  login: "login",
  sendEmailVerification: "send_email_verification",
  resetPassword: "reset_password",
  register: "register",
  getAuthSession: "get_auth_session",
  logout: "logout",
  refreshAccount: "refresh_account",
  fetchNotices: "fetch_notices",
  fetchPlans: "fetch_plans",
  fetchOrders: "fetch_orders",
  fetchOrderDetail: "fetch_order_detail",
  fetchPaymentMethods: "fetch_payment_methods",
  checkoutOrder: "checkout_order",
  cancelOrder: "cancel_order",
  createOrder: "create_order",
  fetchInvitationCenter: "fetch_invitation_center",
  generateInvitationCode: "generate_invitation_code",
  fetchTickets: "fetch_tickets",
  fetchTicketDetail: "fetch_ticket_detail",
  createTicket: "create_ticket",
  replyTicket: "reply_ticket",
  closeTicket: "close_ticket",
  refreshSubscription: "refresh_subscription",
  fetchSubscriptionLink: "fetch_subscription_link",
  resetSubscriptionLink: "reset_subscription_link",
  fetchKnowledgeList: "fetch_knowledge_list",
  fetchKnowledgeDetail: "fetch_knowledge_detail",
  fetchActiveSessions: "fetch_active_sessions",
  removeActiveSession: "remove_active_session",
  fetchCommissionConfig: "fetch_commission_config",
  withdrawCommission: "withdraw_commission",
  transferCommission: "transfer_commission",
  checkGiftCard: "check_gift_card",
  redeemGiftCard: "redeem_gift_card",
  fetchGiftCardHistory: "fetch_gift_card_history",
  getSubscriptionSnapshot: "get_subscription_snapshot",
  getNodeCatalog: "get_node_catalog",
  selectNode: "select_node",
  testNodeDelays: "test_node_delays",
} as const;

export {
  CONTROL_PLANE_STATES,
  DATA_PLANE_STATES,
  type ControlPlaneState,
  type DataPlaneState,
} from "./planeStates";

export const MAX_AUTH_EMAIL_BYTES = 254;
export const MIN_AUTH_PASSWORD_BYTES = 8;
export const MAX_AUTH_PASSWORD_BYTES = 128;
export const MAX_INVITE_CODE_BYTES = 64;
export const EMAIL_VERIFICATION_CODE_LENGTH = 6;

export const ERROR_CODES = [
  "validation",
  "permission",
  "network",
  "bootstrap",
  "subscription",
  "service",
  "timeout",
  "cancelled",
  "internal",
] as const;

export type ErrorCode = (typeof ERROR_CODES)[number];

export const ERROR_DEFINITIONS = {
  validation: { message: "请求参数无效。", retryable: false },
  permission: { message: "当前操作未获授权。", retryable: false },
  network: { message: "网络请求失败，请稍后重试。", retryable: true },
  bootstrap: { message: "安全连接初始化失败。", retryable: true },
  subscription: { message: "订阅数据不可用。", retryable: false },
  service: { message: "系统服务暂不可用。", retryable: true },
  timeout: { message: "操作超时，请重试。", retryable: true },
  cancelled: { message: "操作已取消。", retryable: false },
  internal: { message: "发生内部错误。", retryable: false },
} as const satisfies Record<ErrorCode, { message: string; retryable: boolean }>;

export interface CommandError {
  schemaVersion: typeof IPC_SCHEMA_VERSION;
  code: ErrorCode;
  message: string;
  retryable: boolean;
}

export interface RuntimeInfoRequest {
  schemaVersion: typeof IPC_SCHEMA_VERSION;
}

export interface PlaneStateRequest {
  schemaVersion: typeof IPC_SCHEMA_VERSION;
}

export interface PlaneStateResponse {
  schemaVersion: typeof IPC_SCHEMA_VERSION;
  controlPlane: ControlPlaneState;
  dataPlane: DataPlaneState;
}

export interface DataPlaneControlRequest {
  schemaVersion: typeof IPC_SCHEMA_VERSION;
  action: DataPlaneControlAction;
}

export interface DataPlaneControlResponse extends PlaneStateResponse {
  canStart: boolean;
  canStop: boolean;
}

export interface ConnectionModeResponse {
  schemaVersion: typeof IPC_SCHEMA_VERSION;
  mode: ConnectionMode;
}

export interface RoutingModeResponse {
  schemaVersion: typeof IPC_SCHEMA_VERSION;
  mode: RoutingMode;
}

export interface LaunchOnStartupResponse {
  schemaVersion: typeof IPC_SCHEMA_VERSION;
  enabled: boolean;
}

export interface OpenServicePortalResponse {
  schemaVersion: typeof IPC_SCHEMA_VERSION;
  opened: boolean;
}

export interface ServicePortalUrlResponse {
  schemaVersion: typeof IPC_SCHEMA_VERSION;
  url: string;
}

export interface OpenNetworkToolResponse {
  schemaVersion: typeof IPC_SCHEMA_VERSION;
  tool: NetworkTool;
}

export interface OpenLegalDocumentResponse {
  schemaVersion: typeof IPC_SCHEMA_VERSION;
  document: LegalDocument;
}

export interface SubscriptionSnapshotResponse {
  schemaVersion: typeof IPC_SCHEMA_VERSION;
  subscription: SubscriptionPublicResponse | null;
  localRevision: number | null;
}

export const PUBLIC_NODE_PROTOCOLS = [
  "shadowsocks",
  "trojan",
  "hysteria2",
  "vless",
] as const;
export type PublicNodeProtocol = (typeof PUBLIC_NODE_PROTOCOLS)[number];

export interface PublicNode {
  id: string;
  protocol: PublicNodeProtocol;
}

export interface PublicNodeGroup {
  id: string;
  selectedNodeId: string;
  nodes: PublicNode[];
}

export interface NodeCatalogResponse {
  schemaVersion: typeof IPC_SCHEMA_VERSION;
  revision: number | null;
  groups: PublicNodeGroup[];
}

export interface SelectNodeResponse {
  schemaVersion: typeof IPC_SCHEMA_VERSION;
  selectorId: string;
  nodeId: string;
}

export type PublicNodeDelay =
  | { status: "available"; delayMs: number }
  | { status: "timed_out" | "unavailable" };

export interface PublicNodeDelayResult {
  selectorId: string;
  nodeId: string;
  result: PublicNodeDelay;
}

export interface NodeDelayTestResponse {
  schemaVersion: typeof IPC_SCHEMA_VERSION;
  results: PublicNodeDelayResult[];
}

export interface RuntimeInfoResponse {
  schemaVersion: typeof IPC_SCHEMA_VERSION;
  productName: "Orange";
  productVersion: string;
}

export interface LoginFormInput {
  email: string;
  password: string;
}

export interface RegisterFormInput extends LoginFormInput {
  emailCode: string | null;
  inviteCode: string | null;
}

export interface SendEmailVerificationFormInput {
  email: string;
}

export interface ResetPasswordFormInput extends LoginFormInput {
  emailCode: string;
}

export interface LoginCommandRequest extends LoginFormInput {
  schemaVersion: typeof IPC_SCHEMA_VERSION;
}

export interface RegisterCommandRequest extends RegisterFormInput {
  schemaVersion: typeof IPC_SCHEMA_VERSION;
}

export interface SendEmailVerificationCommandRequest extends SendEmailVerificationFormInput {
  schemaVersion: typeof IPC_SCHEMA_VERSION;
}

export interface ResetPasswordCommandRequest extends ResetPasswordFormInput {
  schemaVersion: typeof IPC_SCHEMA_VERSION;
}

export interface AccountRefreshRequest {
  schemaVersion: typeof IPC_SCHEMA_VERSION;
}

export interface NoticesRequest {
  schemaVersion: typeof IPC_SCHEMA_VERSION;
}

export interface PlansRequest {
  schemaVersion: typeof IPC_SCHEMA_VERSION;
}

export interface OrdersRequest {
  schemaVersion: typeof IPC_SCHEMA_VERSION;
}

export interface OrderDetailCommandRequest {
  schemaVersion: typeof IPC_SCHEMA_VERSION;
  orderId: string;
}

export interface PaymentMethodsRequest {
  schemaVersion: typeof IPC_SCHEMA_VERSION;
}

export interface CheckoutOrderCommandRequest {
  schemaVersion: typeof IPC_SCHEMA_VERSION;
  orderId: string;
  paymentMethod: string;
}

export interface CancelOrderCommandRequest {
  schemaVersion: typeof IPC_SCHEMA_VERSION;
  orderId: string;
}

export interface CreateOrderCommandRequest {
  schemaVersion: typeof IPC_SCHEMA_VERSION;
  planId: string;
  couponCode?: string;
}

export interface InvitationCenterRequest {
  schemaVersion: typeof IPC_SCHEMA_VERSION;
}

export interface TicketsRequest {
  schemaVersion: typeof IPC_SCHEMA_VERSION;
}

export interface TicketDetailCommandRequest {
  schemaVersion: typeof IPC_SCHEMA_VERSION;
  ticketId: string;
}

export interface CreateTicketCommandRequest {
  schemaVersion: typeof IPC_SCHEMA_VERSION;
  subject: string;
  message: string;
}

export interface ReplyTicketCommandRequest {
  schemaVersion: typeof IPC_SCHEMA_VERSION;
  ticketId: string;
  message: string;
}

export interface CloseTicketCommandRequest {
  schemaVersion: typeof IPC_SCHEMA_VERSION;
  ticketId: string;
}

export interface SubscriptionRefreshRequest {
  schemaVersion: typeof IPC_SCHEMA_VERSION;
}

export interface LogoutRequest {
  schemaVersion: typeof IPC_SCHEMA_VERSION;
}

export type AuthFormField = "email" | "password" | "emailCode" | "inviteCode";

export class AuthFormError extends Error {
  readonly field: AuthFormField;

  constructor(field: AuthFormField, message: string) {
    super(message);
    this.name = "AuthFormError";
    this.field = field;
  }
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function hasOnlyKeys(
  value: Record<string, unknown>,
  allowedKeys: readonly string[],
): boolean {
  return Object.keys(value).every((key) => allowedKeys.includes(key));
}

function isErrorCode(value: unknown): value is ErrorCode {
  return (
    typeof value === "string" &&
    (ERROR_CODES as readonly string[]).includes(value)
  );
}

function isControlPlaneState(value: unknown): value is ControlPlaneState {
  return (
    typeof value === "string" &&
    (CONTROL_PLANE_STATES as readonly string[]).includes(value)
  );
}

function isDataPlaneState(value: unknown): value is DataPlaneState {
  return (
    typeof value === "string" &&
    (DATA_PLANE_STATES as readonly string[]).includes(value)
  );
}

function isDataPlaneControlAction(
  value: unknown,
): value is DataPlaneControlAction {
  return (
    typeof value === "string" &&
    (DATA_PLANE_CONTROL_ACTIONS as readonly string[]).includes(value)
  );
}

function isConnectionMode(value: unknown): value is ConnectionMode {
  return (
    typeof value === "string" &&
    (CONNECTION_MODES as readonly string[]).includes(value)
  );
}

function isRoutingMode(value: unknown): value is RoutingMode {
  return (
    typeof value === "string" &&
    (ROUTING_MODES as readonly string[]).includes(value)
  );
}

function isNetworkTool(value: unknown): value is NetworkTool {
  return (
    typeof value === "string" &&
    (NETWORK_TOOLS as readonly string[]).includes(value)
  );
}

function isLegalDocument(value: unknown): value is LegalDocument {
  return (
    typeof value === "string" &&
    (LEGAL_DOCUMENTS as readonly string[]).includes(value)
  );
}

function isPublicNodeProtocol(value: unknown): value is PublicNodeProtocol {
  return (
    typeof value === "string" &&
    (PUBLIC_NODE_PROTOCOLS as readonly string[]).includes(value)
  );
}

function parseSafeIpcInteger(value: unknown, nullable = false): number | null {
  if (nullable && value === null) {
    return null;
  }
  if (typeof value !== "number" || !Number.isSafeInteger(value) || value < 1) {
    throw new Error("IPC integer contract violation");
  }
  return value;
}

function parsePublicNodeId(value: unknown): string {
  if (
    typeof value !== "string" ||
    value.length === 0 ||
    value.length > 64 ||
    value.startsWith("orange-") ||
    !/^[A-Za-z0-9._-]+$/.test(value)
  ) {
    throw new Error("PublicNodeId contract violation");
  }
  return value;
}

function utf8Length(value: string): number {
  return new TextEncoder().encode(value).length;
}

function isAscii(value: string): boolean {
  return [...value].every((character) => character.charCodeAt(0) <= 127);
}

function hasControlCharacter(value: string): boolean {
  return [...value].some((character) => {
    const code = character.charCodeAt(0);
    return code <= 31 || code === 127;
  });
}

function hasUnsafeMultilineControl(value: string): boolean {
  return [...value].some((character) => {
    const code = character.charCodeAt(0);
    return (
      (code <= 31 &&
        character !== "\n" &&
        character !== "\r" &&
        character !== "\t") ||
      code === 127
    );
  });
}

function validateEmail(email: string): void {
  const parts = email.split("@");
  const local = parts[0] ?? "";
  const domain = parts[1] ?? "";
  const labels = domain.split(".");
  if (
    parts.length !== 2 ||
    utf8Length(email) > MAX_AUTH_EMAIL_BYTES ||
    email.length < 3 ||
    email.trim() !== email ||
    !isAscii(email) ||
    hasControlCharacter(email) ||
    local.length === 0 ||
    local.length > 64 ||
    local.startsWith(".") ||
    local.endsWith(".") ||
    local.includes("..") ||
    !/^[A-Za-z0-9.!#$%&'*+/=?^_`{|}~-]+$/.test(local) ||
    labels.length < 2 ||
    labels.some(
      (label) =>
        label.length === 0 ||
        label.startsWith("-") ||
        label.endsWith("-") ||
        !/^[A-Za-z0-9-]+$/.test(label),
    )
  ) {
    throw new AuthFormError("email", "邮箱格式无效。");
  }
}

function validatePassword(password: string): void {
  const bytes = utf8Length(password);
  if (
    bytes < MIN_AUTH_PASSWORD_BYTES ||
    bytes > MAX_AUTH_PASSWORD_BYTES ||
    hasControlCharacter(password)
  ) {
    throw new AuthFormError("password", "密码格式无效。");
  }
}

function validateInviteCode(inviteCode: string | null): void {
  if (
    inviteCode !== null &&
    (inviteCode.length === 0 ||
      utf8Length(inviteCode) > MAX_INVITE_CODE_BYTES ||
      !/^[A-Za-z0-9_-]+$/.test(inviteCode))
  ) {
    throw new AuthFormError("inviteCode", "邀请码格式无效。");
  }
}

function validateEmailVerificationCode(emailCode: string | null): void {
  if (
    emailCode !== null &&
    (emailCode.length !== EMAIL_VERIFICATION_CODE_LENGTH ||
      !/^[0-9]+$/.test(emailCode))
  ) {
    throw new AuthFormError("emailCode", "邮箱验证码格式无效。");
  }
}

export function parseLoginCommandRequest(
  value: LoginFormInput,
): LoginCommandRequest {
  validateEmail(value.email);
  validatePassword(value.password);
  return {
    schemaVersion: IPC_SCHEMA_VERSION,
    email: value.email,
    password: value.password,
  };
}

export function parseRegisterCommandRequest(
  value: RegisterFormInput,
): RegisterCommandRequest {
  validateEmail(value.email);
  validatePassword(value.password);
  validateEmailVerificationCode(value.emailCode);
  validateInviteCode(value.inviteCode);
  return {
    schemaVersion: IPC_SCHEMA_VERSION,
    email: value.email,
    password: value.password,
    emailCode: value.emailCode,
    inviteCode: value.inviteCode,
  };
}

export function parseSendEmailVerificationCommandRequest(
  value: SendEmailVerificationFormInput,
): SendEmailVerificationCommandRequest {
  validateEmail(value.email);
  return {
    schemaVersion: IPC_SCHEMA_VERSION,
    email: value.email,
  };
}

export function parseResetPasswordCommandRequest(
  value: ResetPasswordFormInput,
): ResetPasswordCommandRequest {
  validateEmail(value.email);
  validatePassword(value.password);
  validateEmailVerificationCode(value.emailCode);
  return {
    schemaVersion: IPC_SCHEMA_VERSION,
    email: value.email,
    password: value.password,
    emailCode: value.emailCode,
  };
}

export function parseAccountRefreshRequest(
  value: unknown,
): AccountRefreshRequest {
  if (
    !isRecord(value) ||
    !hasOnlyKeys(value, ["schemaVersion"]) ||
    value.schemaVersion !== IPC_SCHEMA_VERSION
  ) {
    throw new Error("AccountRefreshRequest contract violation");
  }
  return { schemaVersion: IPC_SCHEMA_VERSION };
}

export function parseNoticesRequest(value: unknown): NoticesRequest {
  if (
    !isRecord(value) ||
    !hasOnlyKeys(value, ["schemaVersion"]) ||
    value.schemaVersion !== IPC_SCHEMA_VERSION
  ) {
    throw new Error("NoticesRequest contract violation");
  }
  return { schemaVersion: IPC_SCHEMA_VERSION };
}

export function parsePlansRequest(value: unknown): PlansRequest {
  if (
    !isRecord(value) ||
    !hasOnlyKeys(value, ["schemaVersion"]) ||
    value.schemaVersion !== IPC_SCHEMA_VERSION
  ) {
    throw new Error("PlansRequest contract violation");
  }
  return { schemaVersion: IPC_SCHEMA_VERSION };
}

export function parseOrdersRequest(value: unknown): OrdersRequest {
  if (
    !isRecord(value) ||
    !hasOnlyKeys(value, ["schemaVersion"]) ||
    value.schemaVersion !== IPC_SCHEMA_VERSION
  ) {
    throw new Error("OrdersRequest contract violation");
  }
  return { schemaVersion: IPC_SCHEMA_VERSION };
}

export function parseOrderDetailCommandRequest(
  value: unknown,
): OrderDetailCommandRequest {
  if (
    !isRecord(value) ||
    !hasOnlyKeys(value, ["schemaVersion", "orderId"]) ||
    value.schemaVersion !== IPC_SCHEMA_VERSION ||
    typeof value.orderId !== "string" ||
    value.orderId.length > 128 ||
    !/^[A-Za-z0-9._-]+$/.test(value.orderId)
  ) {
    throw new Error("OrderDetailCommandRequest contract violation");
  }
  return {
    schemaVersion: IPC_SCHEMA_VERSION,
    orderId: value.orderId,
  };
}

export function parsePaymentMethodsRequest(
  value: unknown,
): PaymentMethodsRequest {
  if (
    !isRecord(value) ||
    !hasOnlyKeys(value, ["schemaVersion"]) ||
    value.schemaVersion !== IPC_SCHEMA_VERSION
  ) {
    throw new Error("PaymentMethodsRequest contract violation");
  }
  return { schemaVersion: IPC_SCHEMA_VERSION };
}

export function parseCheckoutOrderCommandRequest(
  value: unknown,
): CheckoutOrderCommandRequest {
  if (
    !isRecord(value) ||
    !hasOnlyKeys(value, ["schemaVersion", "orderId", "paymentMethod"]) ||
    value.schemaVersion !== IPC_SCHEMA_VERSION ||
    typeof value.orderId !== "string" ||
    value.orderId.length > 128 ||
    !/^[A-Za-z0-9._-]+$/.test(value.orderId) ||
    typeof value.paymentMethod !== "string" ||
    value.paymentMethod.length > 20 ||
    !/^[1-9][0-9]*$/.test(value.paymentMethod)
  ) {
    throw new Error("CheckoutOrderCommandRequest contract violation");
  }
  return {
    schemaVersion: IPC_SCHEMA_VERSION,
    orderId: value.orderId,
    paymentMethod: value.paymentMethod,
  };
}

export function parseCancelOrderCommandRequest(
  value: unknown,
): CancelOrderCommandRequest {
  if (
    !isRecord(value) ||
    !hasOnlyKeys(value, ["schemaVersion", "orderId"]) ||
    value.schemaVersion !== IPC_SCHEMA_VERSION ||
    typeof value.orderId !== "string" ||
    value.orderId.length > 128 ||
    !/^[A-Za-z0-9._-]+$/.test(value.orderId)
  ) {
    throw new Error("CancelOrderCommandRequest contract violation");
  }
  return {
    schemaVersion: IPC_SCHEMA_VERSION,
    orderId: value.orderId,
  };
}

export function parseCreateOrderCommandRequest(
  value: unknown,
): CreateOrderCommandRequest {
  if (
    !isRecord(value) ||
    !hasOnlyKeys(value, ["schemaVersion", "planId", "couponCode"]) ||
    value.schemaVersion !== IPC_SCHEMA_VERSION ||
    typeof value.planId !== "string" ||
    value.planId.length > 64 ||
    !/^[1-9][0-9]*:(month_price|quarter_price|half_year_price|year_price|two_year_price|three_year_price|onetime_price)$/.test(
      value.planId,
    ) ||
    (value.couponCode !== undefined &&
      (typeof value.couponCode !== "string" ||
        value.couponCode.length > 64 ||
        !/^[ -~]*$/.test(value.couponCode)))
  ) {
    throw new Error("CreateOrderCommandRequest contract violation");
  }
  const couponCode = value.couponCode?.trim();
  return {
    schemaVersion: IPC_SCHEMA_VERSION,
    planId: value.planId,
    ...(couponCode ? { couponCode } : {}),
  };
}

export function parseInvitationCenterRequest(
  value: unknown,
): InvitationCenterRequest {
  if (
    !isRecord(value) ||
    !hasOnlyKeys(value, ["schemaVersion"]) ||
    value.schemaVersion !== IPC_SCHEMA_VERSION
  ) {
    throw new Error("InvitationCenterRequest contract violation");
  }
  return { schemaVersion: IPC_SCHEMA_VERSION };
}

export function parseTicketsRequest(value: unknown): TicketsRequest {
  if (
    !isRecord(value) ||
    !hasOnlyKeys(value, ["schemaVersion"]) ||
    value.schemaVersion !== IPC_SCHEMA_VERSION
  ) {
    throw new Error("TicketsRequest contract violation");
  }
  return { schemaVersion: IPC_SCHEMA_VERSION };
}

export function parseTicketDetailCommandRequest(
  value: unknown,
): TicketDetailCommandRequest {
  if (
    !isRecord(value) ||
    !hasOnlyKeys(value, ["schemaVersion", "ticketId"]) ||
    value.schemaVersion !== IPC_SCHEMA_VERSION ||
    typeof value.ticketId !== "string" ||
    !/^[1-9][0-9]{0,19}$/.test(value.ticketId)
  ) {
    throw new Error("TicketDetailCommandRequest contract violation");
  }
  return { schemaVersion: IPC_SCHEMA_VERSION, ticketId: value.ticketId };
}

export function parseCreateTicketCommandRequest(
  value: unknown,
): CreateTicketCommandRequest {
  if (
    !isRecord(value) ||
    !hasOnlyKeys(value, ["schemaVersion", "subject", "message"]) ||
    value.schemaVersion !== IPC_SCHEMA_VERSION ||
    typeof value.subject !== "string" ||
    typeof value.message !== "string"
  ) {
    throw new Error("CreateTicketCommandRequest contract violation");
  }
  const subject = value.subject.trim();
  const message = value.message.trim();
  if (
    subject.length === 0 ||
    utf8Length(subject) > 128 ||
    hasControlCharacter(subject) ||
    message.length === 0 ||
    utf8Length(message) > 4 * 1024 ||
    hasUnsafeMultilineControl(message)
  ) {
    throw new Error("CreateTicketCommandRequest contract violation");
  }
  return { schemaVersion: IPC_SCHEMA_VERSION, subject, message };
}

export function parseReplyTicketCommandRequest(
  value: unknown,
): ReplyTicketCommandRequest {
  if (
    !isRecord(value) ||
    !hasOnlyKeys(value, ["schemaVersion", "ticketId", "message"]) ||
    value.schemaVersion !== IPC_SCHEMA_VERSION ||
    typeof value.ticketId !== "string" ||
    !/^[1-9][0-9]{0,19}$/.test(value.ticketId) ||
    typeof value.message !== "string"
  ) {
    throw new Error("ReplyTicketCommandRequest contract violation");
  }
  const message = value.message.trim();
  if (
    message.length === 0 ||
    utf8Length(message) > 4 * 1024 ||
    hasUnsafeMultilineControl(message)
  ) {
    throw new Error("ReplyTicketCommandRequest contract violation");
  }
  return {
    schemaVersion: IPC_SCHEMA_VERSION,
    ticketId: value.ticketId,
    message,
  };
}

export function parseCloseTicketCommandRequest(
  value: unknown,
): CloseTicketCommandRequest {
  if (
    !isRecord(value) ||
    !hasOnlyKeys(value, ["schemaVersion", "ticketId"]) ||
    value.schemaVersion !== IPC_SCHEMA_VERSION ||
    typeof value.ticketId !== "string" ||
    !/^[1-9][0-9]{0,19}$/.test(value.ticketId)
  ) {
    throw new Error("CloseTicketCommandRequest contract violation");
  }
  return { schemaVersion: IPC_SCHEMA_VERSION, ticketId: value.ticketId };
}

export function parseSubscriptionRefreshRequest(
  value: unknown,
): SubscriptionRefreshRequest {
  if (
    !isRecord(value) ||
    !hasOnlyKeys(value, ["schemaVersion"]) ||
    value.schemaVersion !== IPC_SCHEMA_VERSION
  ) {
    throw new Error("SubscriptionRefreshRequest contract violation");
  }
  return { schemaVersion: IPC_SCHEMA_VERSION };
}

export function parseLogoutRequest(value: unknown): LogoutRequest {
  if (
    !isRecord(value) ||
    !hasOnlyKeys(value, ["schemaVersion"]) ||
    value.schemaVersion !== IPC_SCHEMA_VERSION
  ) {
    throw new Error("LogoutRequest contract violation");
  }
  return { schemaVersion: IPC_SCHEMA_VERSION };
}

export function parsePlaneStateRequest(value: unknown): PlaneStateRequest {
  if (
    !isRecord(value) ||
    !hasOnlyKeys(value, ["schemaVersion"]) ||
    value.schemaVersion !== IPC_SCHEMA_VERSION
  ) {
    throw new Error("PlaneStateRequest contract violation");
  }
  return { schemaVersion: IPC_SCHEMA_VERSION };
}

export function parseDataPlaneControlRequest(
  value: unknown,
): DataPlaneControlRequest {
  if (
    !isRecord(value) ||
    !hasOnlyKeys(value, ["schemaVersion", "action"]) ||
    value.schemaVersion !== IPC_SCHEMA_VERSION ||
    !isDataPlaneControlAction(value.action)
  ) {
    throw new Error("DataPlaneControlRequest contract violation");
  }
  return {
    schemaVersion: IPC_SCHEMA_VERSION,
    action: value.action,
  };
}

export function parseDataPlaneControlResponse(
  value: unknown,
): DataPlaneControlResponse {
  if (
    !isRecord(value) ||
    value.schemaVersion !== IPC_SCHEMA_VERSION ||
    !isControlPlaneState(value.controlPlane) ||
    !isDataPlaneState(value.dataPlane) ||
    typeof value.canStart !== "boolean" ||
    typeof value.canStop !== "boolean"
  ) {
    throw new Error("DataPlaneControlResponse contract violation");
  }
  return {
    schemaVersion: IPC_SCHEMA_VERSION,
    controlPlane: value.controlPlane,
    dataPlane: value.dataPlane,
    canStart: value.canStart,
    canStop: value.canStop,
  };
}

export function parseConnectionModeResponse(
  value: unknown,
): ConnectionModeResponse {
  if (
    !isRecord(value) ||
    value.schemaVersion !== IPC_SCHEMA_VERSION ||
    !isConnectionMode(value.mode)
  ) {
    throw new Error("ConnectionModeResponse contract violation");
  }
  return {
    schemaVersion: IPC_SCHEMA_VERSION,
    mode: value.mode,
  };
}

export function parseRoutingModeResponse(value: unknown): RoutingModeResponse {
  if (
    !isRecord(value) ||
    value.schemaVersion !== IPC_SCHEMA_VERSION ||
    !isRoutingMode(value.mode)
  ) {
    throw new Error("RoutingModeResponse contract violation");
  }
  return {
    schemaVersion: IPC_SCHEMA_VERSION,
    mode: value.mode,
  };
}

export function parseLaunchOnStartupResponse(
  value: unknown,
): LaunchOnStartupResponse {
  if (
    !isRecord(value) ||
    value.schemaVersion !== IPC_SCHEMA_VERSION ||
    typeof value.enabled !== "boolean"
  ) {
    throw new Error("LaunchOnStartupResponse contract violation");
  }
  return {
    schemaVersion: IPC_SCHEMA_VERSION,
    enabled: value.enabled,
  };
}

export function parseOpenServicePortalResponse(
  value: unknown,
): OpenServicePortalResponse {
  if (
    !isRecord(value) ||
    value.schemaVersion !== IPC_SCHEMA_VERSION ||
    value.opened !== true
  ) {
    throw new Error("OpenServicePortalResponse contract violation");
  }
  return {
    schemaVersion: IPC_SCHEMA_VERSION,
    opened: true,
  };
}

export function parseOpenNetworkToolResponse(
  value: unknown,
): OpenNetworkToolResponse {
  if (
    !isRecord(value) ||
    value.schemaVersion !== IPC_SCHEMA_VERSION ||
    !isNetworkTool(value.tool)
  ) {
    throw new Error("OpenNetworkToolResponse contract violation");
  }
  return {
    schemaVersion: IPC_SCHEMA_VERSION,
    tool: value.tool,
  };
}

export function parseOpenLegalDocumentResponse(
  value: unknown,
): OpenLegalDocumentResponse {
  if (
    !isRecord(value) ||
    value.schemaVersion !== IPC_SCHEMA_VERSION ||
    !isLegalDocument(value.document)
  ) {
    throw new Error("OpenLegalDocumentResponse contract violation");
  }
  return {
    schemaVersion: IPC_SCHEMA_VERSION,
    document: value.document,
  };
}

export function parseSubscriptionSnapshotResponse(
  value: unknown,
): SubscriptionSnapshotResponse {
  if (!isRecord(value) || value.schemaVersion !== IPC_SCHEMA_VERSION) {
    throw new Error("SubscriptionSnapshotResponse contract violation");
  }
  try {
    return {
      schemaVersion: IPC_SCHEMA_VERSION,
      subscription:
        value.subscription === null
          ? null
          : parseSubscriptionResponse(value.subscription),
      localRevision: parseSafeIpcInteger(value.localRevision, true),
    };
  } catch {
    throw new Error("SubscriptionSnapshotResponse contract violation");
  }
}

function parsePublicNode(value: unknown): PublicNode {
  if (!isRecord(value) || !isPublicNodeProtocol(value.protocol)) {
    throw new Error("NodeCatalogResponse contract violation");
  }
  return { id: parsePublicNodeId(value.id), protocol: value.protocol };
}

export function parseNodeCatalogResponse(value: unknown): NodeCatalogResponse {
  if (
    !isRecord(value) ||
    value.schemaVersion !== IPC_SCHEMA_VERSION ||
    !Array.isArray(value.groups) ||
    value.groups.length > 8
  ) {
    throw new Error("NodeCatalogResponse contract violation");
  }
  try {
    const groups = value.groups.map((candidate): PublicNodeGroup => {
      if (!isRecord(candidate) || !Array.isArray(candidate.nodes)) {
        throw new Error("NodeCatalogResponse contract violation");
      }
      const nodes = candidate.nodes.map(parsePublicNode);
      const selectedNodeId = parsePublicNodeId(candidate.selectedNodeId);
      if (
        nodes.length === 0 ||
        nodes.length > 64 ||
        !nodes.some((node) => node.id === selectedNodeId)
      ) {
        throw new Error("NodeCatalogResponse contract violation");
      }
      return {
        id: parsePublicNodeId(candidate.id),
        selectedNodeId,
        nodes,
      };
    });
    const revision = parseSafeIpcInteger(value.revision, true);
    if ((revision === null) !== (groups.length === 0)) {
      throw new Error("NodeCatalogResponse contract violation");
    }
    return { schemaVersion: IPC_SCHEMA_VERSION, revision, groups };
  } catch {
    throw new Error("NodeCatalogResponse contract violation");
  }
}

export function parseSelectNodeResponse(value: unknown): SelectNodeResponse {
  if (!isRecord(value) || value.schemaVersion !== IPC_SCHEMA_VERSION) {
    throw new Error("SelectNodeResponse contract violation");
  }
  try {
    return {
      schemaVersion: IPC_SCHEMA_VERSION,
      selectorId: parsePublicNodeId(value.selectorId),
      nodeId: parsePublicNodeId(value.nodeId),
    };
  } catch {
    throw new Error("SelectNodeResponse contract violation");
  }
}

export function parseNodeDelayTestResponse(
  value: unknown,
): NodeDelayTestResponse {
  if (
    !isRecord(value) ||
    value.schemaVersion !== IPC_SCHEMA_VERSION ||
    !Array.isArray(value.results) ||
    value.results.length > 64
  ) {
    throw new Error("NodeDelayTestResponse contract violation");
  }
  try {
    const results = value.results.map((candidate): PublicNodeDelayResult => {
      if (!isRecord(candidate) || !isRecord(candidate.result)) {
        throw new Error("NodeDelayTestResponse contract violation");
      }
      const status = candidate.result.status;
      let result: PublicNodeDelay;
      if (status === "available") {
        const delayMs = parseSafeIpcInteger(candidate.result.delayMs);
        if (delayMs === null || delayMs > 60_000) {
          throw new Error("NodeDelayTestResponse contract violation");
        }
        result = { status, delayMs };
      } else if (status === "timed_out" || status === "unavailable") {
        result = { status };
      } else {
        throw new Error("NodeDelayTestResponse contract violation");
      }
      return {
        selectorId: parsePublicNodeId(candidate.selectorId),
        nodeId: parsePublicNodeId(candidate.nodeId),
        result,
      };
    });
    return { schemaVersion: IPC_SCHEMA_VERSION, results };
  } catch {
    throw new Error("NodeDelayTestResponse contract violation");
  }
}

export function parsePlaneStateResponse(value: unknown): PlaneStateResponse {
  if (
    !isRecord(value) ||
    value.schemaVersion !== IPC_SCHEMA_VERSION ||
    !isControlPlaneState(value.controlPlane) ||
    !isDataPlaneState(value.dataPlane)
  ) {
    throw new Error("PlaneStateResponse contract violation");
  }
  return {
    schemaVersion: IPC_SCHEMA_VERSION,
    controlPlane: value.controlPlane,
    dataPlane: value.dataPlane,
  };
}

export function parseRuntimeInfoRequest(value: unknown): RuntimeInfoRequest {
  if (
    !isRecord(value) ||
    !hasOnlyKeys(value, ["schemaVersion"]) ||
    value.schemaVersion !== IPC_SCHEMA_VERSION
  ) {
    throw new Error("RuntimeInfoRequest contract violation");
  }

  return { schemaVersion: IPC_SCHEMA_VERSION };
}

export function parseRuntimeInfoResponse(value: unknown): RuntimeInfoResponse {
  if (
    !isRecord(value) ||
    value.schemaVersion !== IPC_SCHEMA_VERSION ||
    value.productName !== "Orange" ||
    typeof value.productVersion !== "string" ||
    value.productVersion.length === 0
  ) {
    throw new Error("RuntimeInfoResponse contract violation");
  }

  return {
    schemaVersion: IPC_SCHEMA_VERSION,
    productName: "Orange",
    productVersion: value.productVersion,
  };
}

export function parseCommandError(value: unknown): CommandError {
  if (
    !isRecord(value) ||
    value.schemaVersion !== IPC_SCHEMA_VERSION ||
    !isErrorCode(value.code) ||
    typeof value.message !== "string" ||
    value.message.length === 0 ||
    typeof value.retryable !== "boolean"
  ) {
    throw new Error("CommandError contract violation");
  }

  const definition = ERROR_DEFINITIONS[value.code];
  if (
    value.message !== definition.message ||
    value.retryable !== definition.retryable
  ) {
    throw new Error("CommandError contract violation");
  }

  return {
    schemaVersion: IPC_SCHEMA_VERSION,
    code: value.code,
    message: value.message,
    retryable: value.retryable,
  };
}

export async function getRuntimeInfo(): Promise<RuntimeInfoResponse> {
  const request: RuntimeInfoRequest = { schemaVersion: IPC_SCHEMA_VERSION };
  const response = await invoke<unknown>(COMMANDS.getRuntimeInfo, { request });
  return parseRuntimeInfoResponse(response);
}

export async function getPlaneState(): Promise<PlaneStateResponse> {
  const request: PlaneStateRequest = { schemaVersion: IPC_SCHEMA_VERSION };
  const response = await invoke<unknown>(COMMANDS.getPlaneState, { request });
  return parsePlaneStateResponse(response);
}

export async function getDataPlaneEventSnapshot(): Promise<DataPlaneEventSnapshot> {
  const request = { schemaVersion: IPC_SCHEMA_VERSION } as const;
  const response = await invoke<unknown>(COMMANDS.getDataPlaneEventSnapshot, {
    request,
  });
  return parseDataPlaneEventSnapshot(response);
}

export async function controlDataPlane(
  action: DataPlaneControlAction,
): Promise<DataPlaneControlResponse> {
  const request = parseDataPlaneControlRequest({
    schemaVersion: IPC_SCHEMA_VERSION,
    action,
  });
  const response = await invoke<unknown>(COMMANDS.controlDataPlane, {
    request,
  });
  return parseDataPlaneControlResponse(response);
}

export async function getConnectionMode(): Promise<ConnectionModeResponse> {
  const request = { schemaVersion: IPC_SCHEMA_VERSION } as const;
  const response = await invoke<unknown>(COMMANDS.getConnectionMode, {
    request,
  });
  return parseConnectionModeResponse(response);
}

export async function setConnectionMode(
  mode: ConnectionMode,
): Promise<ConnectionModeResponse> {
  const request = { schemaVersion: IPC_SCHEMA_VERSION, mode } as const;
  const response = await invoke<unknown>(COMMANDS.setConnectionMode, {
    request,
  });
  return parseConnectionModeResponse(response);
}

export async function getRoutingMode(): Promise<RoutingModeResponse> {
  const request = { schemaVersion: IPC_SCHEMA_VERSION } as const;
  const response = await invoke<unknown>(COMMANDS.getRoutingMode, { request });
  return parseRoutingModeResponse(response);
}

export async function setRoutingMode(
  mode: RoutingMode,
): Promise<RoutingModeResponse> {
  const request = { schemaVersion: IPC_SCHEMA_VERSION, mode } as const;
  const response = await invoke<unknown>(COMMANDS.setRoutingMode, { request });
  return parseRoutingModeResponse(response);
}

export async function getLaunchOnStartup(): Promise<LaunchOnStartupResponse> {
  const request = { schemaVersion: IPC_SCHEMA_VERSION } as const;
  const response = await invoke<unknown>(COMMANDS.getLaunchOnStartup, {
    request,
  });
  return parseLaunchOnStartupResponse(response);
}

export async function setLaunchOnStartup(
  enabled: boolean,
): Promise<LaunchOnStartupResponse> {
  const request = { schemaVersion: IPC_SCHEMA_VERSION, enabled } as const;
  const response = await invoke<unknown>(COMMANDS.setLaunchOnStartup, {
    request,
  });
  return parseLaunchOnStartupResponse(response);
}

export async function initializeBusiness(): Promise<BusinessInitializationResponse> {
  const request = { schemaVersion: IPC_SCHEMA_VERSION } as const;
  const response = await invoke<unknown>(COMMANDS.initializeBusiness, {
    request,
  });
  return parseBusinessInitializationResponse(response);
}

export async function openServicePortal(): Promise<OpenServicePortalResponse> {
  const request = { schemaVersion: IPC_SCHEMA_VERSION } as const;
  const response = await invoke<unknown>(COMMANDS.openServicePortal, {
    request,
  });
  return parseOpenServicePortalResponse(response);
}

export async function openTelegramBot(): Promise<OpenServicePortalResponse> {
  const request = { schemaVersion: IPC_SCHEMA_VERSION } as const;
  const response = await invoke<unknown>(COMMANDS.openTelegramBot, {
    request,
  });
  return parseOpenServicePortalResponse(response);
}

export async function getServicePortalUrl(): Promise<ServicePortalUrlResponse> {
  const request = { schemaVersion: IPC_SCHEMA_VERSION } as const;
  const response = await invoke<unknown>(COMMANDS.getServicePortalUrl, {
    request,
  });
  if (
    !isRecord(response) ||
    response.schemaVersion !== IPC_SCHEMA_VERSION ||
    typeof response.url !== "string" ||
    response.url.length === 0
  ) {
    throw new Error("ServicePortalUrlResponse contract violation");
  }
  return {
    schemaVersion: IPC_SCHEMA_VERSION,
    url: response.url,
  };
}

export async function openNetworkTool(
  tool: NetworkTool,
): Promise<OpenNetworkToolResponse> {
  const request = { schemaVersion: IPC_SCHEMA_VERSION, tool } as const;
  const response = await invoke<unknown>(COMMANDS.openNetworkTool, { request });
  return parseOpenNetworkToolResponse(response);
}

export async function openLegalDocument(
  document: LegalDocument,
): Promise<OpenLegalDocumentResponse> {
  const request = { schemaVersion: IPC_SCHEMA_VERSION, document } as const;
  const response = await invoke<unknown>(COMMANDS.openLegalDocument, {
    request,
  });
  return parseOpenLegalDocumentResponse(response);
}

export async function login(
  input: LoginFormInput,
): Promise<AuthPublicResponse> {
  const request = parseLoginCommandRequest(input);
  const response = await invoke<unknown>(COMMANDS.login, { request });
  return parseAuthPublicResponse(response);
}

export async function sendEmailVerification(
  email: string,
): Promise<EmailVerificationResponse> {
  const request = parseSendEmailVerificationCommandRequest({ email });
  const response = await invoke<unknown>(COMMANDS.sendEmailVerification, {
    request,
  });
  return parseEmailVerificationResponse(response);
}

export async function resetPassword(
  input: ResetPasswordFormInput,
): Promise<PasswordResetResponse> {
  const request = parseResetPasswordCommandRequest(input);
  const response = await invoke<unknown>(COMMANDS.resetPassword, { request });
  return parsePasswordResetResponse(response);
}

export async function register(
  input: RegisterFormInput,
): Promise<AuthPublicResponse> {
  const request = parseRegisterCommandRequest(input);
  const response = await invoke<unknown>(COMMANDS.register, { request });
  return parseAuthPublicResponse(response);
}

export async function getAuthSession(): Promise<AuthSessionResponse> {
  const request = { schemaVersion: IPC_SCHEMA_VERSION } as const;
  const response = await invoke<unknown>(COMMANDS.getAuthSession, { request });
  return parseAuthSessionResponse(response);
}

export async function logout(): Promise<AuthSessionResponse> {
  const request = parseLogoutRequest({ schemaVersion: IPC_SCHEMA_VERSION });
  const response = await invoke<unknown>(COMMANDS.logout, { request });
  return parseAuthSessionResponse(response);
}

export async function refreshAccount(): Promise<AccountResponse> {
  const request = parseAccountRefreshRequest({
    schemaVersion: IPC_SCHEMA_VERSION,
  });
  const response = await invoke<unknown>(COMMANDS.refreshAccount, { request });
  return parseAccountResponse(response);
}

export async function fetchNotices(): Promise<NoticesResponse> {
  const request = parseNoticesRequest({ schemaVersion: IPC_SCHEMA_VERSION });
  const response = await invoke<unknown>(COMMANDS.fetchNotices, { request });
  return parseNoticesResponse(response);
}

export async function fetchPlans(): Promise<PlansResponse> {
  const request = parsePlansRequest({ schemaVersion: IPC_SCHEMA_VERSION });
  const response = await invoke<unknown>(COMMANDS.fetchPlans, { request });
  return parsePlansResponse(response);
}

export async function fetchOrders(): Promise<OrdersResponse> {
  const request = parseOrdersRequest({ schemaVersion: IPC_SCHEMA_VERSION });
  const response = await invoke<unknown>(COMMANDS.fetchOrders, { request });
  return parseOrdersResponse(response);
}

export async function fetchOrderDetail(
  orderId: string,
): Promise<OrderDetailResponse> {
  const request = parseOrderDetailCommandRequest({
    schemaVersion: IPC_SCHEMA_VERSION,
    orderId,
  });
  const response = await invoke<unknown>(COMMANDS.fetchOrderDetail, {
    request,
  });
  return parseOrderDetailResponse(response);
}

export async function fetchPaymentMethods(): Promise<PaymentMethodsResponse> {
  const request = parsePaymentMethodsRequest({
    schemaVersion: IPC_SCHEMA_VERSION,
  });
  const response = await invoke<unknown>(COMMANDS.fetchPaymentMethods, {
    request,
  });
  return parsePaymentMethodsResponse(response);
}

export async function checkoutOrder(
  orderId: string,
  paymentMethod: string,
): Promise<PaymentPublicResponse> {
  const request = parseCheckoutOrderCommandRequest({
    schemaVersion: IPC_SCHEMA_VERSION,
    orderId,
    paymentMethod,
  });
  const response = await invoke<unknown>(COMMANDS.checkoutOrder, { request });
  return parsePaymentResponse(response);
}

export async function cancelOrder(
  orderId: string,
): Promise<CancelOrderResponse> {
  const request = parseCancelOrderCommandRequest({
    schemaVersion: IPC_SCHEMA_VERSION,
    orderId,
  });
  const response = await invoke<unknown>(COMMANDS.cancelOrder, { request });
  return parseCancelOrderResponse(response);
}

export async function createOrder(
  planId: string,
  couponCode?: string,
): Promise<CreateOrderResponse> {
  const request = parseCreateOrderCommandRequest({
    schemaVersion: IPC_SCHEMA_VERSION,
    planId,
    couponCode,
  });
  const response = await invoke<unknown>(COMMANDS.createOrder, { request });
  return parseCreateOrderResponse(response);
}

export async function fetchInvitationCenter(): Promise<InvitationCenterResponse> {
  const request = parseInvitationCenterRequest({
    schemaVersion: IPC_SCHEMA_VERSION,
  });
  const response = await invoke<unknown>(COMMANDS.fetchInvitationCenter, {
    request,
  });
  return parseInvitationCenterResponse(response);
}

export async function generateInvitationCode(): Promise<InvitationCenterResponse> {
  const request = parseInvitationCenterRequest({
    schemaVersion: IPC_SCHEMA_VERSION,
  });
  const response = await invoke<unknown>(COMMANDS.generateInvitationCode, {
    request,
  });
  return parseInvitationCenterResponse(response);
}

export async function fetchTickets(): Promise<TicketsResponse> {
  const request = parseTicketsRequest({ schemaVersion: IPC_SCHEMA_VERSION });
  const response = await invoke<unknown>(COMMANDS.fetchTickets, { request });
  return parseTicketsResponse(response);
}

export async function fetchTicketDetail(
  ticketId: string,
): Promise<TicketDetailResponse> {
  const request = parseTicketDetailCommandRequest({
    schemaVersion: IPC_SCHEMA_VERSION,
    ticketId,
  });
  const response = await invoke<unknown>(COMMANDS.fetchTicketDetail, {
    request,
  });
  return parseTicketDetailResponse(response);
}

export async function createTicket(
  subject: string,
  message: string,
): Promise<TicketsResponse> {
  const request = parseCreateTicketCommandRequest({
    schemaVersion: IPC_SCHEMA_VERSION,
    subject,
    message,
  });
  const response = await invoke<unknown>(COMMANDS.createTicket, { request });
  return parseTicketsResponse(response);
}

export async function replyTicket(
  ticketId: string,
  message: string,
): Promise<TicketDetailResponse> {
  const request = parseReplyTicketCommandRequest({
    schemaVersion: IPC_SCHEMA_VERSION,
    ticketId,
    message,
  });
  const response = await invoke<unknown>(COMMANDS.replyTicket, { request });
  return parseTicketDetailResponse(response);
}

export async function closeTicket(
  ticketId: string,
): Promise<TicketDetailResponse> {
  const request = parseCloseTicketCommandRequest({
    schemaVersion: IPC_SCHEMA_VERSION,
    ticketId,
  });
  const response = await invoke<unknown>(COMMANDS.closeTicket, { request });
  return parseTicketDetailResponse(response);
}

export async function refreshSubscription(): Promise<SubscriptionPublicResponse> {
  const request = parseSubscriptionRefreshRequest({
    schemaVersion: IPC_SCHEMA_VERSION,
  });
  const response = await invoke<unknown>(COMMANDS.refreshSubscription, {
    request,
  });
  return parseSubscriptionResponse(response);
}

export async function fetchSubscriptionLink(): Promise<SubscriptionLinkResponse> {
  const request = parseSubscriptionRefreshRequest({
    schemaVersion: IPC_SCHEMA_VERSION,
  });
  const response = await invoke<unknown>(COMMANDS.fetchSubscriptionLink, {
    request,
  });
  return parseSubscriptionLinkResponse(response);
}

export async function resetSubscriptionLink(): Promise<SubscriptionLinkResponse> {
  const request = parseSubscriptionRefreshRequest({
    schemaVersion: IPC_SCHEMA_VERSION,
  });
  const response = await invoke<unknown>(COMMANDS.resetSubscriptionLink, {
    request,
  });
  return parseSubscriptionLinkResponse(response);
}

export async function fetchKnowledgeList(
  keyword?: string,
): Promise<KnowledgeListResponse> {
  const trimmed = keyword?.trim();
  const request = {
    schemaVersion: IPC_SCHEMA_VERSION,
    ...(trimmed ? { keyword: trimmed } : {}),
  } as const;
  const response = await invoke<unknown>(COMMANDS.fetchKnowledgeList, {
    request,
  });
  return parseKnowledgeListResponse(response);
}

export async function fetchKnowledgeDetail(
  articleId: string,
): Promise<KnowledgeDetailResponse> {
  if (!/^[0-9]{1,32}$/.test(articleId)) {
    throw new Error("KnowledgeDetailCommandRequest contract violation");
  }
  const request = {
    schemaVersion: IPC_SCHEMA_VERSION,
    articleId,
  } as const;
  const response = await invoke<unknown>(COMMANDS.fetchKnowledgeDetail, {
    request,
  });
  return parseKnowledgeDetailResponse(response);
}

export async function fetchActiveSessions(): Promise<ActiveSessionsResponse> {
  const request = { schemaVersion: IPC_SCHEMA_VERSION } as const;
  const response = await invoke<unknown>(COMMANDS.fetchActiveSessions, {
    request,
  });
  return parseActiveSessionsResponse(response);
}

export async function removeActiveSession(
  sessionId: string,
): Promise<CommissionOperationResponse> {
  if (!/^[0-9]{1,32}$/.test(sessionId)) {
    throw new Error("RemoveActiveSessionCommandRequest contract violation");
  }
  const request = {
    schemaVersion: IPC_SCHEMA_VERSION,
    sessionId,
  } as const;
  const response = await invoke<unknown>(COMMANDS.removeActiveSession, {
    request,
  });
  return parseCommissionOperationResponse(response);
}

export async function fetchCommissionConfig(): Promise<CommissionConfigResponse> {
  const request = { schemaVersion: IPC_SCHEMA_VERSION } as const;
  const response = await invoke<unknown>(COMMANDS.fetchCommissionConfig, {
    request,
  });
  return parseCommissionConfigResponse(response);
}

export async function withdrawCommission(
  withdrawMethod: string,
  withdrawAccount: string,
): Promise<CommissionOperationResponse> {
  const method = withdrawMethod.trim();
  const account = withdrawAccount.trim();
  if (
    method.length === 0 ||
    method.length > 64 ||
    account.length === 0 ||
    account.length > 512
  ) {
    throw new Error("WithdrawCommissionCommandRequest contract violation");
  }
  const request = {
    schemaVersion: IPC_SCHEMA_VERSION,
    withdrawMethod: method,
    withdrawAccount: account,
  } as const;
  const response = await invoke<unknown>(COMMANDS.withdrawCommission, {
    request,
  });
  return parseCommissionOperationResponse(response);
}

export async function transferCommission(
  amountMinor: number,
): Promise<CommissionOperationResponse> {
  if (
    !Number.isSafeInteger(amountMinor) ||
    amountMinor <= 0
  ) {
    throw new Error("TransferCommissionCommandRequest contract violation");
  }
  const request = {
    schemaVersion: IPC_SCHEMA_VERSION,
    amountMinor,
  } as const;
  const response = await invoke<unknown>(COMMANDS.transferCommission, {
    request,
  });
  return parseCommissionOperationResponse(response);
}

function parseGiftCardCode(value: unknown): string {
  if (
    typeof value !== "string" ||
    value.trim().length < 8 ||
    value.trim().length > 64 ||
    !/^[ -~]*$/.test(value)
  ) {
    throw new Error("GiftCardCodeCommandRequest contract violation");
  }
  return value.trim();
}

export async function checkGiftCard(
  code: string,
): Promise<GiftCardCheckResponse> {
  const request = {
    schemaVersion: IPC_SCHEMA_VERSION,
    code: parseGiftCardCode(code),
  } as const;
  const response = await invoke<unknown>(COMMANDS.checkGiftCard, { request });
  return parseGiftCardCheckResponse(response);
}

export async function redeemGiftCard(
  code: string,
): Promise<GiftCardRedeemResponse> {
  const request = {
    schemaVersion: IPC_SCHEMA_VERSION,
    code: parseGiftCardCode(code),
  } as const;
  const response = await invoke<unknown>(COMMANDS.redeemGiftCard, { request });
  return parseGiftCardRedeemResponse(response);
}

export async function fetchGiftCardHistory(): Promise<GiftCardHistoryResponse> {
  const request = { schemaVersion: IPC_SCHEMA_VERSION } as const;
  const response = await invoke<unknown>(COMMANDS.fetchGiftCardHistory, {
    request,
  });
  return parseGiftCardHistoryResponse(response);
}

export async function getSubscriptionSnapshot(): Promise<SubscriptionSnapshotResponse> {
  const request = { schemaVersion: IPC_SCHEMA_VERSION } as const;
  const response = await invoke<unknown>(COMMANDS.getSubscriptionSnapshot, {
    request,
  });
  return parseSubscriptionSnapshotResponse(response);
}

export async function getNodeCatalog(): Promise<NodeCatalogResponse> {
  const request = { schemaVersion: IPC_SCHEMA_VERSION } as const;
  const response = await invoke<unknown>(COMMANDS.getNodeCatalog, { request });
  return parseNodeCatalogResponse(response);
}

export async function selectNode(
  selectorId: string,
  nodeId: string,
): Promise<SelectNodeResponse> {
  const request = {
    schemaVersion: IPC_SCHEMA_VERSION,
    selectorId: parsePublicNodeId(selectorId),
    nodeId: parsePublicNodeId(nodeId),
  } as const;
  const response = await invoke<unknown>(COMMANDS.selectNode, { request });
  return parseSelectNodeResponse(response);
}

export async function testNodeDelays(): Promise<NodeDelayTestResponse> {
  const request = { schemaVersion: IPC_SCHEMA_VERSION } as const;
  const response = await invoke<unknown>(COMMANDS.testNodeDelays, { request });
  return parseNodeDelayTestResponse(response);
}

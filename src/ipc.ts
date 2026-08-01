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
  type CreateOrderResponse,
  type OrdersResponse,
  type PlansResponse,
  type SubscriptionPublicResponse,
  parseAccountResponse,
  parseAuthPublicResponse,
  parseAuthSessionResponse,
  parseBusinessInitializationResponse,
  parseCreateOrderResponse,
  parseOrdersResponse,
  parsePlansResponse,
  parseSubscriptionResponse,
} from "./businessApi";

export const IPC_SCHEMA_VERSION = 2 as const;

export const DATA_PLANE_CONTROL_ACTIONS = ["status", "start", "stop"] as const;
export type DataPlaneControlAction =
  (typeof DATA_PLANE_CONTROL_ACTIONS)[number];

export const CONNECTION_MODES = ["system_proxy", "tun"] as const;
export type ConnectionMode = (typeof CONNECTION_MODES)[number];

export const COMMANDS = {
  getPlaneState: "get_plane_state",
  getRuntimeInfo: "get_runtime_info",
  getDataPlaneEventSnapshot: "get_data_plane_event_snapshot",
  controlDataPlane: "control_data_plane",
  getConnectionMode: "get_connection_mode",
  setConnectionMode: "set_connection_mode",
  initializeBusiness: "initialize_business",
  login: "login",
  register: "register",
  getAuthSession: "get_auth_session",
  logout: "logout",
  refreshAccount: "refresh_account",
  fetchPlans: "fetch_plans",
  fetchOrders: "fetch_orders",
  createOrder: "create_order",
  refreshSubscription: "refresh_subscription",
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
  inviteCode: string | null;
}

export interface LoginCommandRequest extends LoginFormInput {
  schemaVersion: typeof IPC_SCHEMA_VERSION;
}

export interface RegisterCommandRequest extends RegisterFormInput {
  schemaVersion: typeof IPC_SCHEMA_VERSION;
}

export interface AccountRefreshRequest {
  schemaVersion: typeof IPC_SCHEMA_VERSION;
}

export interface PlansRequest {
  schemaVersion: typeof IPC_SCHEMA_VERSION;
}

export interface OrdersRequest {
  schemaVersion: typeof IPC_SCHEMA_VERSION;
}

export interface CreateOrderCommandRequest {
  schemaVersion: typeof IPC_SCHEMA_VERSION;
  planId: string;
}

export interface SubscriptionRefreshRequest {
  schemaVersion: typeof IPC_SCHEMA_VERSION;
}

export interface LogoutRequest {
  schemaVersion: typeof IPC_SCHEMA_VERSION;
}

export type AuthFormField = "email" | "password" | "inviteCode";

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
  validateInviteCode(value.inviteCode);
  return {
    schemaVersion: IPC_SCHEMA_VERSION,
    email: value.email,
    password: value.password,
    inviteCode: value.inviteCode,
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

export function parseCreateOrderCommandRequest(
  value: unknown,
): CreateOrderCommandRequest {
  if (
    !isRecord(value) ||
    !hasOnlyKeys(value, ["schemaVersion", "planId"]) ||
    value.schemaVersion !== IPC_SCHEMA_VERSION ||
    typeof value.planId !== "string" ||
    value.planId.length > 64 ||
    !/^[1-9][0-9]*:(month_price|quarter_price|half_year_price|year_price|two_year_price|three_year_price|onetime_price)$/.test(
      value.planId,
    )
  ) {
    throw new Error("CreateOrderCommandRequest contract violation");
  }
  return {
    schemaVersion: IPC_SCHEMA_VERSION,
    planId: value.planId,
  };
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

export async function initializeBusiness(): Promise<BusinessInitializationResponse> {
  const request = { schemaVersion: IPC_SCHEMA_VERSION } as const;
  const response = await invoke<unknown>(COMMANDS.initializeBusiness, {
    request,
  });
  return parseBusinessInitializationResponse(response);
}

export async function login(
  input: LoginFormInput,
): Promise<AuthPublicResponse> {
  const request = parseLoginCommandRequest(input);
  const response = await invoke<unknown>(COMMANDS.login, { request });
  return parseAuthPublicResponse(response);
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

export async function createOrder(
  planId: string,
): Promise<CreateOrderResponse> {
  const request = parseCreateOrderCommandRequest({
    schemaVersion: IPC_SCHEMA_VERSION,
    planId,
  });
  const response = await invoke<unknown>(COMMANDS.createOrder, { request });
  return parseCreateOrderResponse(response);
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

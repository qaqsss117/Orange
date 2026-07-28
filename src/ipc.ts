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
  type SubscriptionPublicResponse,
  parseAccountResponse,
  parseAuthPublicResponse,
  parseAuthSessionResponse,
  parseBusinessInitializationResponse,
  parseSubscriptionResponse,
} from "./businessApi";

export const IPC_SCHEMA_VERSION = 1 as const;

export const COMMANDS = {
  getPlaneState: "get_plane_state",
  getRuntimeInfo: "get_runtime_info",
  getDataPlaneEventSnapshot: "get_data_plane_event_snapshot",
  initializeBusiness: "initialize_business",
  login: "login",
  register: "register",
  getAuthSession: "get_auth_session",
  logout: "logout",
  refreshAccount: "refresh_account",
  refreshSubscription: "refresh_subscription",
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

export async function refreshSubscription(): Promise<SubscriptionPublicResponse> {
  const request = parseSubscriptionRefreshRequest({
    schemaVersion: IPC_SCHEMA_VERSION,
  });
  const response = await invoke<unknown>(COMMANDS.refreshSubscription, {
    request,
  });
  return parseSubscriptionResponse(response);
}

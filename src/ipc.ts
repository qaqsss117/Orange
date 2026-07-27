import { invoke } from "@tauri-apps/api/core";

export const IPC_SCHEMA_VERSION = 1 as const;

export const COMMANDS = {
  getRuntimeInfo: "get_runtime_info",
} as const;

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

export interface RuntimeInfoResponse {
  schemaVersion: typeof IPC_SCHEMA_VERSION;
  productName: "Orange";
  productVersion: string;
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

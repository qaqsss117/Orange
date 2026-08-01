import { listen } from "@tauri-apps/api/event";
import type {
  AccountResponse,
  AuthPublicResponse,
  AuthSessionResponse,
  BusinessInitializationResponse,
  ConfigResponse,
  UserProfile,
} from "./businessApi";
import {
  AuthFormError,
  ERROR_DEFINITIONS,
  IPC_SCHEMA_VERSION,
  controlDataPlane,
  type ConnectionMode,
  type ConnectionModeResponse,
  type DataPlaneControlAction,
  type DataPlaneControlResponse,
  type NodeCatalogResponse,
  type NodeDelayTestResponse,
  type SelectNodeResponse,
  type SubscriptionSnapshotResponse,
  type LoginFormInput,
  type RegisterFormInput,
  getDataPlaneEventSnapshot,
  getConnectionMode,
  getNodeCatalog,
  getPlaneState,
  getSubscriptionSnapshot,
  initializeBusiness,
  login,
  logout,
  parseCommandError,
  register,
  refreshAccount,
  selectNode,
  setConnectionMode,
  testNodeDelays,
  refreshSubscription,
} from "./ipc";
import type { DataPlaneEventSnapshot } from "./events";
import type { PlaneStateResponse } from "./ipc";
import { SHELL_TEXT } from "./shellContent";

export interface ShellServices {
  initializeBusiness(): Promise<BusinessInitializationResponse>;
  login(input: LoginFormInput): Promise<AuthPublicResponse>;
  register(input: RegisterFormInput): Promise<AuthPublicResponse>;
  logout(): Promise<AuthSessionResponse>;
  refreshAccount(): Promise<AccountResponse>;
  getPlaneState(): Promise<PlaneStateResponse>;
  getDataPlaneEventSnapshot(): Promise<DataPlaneEventSnapshot>;
  controlDataPlane(
    action: DataPlaneControlAction,
  ): Promise<DataPlaneControlResponse>;
  getConnectionMode(): Promise<ConnectionModeResponse>;
  setConnectionMode(mode: ConnectionMode): Promise<ConnectionModeResponse>;
  getSubscriptionSnapshot(): Promise<SubscriptionSnapshotResponse>;
  refreshSubscription(): Promise<
    import("./businessApi").SubscriptionPublicResponse
  >;
  getNodeCatalog(): Promise<NodeCatalogResponse>;
  selectNode(selectorId: string, nodeId: string): Promise<SelectNodeResponse>;
  testNodeDelays(): Promise<NodeDelayTestResponse>;
  listenForTrayRuntimeError(
    handler: (kind: "action" | "exit") => void,
  ): Promise<() => void>;
}

export interface PublicUiError {
  message: string;
  retryable: boolean;
  field: "email" | "password" | "inviteCode" | null;
}

export const SHELL_PREVIEW_MODES = [
  "authenticated",
  "auth-error",
  "loading",
  "maintenance",
  "signed-out",
  "startup-error",
  "unverified",
] as const;

export type ShellPreviewMode = (typeof SHELL_PREVIEW_MODES)[number];

export const nativeShellServices: ShellServices = {
  initializeBusiness,
  login,
  register,
  logout,
  refreshAccount,
  getPlaneState,
  getDataPlaneEventSnapshot,
  controlDataPlane,
  getConnectionMode,
  setConnectionMode,
  getSubscriptionSnapshot,
  refreshSubscription,
  getNodeCatalog,
  selectNode,
  testNodeDelays,
  async listenForTrayRuntimeError(handler) {
    const unlistenAction = await listen("orange://tray-action-error", () =>
      handler("action"),
    );
    try {
      const unlistenExit = await listen("orange://tray-exit-error", () =>
        handler("exit"),
      );
      return () => {
        unlistenAction();
        unlistenExit();
      };
    } catch (error) {
      unlistenAction();
      throw error;
    }
  },
};

const FIELD_ERRORS = {
  email: SHELL_TEXT.emailInvalid,
  password: SHELL_TEXT.passwordInvalid,
  inviteCode: SHELL_TEXT.inviteInvalid,
} as const;

function previewUser(email: string = SHELL_TEXT.previewUserEmail): UserProfile {
  return {
    userId: "preview-user",
    email,
    status: "active",
    balance: { minorUnits: 3680, currency: "CNY" },
  };
}

function previewConfig(maintenance = false): ConfigResponse {
  return {
    schemaVersion: 1,
    minimumSupportedVersion: "0.1.0",
    maintenance,
    notice: maintenance ? SHELL_TEXT.maintenanceDetail : null,
    registrationRequiresInvite: false,
  };
}

function previewCommandError(code: keyof typeof ERROR_DEFINITIONS): object {
  return {
    schemaVersion: IPC_SCHEMA_VERSION,
    code,
    ...ERROR_DEFINITIONS[code],
  };
}

function authenticatedResponse(email: string): AuthPublicResponse {
  return {
    schemaVersion: 1,
    authenticated: true,
    user: previewUser(email),
  };
}

export function readShellPreview(
  search: string,
  developmentEnabled: boolean,
): ShellPreviewMode | null {
  if (!developmentEnabled) {
    return null;
  }
  const value = new URLSearchParams(search).get("shell");
  return value !== null &&
    (SHELL_PREVIEW_MODES as readonly string[]).includes(value)
    ? (value as ShellPreviewMode)
    : null;
}

export function createPreviewShellServices(
  mode: ShellPreviewMode,
): ShellServices {
  let previewDataPlane: DataPlaneControlResponse["dataPlane"] =
    mode === "authenticated" ? "online" : "unconfigured";
  let previewConnectionMode: ConnectionMode = "system_proxy";
  let previewSelectedNode = "node-01";

  const previewSubscription = {
    schemaVersion: 1 as const,
    status: "active" as const,
    planId: "orange-standard",
    expiresAtUnixMs: 1_798_761_600_000,
    usedBytes: 32 * 1024 * 1024 * 1024,
    totalBytes: 100 * 1024 * 1024 * 1024,
  };

  function previewDataPlaneResponse(): DataPlaneControlResponse {
    return {
      schemaVersion: IPC_SCHEMA_VERSION,
      controlPlane: "ready",
      dataPlane: previewDataPlane,
      canStart:
        mode === "authenticated" &&
        ["unconfigured", "permission_required", "failed"].includes(
          previewDataPlane,
        ),
      canStop: previewDataPlane === "online",
    };
  }

  return {
    async initializeBusiness() {
      if (mode === "loading") {
        return await new Promise<BusinessInitializationResponse>(
          () => undefined,
        );
      }
      if (mode === "startup-error") {
        throw previewCommandError("bootstrap");
      }
      const status =
        mode === "authenticated"
          ? "authenticated"
          : mode === "unverified"
            ? "unverified"
            : "signed_out";
      return {
        schemaVersion: 1,
        config: previewConfig(mode === "maintenance"),
        session: {
          schemaVersion: 1,
          status,
          user: status === "signed_out" ? null : previewUser(),
        },
      };
    },
    async login(input) {
      if (mode === "auth-error") {
        throw previewCommandError("network");
      }
      return authenticatedResponse(input.email);
    },
    async register(input) {
      if (mode === "auth-error") {
        throw previewCommandError("network");
      }
      return authenticatedResponse(input.email);
    },
    async logout() {
      return {
        schemaVersion: 1,
        status: "signed_out",
        user: null,
      };
    },
    async refreshAccount() {
      return {
        schemaVersion: 1,
        user: previewUser(),
      };
    },
    async getPlaneState() {
      return {
        schemaVersion: IPC_SCHEMA_VERSION,
        controlPlane: "ready",
        dataPlane: previewDataPlane,
      };
    },
    async controlDataPlane(action) {
      if (action === "start" && previewDataPlaneResponse().canStart) {
        previewDataPlane = "online";
      } else if (action === "stop" && previewDataPlaneResponse().canStop) {
        previewDataPlane = "unconfigured";
      }
      return previewDataPlaneResponse();
    },
    async getDataPlaneEventSnapshot() {
      if (mode !== "authenticated") {
        return {
          schemaVersion: 1,
          capacity: 64,
          droppedCount: 0,
          streamInstanceId: null,
          events: [],
        };
      }
      return {
        schemaVersion: 1,
        capacity: 64,
        droppedCount: 0,
        streamInstanceId: 7,
        events: [
          {
            schemaVersion: 1,
            instanceId: 7,
            sequence: 1,
            occurredAtUnixMs: 1785157200000,
            event: { kind: "data_state", state: "online" },
          },
          {
            schemaVersion: 1,
            instanceId: 7,
            sequence: 2,
            occurredAtUnixMs: 1785157200500,
            event: {
              kind: "traffic",
              sample: {
                uploadBytesTotal: 4_194_304,
                downloadBytesTotal: 12_582_912,
                uploadBytesPerSecond: 786_432,
                downloadBytesPerSecond: 2_621_440,
              },
            },
          },
        ],
      };
    },
    async getConnectionMode() {
      return {
        schemaVersion: IPC_SCHEMA_VERSION,
        mode: previewConnectionMode,
      };
    },
    async setConnectionMode(mode) {
      previewConnectionMode = mode;
      return {
        schemaVersion: IPC_SCHEMA_VERSION,
        mode: previewConnectionMode,
      };
    },
    async getSubscriptionSnapshot() {
      return {
        schemaVersion: IPC_SCHEMA_VERSION,
        subscription: mode === "authenticated" ? previewSubscription : null,
        localRevision: mode === "authenticated" ? 1_785_157_200_000 : null,
      };
    },
    async refreshSubscription() {
      return previewSubscription;
    },
    async getNodeCatalog() {
      return {
        schemaVersion: IPC_SCHEMA_VERSION,
        revision: mode === "authenticated" ? 1_785_157_200_000 : null,
        groups:
          mode === "authenticated"
            ? [
                {
                  id: "proxy",
                  selectedNodeId: previewSelectedNode,
                  nodes: [
                    { id: "node-01", protocol: "vless" },
                    { id: "node-02", protocol: "vless" },
                  ],
                },
              ]
            : [],
      };
    },
    async selectNode(selectorId, nodeId) {
      previewSelectedNode = nodeId;
      return {
        schemaVersion: IPC_SCHEMA_VERSION,
        selectorId,
        nodeId,
      };
    },
    async testNodeDelays() {
      return {
        schemaVersion: IPC_SCHEMA_VERSION,
        results: [
          {
            selectorId: "proxy",
            nodeId: "node-01",
            result: { status: "available", delayMs: 42 },
          },
          {
            selectorId: "proxy",
            nodeId: "node-02",
            result: { status: "available", delayMs: 96 },
          },
        ],
      };
    },
    async listenForTrayRuntimeError() {
      return () => undefined;
    },
  };
}

export function toPublicUiError(error: unknown): PublicUiError {
  if (error instanceof AuthFormError) {
    return {
      message: FIELD_ERRORS[error.field],
      retryable: false,
      field: error.field,
    };
  }
  try {
    const parsed = parseCommandError(error);
    return {
      message: parsed.message,
      retryable: parsed.retryable,
      field: null,
    };
  } catch {
    return {
      message: SHELL_TEXT.operationFailed,
      retryable: true,
      field: null,
    };
  }
}

import type {
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
  type LoginFormInput,
  type RegisterFormInput,
  getDataPlaneEventSnapshot,
  getPlaneState,
  initializeBusiness,
  login,
  logout,
  parseCommandError,
  register,
} from "./ipc";
import type { DataPlaneEventSnapshot } from "./events";
import type { PlaneStateResponse } from "./ipc";
import { SHELL_TEXT } from "./shellContent";

export interface ShellServices {
  initializeBusiness(): Promise<BusinessInitializationResponse>;
  login(input: LoginFormInput): Promise<AuthPublicResponse>;
  register(input: RegisterFormInput): Promise<AuthPublicResponse>;
  logout(): Promise<AuthSessionResponse>;
  getPlaneState(): Promise<PlaneStateResponse>;
  getDataPlaneEventSnapshot(): Promise<DataPlaneEventSnapshot>;
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
  getPlaneState,
  getDataPlaneEventSnapshot,
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
    balance: { minorUnits: 0, currency: "CNY" },
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
    async getPlaneState() {
      return {
        schemaVersion: 1,
        controlPlane: "ready",
        dataPlane: mode === "authenticated" ? "online" : "unconfigured",
      };
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

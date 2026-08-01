import { listen } from "@tauri-apps/api/event";
import type {
  AccountResponse,
  AuthPublicResponse,
  AuthSessionResponse,
  BusinessInitializationResponse,
  CancelOrderResponse,
  ConfigResponse,
  CreateOrderResponse,
  InvitationCenterResponse,
  OrderDetailResponse,
  OrdersResponse,
  PaymentMethodsResponse,
  PaymentPublicResponse,
  PlansResponse,
  TicketsResponse,
  TicketDetailResponse,
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
  fetchPlans,
  fetchOrders,
  fetchOrderDetail,
  fetchPaymentMethods,
  checkoutOrder,
  cancelOrder,
  createOrder,
  fetchInvitationCenter,
  generateInvitationCode,
  fetchTickets,
  fetchTicketDetail,
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
  fetchPlans(): Promise<PlansResponse>;
  fetchOrders(): Promise<OrdersResponse>;
  fetchOrderDetail(orderId: string): Promise<OrderDetailResponse>;
  fetchPaymentMethods(): Promise<PaymentMethodsResponse>;
  checkoutOrder(
    orderId: string,
    paymentMethod: string,
  ): Promise<PaymentPublicResponse>;
  cancelOrder(orderId: string): Promise<CancelOrderResponse>;
  createOrder(planId: string): Promise<CreateOrderResponse>;
  fetchInvitationCenter(): Promise<InvitationCenterResponse>;
  generateInvitationCode(): Promise<InvitationCenterResponse>;
  fetchTickets(): Promise<TicketsResponse>;
  fetchTicketDetail(ticketId: string): Promise<TicketDetailResponse>;
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
  fetchPlans,
  fetchOrders,
  fetchOrderDetail,
  fetchPaymentMethods,
  checkoutOrder,
  cancelOrder,
  createOrder,
  fetchInvitationCenter,
  generateInvitationCode,
  fetchTickets,
  fetchTicketDetail,
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
    async fetchPlans() {
      return {
        schemaVersion: 1,
        plans: [
          {
            planId: "1:month_price",
            name: "畅享套餐",
            price: { minorUnits: 2800, currency: "CNY" },
            billingPeriodDays: 30,
            trafficBytes: 100 * 1024 * 1024 * 1024,
          },
          {
            planId: "1:quarter_price",
            name: "畅享套餐",
            price: { minorUnits: 7600, currency: "CNY" },
            billingPeriodDays: 90,
            trafficBytes: 100 * 1024 * 1024 * 1024,
          },
          {
            planId: "2:year_price",
            name: "无限套餐",
            price: { minorUnits: 19800, currency: "CNY" },
            billingPeriodDays: 365,
            trafficBytes: null,
          },
        ],
      };
    },
    async fetchOrders() {
      return {
        schemaVersion: 1,
        orders: [
          {
            orderId: "202608010001",
            planId: "1",
            planName: "畅享套餐",
            billingPeriodDays: 90,
            status: "pending",
            amount: { minorUnits: 7600, currency: "CNY" },
            createdAtUnixMs: 1_775_174_400_000,
            paidAtUnixMs: null,
          },
          {
            orderId: "202607150018",
            planId: "2",
            planName: "无限套餐",
            billingPeriodDays: 365,
            status: "paid",
            amount: { minorUnits: 19800, currency: "CNY" },
            createdAtUnixMs: 1_773_619_200_000,
            paidAtUnixMs: 1_773_619_500_000,
          },
        ],
      };
    },
    async fetchOrderDetail(orderId) {
      const pending = orderId === "202608010001";
      return {
        schemaVersion: 1,
        order: {
          orderId,
          planId: pending ? "1" : "2",
          planName: pending ? "畅享套餐" : "无限套餐",
          billingPeriodDays: pending ? 90 : 365,
          trafficBytes: pending ? 100 * 1024 * 1024 * 1024 : null,
          status: pending ? "pending" : "paid",
          amount: {
            minorUnits: pending ? 7600 : 19800,
            currency: "CNY",
          },
          createdAtUnixMs: pending ? 1_775_174_400_000 : 1_773_619_200_000,
          updatedAtUnixMs: pending ? 1_775_174_400_000 : 1_773_619_500_000,
          paidAtUnixMs: pending ? null : 1_773_619_500_000,
        },
      };
    },
    async fetchPaymentMethods() {
      return {
        schemaVersion: 1,
        paymentMethods: [
          {
            paymentMethodId: "1",
            name: "支付宝",
            provider: "alipay",
            handlingFeePercent: "0",
          },
          {
            paymentMethodId: "2",
            name: "微信支付",
            provider: "wechat",
            handlingFeePercent: "1.5",
          },
        ],
      };
    },
    async checkoutOrder(orderId) {
      return {
        schemaVersion: 1,
        orderId,
        status: "ready",
        available: true,
        targetHost: "pay.orange.invalid",
        expiresAtUnixMs: null,
      };
    },
    async cancelOrder(orderId) {
      return {
        schemaVersion: 1,
        orderId,
        status: "cancelled",
      };
    },
    async createOrder() {
      return {
        schemaVersion: 1,
        orderId: "202608010099",
      };
    },
    async fetchInvitationCenter() {
      return {
        schemaVersion: 1,
        stats: {
          registeredUsers: 12,
          pendingCommission: { minorUnits: 3200, currency: "CNY" },
          totalCommission: { minorUnits: 18600, currency: "CNY" },
          commissionRatePercent: 30,
        },
        codes: [
          {
            code: "ORANGE8A",
            status: "available",
            views: 19,
            createdAtUnixMs: 1_775_174_400_000,
          },
          {
            code: "ORANGE5F",
            status: "used",
            views: 7,
            createdAtUnixMs: 1_773_619_200_000,
          },
        ],
      };
    },
    async generateInvitationCode() {
      return {
        schemaVersion: 1,
        stats: {
          registeredUsers: 12,
          pendingCommission: { minorUnits: 3200, currency: "CNY" },
          totalCommission: { minorUnits: 18600, currency: "CNY" },
          commissionRatePercent: 30,
        },
        codes: [
          {
            code: "ORANGENEW",
            status: "available",
            views: 0,
            createdAtUnixMs: Date.now(),
          },
          {
            code: "ORANGE8A",
            status: "available",
            views: 19,
            createdAtUnixMs: 1_775_174_400_000,
          },
        ],
      };
    },
    async fetchTickets() {
      return {
        schemaVersion: 1,
        tickets: [
          {
            ticketId: "1024",
            status: "open",
            subject: "Windows 连接后无法访问网络",
            lastMessageAtUnixMs: 1_775_174_400_000,
            closedAtUnixMs: null,
          },
          {
            ticketId: "1008",
            status: "answered",
            subject: "订阅流量显示异常",
            lastMessageAtUnixMs: 1_773_619_200_000,
            closedAtUnixMs: null,
          },
          {
            ticketId: "982",
            status: "closed",
            subject: "套餐续费咨询",
            lastMessageAtUnixMs: 1_771_027_200_000,
            closedAtUnixMs: 1_771_027_200_000,
          },
        ],
      };
    },
    async fetchTicketDetail(ticketId) {
      return {
        schemaVersion: 1,
        ticket: {
          ticketId,
          status: ticketId === "982" ? "closed" : "open",
          subject:
            ticketId === "982" ? "套餐续费咨询" : "Windows 连接后无法访问网络",
          createdAtUnixMs: 1_775_174_100_000,
          updatedAtUnixMs: 1_775_174_400_000,
          messages: [
            {
              messageId: "5001",
              fromUser: true,
              body: "连接成功后浏览器无法打开网页，请协助查看。",
              createdAtUnixMs: 1_775_174_100_000,
            },
            {
              messageId: "5002",
              fromUser: false,
              body: "已收到，请先确认系统代理模式是否开启。",
              createdAtUnixMs: 1_775_174_400_000,
            },
          ],
        },
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

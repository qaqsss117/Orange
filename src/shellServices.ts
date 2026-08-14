import { listen } from "@tauri-apps/api/event";
import type {
  AccountResponse,
  AuthPublicResponse,
  AuthSessionResponse,
  BusinessInitializationResponse,
  CancelOrderResponse,
  CreateOrderResponse,
  EmailVerificationResponse,
  InvitationCenterResponse,
  NoticesResponse,
  OrderDetailResponse,
  OrdersResponse,
  PasswordResetResponse,
  PaymentMethodsResponse,
  PaymentPublicResponse,
  PlansResponse,
  TicketDetailResponse,
  TicketsResponse,
} from "./businessApi";
import type { DataPlaneEventSnapshot } from "./events";
import {
  AuthFormError,
  cancelOrder,
  checkoutOrder,
  closeTicket,
  checkMacosPackageUpdate,
  controlDataPlane,
  createOrder,
  createTicket,
  fetchInvitationCenter,
  fetchNotices,
  fetchOrderDetail,
  fetchOrders,
  fetchPaymentMethods,
  fetchPlans,
  fetchTicketDetail,
  fetchTickets,
  generateInvitationCode,
  getConnectionMode,
  getDataPlaneEventSnapshot,
  getLaunchOnStartup,
  getNodeCatalog,
  getPlaneState,
  getProxyPort,
  getRoutingMode,
  checkGiftCard,
  fetchActiveSessions,
  fetchCommissionConfig,
  fetchGiftCardHistory,
  fetchKnowledgeDetail,
  fetchKnowledgeList,
  getRuntimeInfo,
  getServicePortalUrl,
  withdrawCommission,
  getSubscriptionSnapshot,
  initializeBusiness,
  login,
  logout,
  openLegalDocument,
  openNetworkTool,
  openServicePortal,
  openTelegramBot,
  prepareMacosPackageUpdate,
  parseCommandError,
  refreshAccount,
  refreshSubscription,
  redeemGiftCard,
  register,
  removeActiveSession,
  replyTicket,
  resetPassword,
  selectNode,
  setNodeSelectionMode,
  setProxyPort,
  transferCommission,
  sendEmailVerification,
  setConnectionMode,
  setLaunchOnStartup,
  setRoutingMode,
  testNodeDelays,
  type ConnectionMode,
  type ConnectionModeResponse,
  type DataPlaneControlAction,
  type DataPlaneControlResponse,
  type LoginFormInput,
  type LaunchOnStartupResponse,
  type LegalDocument,
  type NodeCatalogResponse,
  type NodeDelayTestResponse,
  type NodeSelectionMode,
  type NodeSelectionModeResponse,
  type NetworkTool,
  type OpenNetworkToolResponse,
  type OpenLegalDocumentResponse,
  type OpenServicePortalResponse,
  type PlaneStateResponse,
  type ProxyPortResponse,
  type RegisterFormInput,
  type ResetPasswordFormInput,
  type RuntimeInfoResponse,
  type MacosPackageUpdateResponse,
  type RoutingMode,
  type RoutingModeResponse,
  type SelectNodeResponse,
  type SubscriptionSnapshotResponse,
} from "./ipc";
import { SHELL_TEXT } from "./shellContent";

export interface ShellServices {
  initializeBusiness(): Promise<BusinessInitializationResponse>;
  openServicePortal(): Promise<OpenServicePortalResponse>;
  openTelegramBot(): Promise<OpenServicePortalResponse>;
  openNetworkTool(tool: NetworkTool): Promise<OpenNetworkToolResponse>;
  openLegalDocument(
    document: LegalDocument,
  ): Promise<OpenLegalDocumentResponse>;
  login(input: LoginFormInput): Promise<AuthPublicResponse>;
  sendEmailVerification(email: string): Promise<EmailVerificationResponse>;
  resetPassword(input: ResetPasswordFormInput): Promise<PasswordResetResponse>;
  register(input: RegisterFormInput): Promise<AuthPublicResponse>;
  logout(): Promise<AuthSessionResponse>;
  refreshAccount(): Promise<AccountResponse>;
  fetchNotices(): Promise<NoticesResponse>;
  fetchPlans(): Promise<PlansResponse>;
  fetchOrders(): Promise<OrdersResponse>;
  fetchOrderDetail(orderId: string): Promise<OrderDetailResponse>;
  fetchPaymentMethods(): Promise<PaymentMethodsResponse>;
  checkoutOrder(
    orderId: string,
    paymentMethod: string,
  ): Promise<PaymentPublicResponse>;
  cancelOrder(orderId: string): Promise<CancelOrderResponse>;
  createOrder(
    planId: string,
    couponCode?: string,
  ): Promise<CreateOrderResponse>;
  fetchInvitationCenter(): Promise<InvitationCenterResponse>;
  generateInvitationCode(): Promise<InvitationCenterResponse>;
  fetchTickets(): Promise<TicketsResponse>;
  fetchTicketDetail(ticketId: string): Promise<TicketDetailResponse>;
  createTicket(subject: string, message: string): Promise<TicketsResponse>;
  replyTicket(ticketId: string, message: string): Promise<TicketDetailResponse>;
  closeTicket(ticketId: string): Promise<TicketDetailResponse>;
  getPlaneState(): Promise<PlaneStateResponse>;
  getRuntimeInfo(): Promise<RuntimeInfoResponse>;
  checkMacosPackageUpdate(): Promise<MacosPackageUpdateResponse>;
  prepareMacosPackageUpdate(): Promise<MacosPackageUpdateResponse>;
  getDataPlaneEventSnapshot(): Promise<DataPlaneEventSnapshot>;
  controlDataPlane(
    action: DataPlaneControlAction,
  ): Promise<DataPlaneControlResponse>;
  getConnectionMode(): Promise<ConnectionModeResponse>;
  setConnectionMode(mode: ConnectionMode): Promise<ConnectionModeResponse>;
  getRoutingMode(): Promise<RoutingModeResponse>;
  setRoutingMode(mode: RoutingMode): Promise<RoutingModeResponse>;
  getProxyPort(): Promise<ProxyPortResponse>;
  setProxyPort(port: number): Promise<ProxyPortResponse>;
  getLaunchOnStartup(): Promise<LaunchOnStartupResponse>;
  setLaunchOnStartup(enabled: boolean): Promise<LaunchOnStartupResponse>;
  getSubscriptionSnapshot(): Promise<SubscriptionSnapshotResponse>;
  getServicePortalUrl(): Promise<import("./ipc").ServicePortalUrlResponse>;
  refreshSubscription(): Promise<
    import("./businessApi").SubscriptionPublicResponse
  >;
  checkGiftCard(
    code: string,
  ): Promise<import("./businessApi").GiftCardCheckResponse>;
  redeemGiftCard(
    code: string,
  ): Promise<import("./businessApi").GiftCardRedeemResponse>;
  fetchGiftCardHistory(): Promise<
    import("./businessApi").GiftCardHistoryResponse
  >;
  fetchCommissionConfig(): Promise<
    import("./businessApi").CommissionConfigResponse
  >;
  fetchActiveSessions(): Promise<
    import("./businessApi").ActiveSessionsResponse
  >;
  fetchKnowledgeList(
    keyword?: string,
  ): Promise<import("./businessApi").KnowledgeListResponse>;
  fetchKnowledgeDetail(
    articleId: string,
  ): Promise<import("./businessApi").KnowledgeDetailResponse>;
  removeActiveSession(
    sessionId: string,
  ): Promise<import("./businessApi").CommissionOperationResponse>;
  withdrawCommission(
    withdrawMethod: string,
    withdrawAccount: string,
  ): Promise<import("./businessApi").CommissionOperationResponse>;
  transferCommission(
    amountMinor: number,
  ): Promise<import("./businessApi").CommissionOperationResponse>;
  getNodeCatalog(): Promise<NodeCatalogResponse>;
  selectNode(selectorId: string, nodeId: string): Promise<SelectNodeResponse>;
  setNodeSelectionMode(
    mode: NodeSelectionMode,
  ): Promise<NodeSelectionModeResponse>;
  testNodeDelays(): Promise<NodeDelayTestResponse>;
  listenForTrayRuntimeError(
    handler: (kind: "action" | "exit") => void,
  ): Promise<() => void>;
}

export interface PublicUiError {
  message: string;
  retryable: boolean;
  field: "email" | "password" | "emailCode" | "inviteCode" | null;
}

export const nativeShellServices: ShellServices = {
  initializeBusiness,
  openLegalDocument,
  openNetworkTool,
  openServicePortal,
  openTelegramBot,
  login,
  sendEmailVerification,
  resetPassword,
  register,
  logout,
  refreshAccount,
  fetchNotices,
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
  createTicket,
  replyTicket,
  closeTicket,
  getPlaneState,
  getRuntimeInfo,
  checkMacosPackageUpdate,
  prepareMacosPackageUpdate,
  getDataPlaneEventSnapshot,
  controlDataPlane,
  getConnectionMode,
  setConnectionMode,
  getRoutingMode,
  setRoutingMode,
  getProxyPort,
  setProxyPort,
  getLaunchOnStartup,
  setLaunchOnStartup,
  getSubscriptionSnapshot,
  getServicePortalUrl,
  refreshSubscription,
  checkGiftCard,
  redeemGiftCard,
  fetchGiftCardHistory,
  fetchCommissionConfig,
  withdrawCommission,
  transferCommission,
  fetchActiveSessions,
  removeActiveSession,
  fetchKnowledgeList,
  fetchKnowledgeDetail,
  getNodeCatalog,
  selectNode,
  setNodeSelectionMode,
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
  emailCode: SHELL_TEXT.emailCodeInvalid,
  inviteCode: SHELL_TEXT.inviteInvalid,
} as const;

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

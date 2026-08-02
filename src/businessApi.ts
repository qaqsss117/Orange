export const BUSINESS_API_SCHEMA_VERSION = 1 as const;
export const MAX_BUSINESS_API_INTEGER = Number.MAX_SAFE_INTEGER;
export const MAX_BUSINESS_API_ITEMS = 256;
export const MAX_BUSINESS_API_NOTICES = 64;

export const BUSINESS_API_OPERATIONS = [
  "config",
  "login",
  "emailVerification",
  "register",
  "account",
  "notices",
  "subscription",
  "plans",
  "orders",
  "payment",
  "invite",
  "tickets",
  "update",
] as const;

export const ACCOUNT_STATUSES = ["active", "disabled"] as const;
export const SUBSCRIPTION_STATUSES = [
  "none",
  "trial",
  "active",
  "expired",
  "exhausted",
] as const;
export const ORDER_STATUSES = [
  "pending",
  "paid",
  "cancelled",
  "closed",
  "refunded",
] as const;
export const PAYMENT_STATUSES = [
  "unavailable",
  "pending",
  "ready",
  "expired",
] as const;
export const INVITATION_CODE_STATUSES = [
  "available",
  "used",
  "disabled",
] as const;
export const TICKET_STATUSES = ["open", "answered", "closed"] as const;
export const AUTH_SESSION_STATUSES = [
  "signed_out",
  "authenticated",
  "unverified",
] as const;

export type BusinessApiOperation = (typeof BUSINESS_API_OPERATIONS)[number];
export type AccountStatus = (typeof ACCOUNT_STATUSES)[number] | "unknown";
export type SubscriptionStatus =
  (typeof SUBSCRIPTION_STATUSES)[number] | "unknown";
export type OrderStatus = (typeof ORDER_STATUSES)[number] | "unknown";
export type PaymentStatus = (typeof PAYMENT_STATUSES)[number] | "unknown";
export type InvitationCodeStatus =
  (typeof INVITATION_CODE_STATUSES)[number] | "unknown";
export type TicketStatus = (typeof TICKET_STATUSES)[number] | "unknown";
export type AuthSessionStatus = (typeof AUTH_SESSION_STATUSES)[number];

export interface Money {
  minorUnits: number;
  currency: string;
}

export interface UserProfile {
  userId: string;
  email: string;
  status: AccountStatus;
  balance: Money;
}

export interface AuthPublicResponse {
  schemaVersion: typeof BUSINESS_API_SCHEMA_VERSION;
  authenticated: boolean;
  user: UserProfile;
}

export interface EmailVerificationResponse {
  schemaVersion: typeof BUSINESS_API_SCHEMA_VERSION;
  sent: boolean;
}

export interface PasswordResetResponse {
  schemaVersion: typeof BUSINESS_API_SCHEMA_VERSION;
  succeeded: boolean;
}

export interface AuthSessionResponse {
  schemaVersion: typeof BUSINESS_API_SCHEMA_VERSION;
  status: AuthSessionStatus;
  user: UserProfile | null;
}

export interface ConfigResponse {
  schemaVersion: typeof BUSINESS_API_SCHEMA_VERSION;
  minimumSupportedVersion: string;
  maintenance: boolean;
  notice: string | null;
  registrationRequiresInvite: boolean;
  registrationRequiresEmailVerification: boolean;
}

export interface BusinessInitializationResponse {
  schemaVersion: typeof BUSINESS_API_SCHEMA_VERSION;
  config: ConfigResponse;
  session: AuthSessionResponse;
}

export interface AccountResponse {
  schemaVersion: typeof BUSINESS_API_SCHEMA_VERSION;
  user: UserProfile;
}

export interface Notice {
  title: string;
  content: string;
}

export interface NoticesResponse {
  schemaVersion: typeof BUSINESS_API_SCHEMA_VERSION;
  notices: Notice[];
}

export interface SubscriptionPublicResponse {
  schemaVersion: typeof BUSINESS_API_SCHEMA_VERSION;
  status: SubscriptionStatus;
  planId: string | null;
  expiresAtUnixMs: number | null;
  usedBytes: number;
  uploadBytes: number | null;
  downloadBytes: number | null;
  totalBytes: number | null;
}

export interface SubscriptionLinkResponse {
  schemaVersion: typeof BUSINESS_API_SCHEMA_VERSION;
  subscribeUrl: string;
}

export interface KnowledgeArticleSummary {
  articleId: string;
  title: string;
  updatedAtUnixMs: number | null;
}

export interface KnowledgeGroup {
  category: string;
  articles: KnowledgeArticleSummary[];
}

export interface KnowledgeListResponse {
  schemaVersion: typeof BUSINESS_API_SCHEMA_VERSION;
  groups: KnowledgeGroup[];
}

export interface KnowledgeDetailResponse {
  schemaVersion: typeof BUSINESS_API_SCHEMA_VERSION;
  articleId: string;
  category: string | null;
  title: string;
  bodyHtml: string;
  updatedAtUnixMs: number | null;
}

export interface ActiveSessionInfo {
  sessionId: string;
  name: string | null;
  lastUsedAt: string | null;
  createdAt: string | null;
}

export interface ActiveSessionsResponse {
  schemaVersion: typeof BUSINESS_API_SCHEMA_VERSION;
  sessions: ActiveSessionInfo[];
}

export interface CommissionConfigResponse {
  schemaVersion: typeof BUSINESS_API_SCHEMA_VERSION;
  withdrawMethods: string[];
  withdrawClosed: boolean;
  telegramDiscussLink: string | null;
}

export interface CommissionOperationResponse {
  schemaVersion: typeof BUSINESS_API_SCHEMA_VERSION;
  succeeded: boolean;
}

export interface GiftCardCheckResponse {
  schemaVersion: typeof BUSINESS_API_SCHEMA_VERSION;
  canRedeem: boolean;
  reason: string | null;
  cardName: string | null;
  rewardPreviewJson: string | null;
}

export interface GiftCardRedeemResponse {
  schemaVersion: typeof BUSINESS_API_SCHEMA_VERSION;
  message: string;
  templateName: string | null;
  rewardsJson: string | null;
}

export interface GiftCardHistoryRecord {
  recordId: string;
  code: string;
  templateName: string | null;
  templateTypeName: string | null;
  createdAt: string | null;
}

export interface GiftCardHistoryResponse {
  schemaVersion: typeof BUSINESS_API_SCHEMA_VERSION;
  records: GiftCardHistoryRecord[];
  total: number;
}

export interface Plan {
  planId: string;
  name: string;
  price: Money;
  billingPeriodDays: number;
  trafficBytes: number | null;
}

export interface PlansResponse {
  schemaVersion: typeof BUSINESS_API_SCHEMA_VERSION;
  plans: Plan[];
}

export interface CreateOrderResponse {
  schemaVersion: typeof BUSINESS_API_SCHEMA_VERSION;
  orderId: string;
}

export interface Order {
  orderId: string;
  planId: string;
  status: OrderStatus;
  amount: Money;
  createdAtUnixMs: number;
  paidAtUnixMs: number | null;
}

export interface OrderResponse {
  schemaVersion: typeof BUSINESS_API_SCHEMA_VERSION;
  order: Order;
}

export interface OrderSummary {
  orderId: string;
  planId: string;
  planName: string;
  billingPeriodDays: number | null;
  status: OrderStatus;
  amount: Money;
  createdAtUnixMs: number;
  paidAtUnixMs: number | null;
}

export interface OrdersResponse {
  schemaVersion: typeof BUSINESS_API_SCHEMA_VERSION;
  orders: OrderSummary[];
}

export interface OrderDetail {
  orderId: string;
  planId: string;
  planName: string;
  billingPeriodDays: number | null;
  trafficBytes: number | null;
  status: OrderStatus;
  amount: Money;
  createdAtUnixMs: number;
  updatedAtUnixMs: number | null;
  paidAtUnixMs: number | null;
}

export interface OrderDetailResponse {
  schemaVersion: typeof BUSINESS_API_SCHEMA_VERSION;
  order: OrderDetail;
}

export interface CancelOrderResponse {
  schemaVersion: typeof BUSINESS_API_SCHEMA_VERSION;
  orderId: string;
  status: OrderStatus;
}

export interface PaymentMethod {
  paymentMethodId: string;
  name: string;
  provider: string;
  handlingFeePercent: string;
}

export interface PaymentMethodsResponse {
  schemaVersion: typeof BUSINESS_API_SCHEMA_VERSION;
  paymentMethods: PaymentMethod[];
}

export interface PaymentPublicResponse {
  schemaVersion: typeof BUSINESS_API_SCHEMA_VERSION;
  orderId: string;
  status: PaymentStatus;
  available: boolean;
  targetHost: string | null;
  expiresAtUnixMs: number | null;
}

export interface InvitationCode {
  code: string;
  status: InvitationCodeStatus;
  views: number;
  createdAtUnixMs: number | null;
}

export interface InvitationStats {
  registeredUsers: number;
  pendingCommission: Money;
  totalCommission: Money;
  commissionRatePercent: number;
}

export interface InvitationCenterResponse {
  schemaVersion: typeof BUSINESS_API_SCHEMA_VERSION;
  stats: InvitationStats;
  codes: InvitationCode[];
}

export interface Ticket {
  ticketId: string;
  status: TicketStatus;
  subject: string;
  lastMessageAtUnixMs: number;
  closedAtUnixMs: number | null;
}

export interface TicketsResponse {
  schemaVersion: typeof BUSINESS_API_SCHEMA_VERSION;
  tickets: Ticket[];
}

export interface TicketMessage {
  messageId: string;
  fromUser: boolean;
  body: string;
  createdAtUnixMs: number;
}

export interface TicketDetail {
  ticketId: string;
  status: TicketStatus;
  subject: string;
  createdAtUnixMs: number;
  updatedAtUnixMs: number;
  messages: TicketMessage[];
}

export interface TicketDetailResponse {
  schemaVersion: typeof BUSINESS_API_SCHEMA_VERSION;
  ticket: TicketDetail;
}

export interface UpdateResponse {
  schemaVersion: typeof BUSINESS_API_SCHEMA_VERSION;
  latestVersion: string;
  mandatory: boolean;
  releaseNotes: string | null;
}

const CONTRACT_ERROR = "Business API public contract violation";
type JsonObject = Record<string, unknown>;

function isObject(value: unknown): value is JsonObject {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function parseObject(value: unknown, keys: readonly string[]): JsonObject {
  if (!isObject(value)) {
    throw new Error(CONTRACT_ERROR);
  }
  const actualKeys = Object.keys(value);
  if (
    actualKeys.length !== keys.length ||
    !actualKeys.every((key) => keys.includes(key))
  ) {
    throw new Error(CONTRACT_ERROR);
  }
  return value;
}

function parseString(value: unknown, pattern: RegExp | null = null): string {
  const text = parseText(value);
  if (text.length === 0 || (pattern !== null && !pattern.test(text))) {
    throw new Error(CONTRACT_ERROR);
  }
  return text;
}

function parseText(value: unknown): string {
  if (typeof value !== "string") {
    throw new Error(CONTRACT_ERROR);
  }
  return value;
}

function parseNullable<T>(
  value: unknown,
  parser: (candidate: unknown) => T,
): T | null {
  return value === null ? null : parser(value);
}

function parseSafeInteger(value: unknown): number {
  if (
    typeof value !== "number" ||
    !Number.isSafeInteger(value) ||
    value < 0 ||
    value > MAX_BUSINESS_API_INTEGER
  ) {
    throw new Error(CONTRACT_ERROR);
  }
  return value;
}

function parseSchemaVersion(
  value: unknown,
): typeof BUSINESS_API_SCHEMA_VERSION {
  if (value !== BUSINESS_API_SCHEMA_VERSION) {
    throw new Error(CONTRACT_ERROR);
  }
  return BUSINESS_API_SCHEMA_VERSION;
}

function parseBoolean(value: unknown): boolean {
  if (typeof value !== "boolean") {
    throw new Error(CONTRACT_ERROR);
  }
  return value;
}

function parseStatus<T extends string>(
  value: unknown,
  knownValues: readonly T[],
): T | "unknown" {
  if (typeof value !== "string") {
    throw new Error(CONTRACT_ERROR);
  }
  return knownValues.includes(value as T) ? (value as T) : "unknown";
}

function parseItems<T>(value: unknown, parser: (candidate: unknown) => T): T[] {
  if (!Array.isArray(value) || value.length > MAX_BUSINESS_API_ITEMS) {
    throw new Error(CONTRACT_ERROR);
  }
  return value.map(parser);
}

function parseMoney(value: unknown): Money {
  const object = parseObject(value, ["minorUnits", "currency"]);
  return {
    minorUnits: parseSafeInteger(object.minorUnits),
    currency: parseString(object.currency, /^[A-Z]{3}$/),
  };
}

function parseUserProfile(value: unknown): UserProfile {
  const object = parseObject(value, ["userId", "email", "status", "balance"]);
  const email = parseString(object.email);
  if (email.length < 3) {
    throw new Error(CONTRACT_ERROR);
  }
  return {
    userId: parseString(object.userId),
    email,
    status: parseStatus(object.status, ACCOUNT_STATUSES),
    balance: parseMoney(object.balance),
  };
}

export function parseAuthPublicResponse(value: unknown): AuthPublicResponse {
  const object = parseObject(value, ["schemaVersion", "authenticated", "user"]);
  return {
    schemaVersion: parseSchemaVersion(object.schemaVersion),
    authenticated: parseBoolean(object.authenticated),
    user: parseUserProfile(object.user),
  };
}

export function parseEmailVerificationResponse(
  value: unknown,
): EmailVerificationResponse {
  const object = parseObject(value, ["schemaVersion", "sent"]);
  return {
    schemaVersion: parseSchemaVersion(object.schemaVersion),
    sent: parseBoolean(object.sent),
  };
}

export function parsePasswordResetResponse(
  value: unknown,
): PasswordResetResponse {
  const object = parseObject(value, ["schemaVersion", "succeeded"]);
  return {
    schemaVersion: parseSchemaVersion(object.schemaVersion),
    succeeded: parseBoolean(object.succeeded),
  };
}

export function parseConfigResponse(value: unknown): ConfigResponse {
  const object = parseObject(value, [
    "schemaVersion",
    "minimumSupportedVersion",
    "maintenance",
    "notice",
    "registrationRequiresInvite",
    "registrationRequiresEmailVerification",
  ]);
  return {
    schemaVersion: parseSchemaVersion(object.schemaVersion),
    minimumSupportedVersion: parseString(object.minimumSupportedVersion),
    maintenance: parseBoolean(object.maintenance),
    notice: parseNullable(object.notice, parseText),
    registrationRequiresInvite: parseBoolean(object.registrationRequiresInvite),
    registrationRequiresEmailVerification: parseBoolean(
      object.registrationRequiresEmailVerification,
    ),
  };
}

export function parseAuthSessionResponse(value: unknown): AuthSessionResponse {
  const object = parseObject(value, ["schemaVersion", "status", "user"]);
  const status = parseStatus(object.status, AUTH_SESSION_STATUSES);
  if (status === "unknown") {
    throw new Error(CONTRACT_ERROR);
  }
  const user = parseNullable(object.user, parseUserProfile);
  if (
    (status === "authenticated" && user === null) ||
    (status === "signed_out" && user !== null)
  ) {
    throw new Error(CONTRACT_ERROR);
  }
  return {
    schemaVersion: parseSchemaVersion(object.schemaVersion),
    status,
    user,
  };
}

export function parseBusinessInitializationResponse(
  value: unknown,
): BusinessInitializationResponse {
  const object = parseObject(value, ["schemaVersion", "config", "session"]);
  return {
    schemaVersion: parseSchemaVersion(object.schemaVersion),
    config: parseConfigResponse(object.config),
    session: parseAuthSessionResponse(object.session),
  };
}

export function parseAccountResponse(value: unknown): AccountResponse {
  const object = parseObject(value, ["schemaVersion", "user"]);
  return {
    schemaVersion: parseSchemaVersion(object.schemaVersion),
    user: parseUserProfile(object.user),
  };
}

function parseNotice(value: unknown): Notice {
  const object = parseObject(value, ["title", "content"]);
  const title = parseString(object.title);
  const content = parseString(object.content);
  const encoder = new TextEncoder();
  if (
    title.trim() !== title ||
    encoder.encode(title).length > 512 ||
    /[\u0000-\u001f\u007f]/.test(title) ||
    content.trim() !== content ||
    encoder.encode(content).length > 64 * 1024 ||
    /[\u0000-\u0008\u000b\u000c\u000e-\u001f\u007f]/.test(content)
  ) {
    throw new Error(CONTRACT_ERROR);
  }
  return { title, content };
}

export function parseNoticesResponse(value: unknown): NoticesResponse {
  const object = parseObject(value, ["schemaVersion", "notices"]);
  if (
    !Array.isArray(object.notices) ||
    object.notices.length > MAX_BUSINESS_API_NOTICES
  ) {
    throw new Error(CONTRACT_ERROR);
  }
  return {
    schemaVersion: parseSchemaVersion(object.schemaVersion),
    notices: object.notices.map(parseNotice),
  };
}

function parseKnowledgeArticleSummary(
  value: unknown,
): KnowledgeArticleSummary {
  const object = parseObject(value, ["articleId", "title", "updatedAtUnixMs"]);
  return {
    articleId: parseText(object.articleId),
    title: parseText(object.title),
    updatedAtUnixMs: parseNullable(object.updatedAtUnixMs, parseSafeInteger),
  };
}

function parseKnowledgeGroup(value: unknown): KnowledgeGroup {
  const object = parseObject(value, ["category", "articles"]);
  if (!Array.isArray(object.articles)) {
    throw new Error(CONTRACT_ERROR);
  }
  return {
    category: parseText(object.category),
    articles: object.articles.map(parseKnowledgeArticleSummary),
  };
}

export function parseKnowledgeListResponse(
  value: unknown,
): KnowledgeListResponse {
  const object = parseObject(value, ["schemaVersion", "groups"]);
  if (!Array.isArray(object.groups)) {
    throw new Error(CONTRACT_ERROR);
  }
  return {
    schemaVersion: parseSchemaVersion(object.schemaVersion),
    groups: object.groups.map(parseKnowledgeGroup),
  };
}

export function parseKnowledgeDetailResponse(
  value: unknown,
): KnowledgeDetailResponse {
  const object = parseObject(value, [
    "schemaVersion",
    "articleId",
    "category",
    "title",
    "bodyHtml",
    "updatedAtUnixMs",
  ]);
  return {
    schemaVersion: parseSchemaVersion(object.schemaVersion),
    articleId: parseText(object.articleId),
    category: parseNullable(object.category, parseText),
    title: parseText(object.title),
    bodyHtml: parseText(object.bodyHtml),
    updatedAtUnixMs: parseNullable(object.updatedAtUnixMs, parseSafeInteger),
  };
}

function parseActiveSessionInfo(value: unknown): ActiveSessionInfo {
  const object = parseObject(value, [
    "sessionId",
    "name",
    "lastUsedAt",
    "createdAt",
  ]);
  return {
    sessionId: parseText(object.sessionId),
    name: parseNullable(object.name, parseText),
    lastUsedAt: parseNullable(object.lastUsedAt, parseText),
    createdAt: parseNullable(object.createdAt, parseText),
  };
}

export function parseActiveSessionsResponse(
  value: unknown,
): ActiveSessionsResponse {
  const object = parseObject(value, ["schemaVersion", "sessions"]);
  if (!Array.isArray(object.sessions)) {
    throw new Error(CONTRACT_ERROR);
  }
  return {
    schemaVersion: parseSchemaVersion(object.schemaVersion),
    sessions: object.sessions.map(parseActiveSessionInfo),
  };
}

export function parseCommissionConfigResponse(
  value: unknown,
): CommissionConfigResponse {
  const object = parseObject(value, [
    "schemaVersion",
    "withdrawMethods",
    "withdrawClosed",
    "telegramDiscussLink",
  ]);
  if (!Array.isArray(object.withdrawMethods)) {
    throw new Error(CONTRACT_ERROR);
  }
  return {
    schemaVersion: parseSchemaVersion(object.schemaVersion),
    withdrawMethods: object.withdrawMethods.map(parseText),
    withdrawClosed: object.withdrawClosed === true,
    telegramDiscussLink: parseNullable(object.telegramDiscussLink, parseText),
  };
}

export function parseCommissionOperationResponse(
  value: unknown,
): CommissionOperationResponse {
  const object = parseObject(value, ["schemaVersion", "succeeded"]);
  if (object.succeeded !== true) {
    throw new Error(CONTRACT_ERROR);
  }
  return {
    schemaVersion: parseSchemaVersion(object.schemaVersion),
    succeeded: true,
  };
}

export function parseGiftCardCheckResponse(
  value: unknown,
): GiftCardCheckResponse {
  const object = parseObject(value, [
    "schemaVersion",
    "canRedeem",
    "reason",
    "cardName",
    "rewardPreviewJson",
  ]);
  return {
    schemaVersion: parseSchemaVersion(object.schemaVersion),
    canRedeem: object.canRedeem === true,
    reason: parseNullable(object.reason, parseText),
    cardName: parseNullable(object.cardName, parseText),
    rewardPreviewJson: parseNullable(object.rewardPreviewJson, parseText),
  };
}

export function parseGiftCardRedeemResponse(
  value: unknown,
): GiftCardRedeemResponse {
  const object = parseObject(value, [
    "schemaVersion",
    "message",
    "templateName",
    "rewardsJson",
  ]);
  return {
    schemaVersion: parseSchemaVersion(object.schemaVersion),
    message: parseText(object.message),
    templateName: parseNullable(object.templateName, parseText),
    rewardsJson: parseNullable(object.rewardsJson, parseText),
  };
}

function parseGiftCardHistoryRecord(value: unknown): GiftCardHistoryRecord {
  const object = parseObject(value, [
    "recordId",
    "code",
    "templateName",
    "templateTypeName",
    "createdAt",
  ]);
  return {
    recordId: parseText(object.recordId),
    code: parseText(object.code),
    templateName: parseNullable(object.templateName, parseText),
    templateTypeName: parseNullable(object.templateTypeName, parseText),
    createdAt: parseNullable(object.createdAt, parseText),
  };
}

export function parseGiftCardHistoryResponse(
  value: unknown,
): GiftCardHistoryResponse {
  const object = parseObject(value, ["schemaVersion", "records", "total"]);
  if (!Array.isArray(object.records)) {
    throw new Error(CONTRACT_ERROR);
  }
  return {
    schemaVersion: parseSchemaVersion(object.schemaVersion),
    records: object.records.map(parseGiftCardHistoryRecord),
    total: parseSafeInteger(object.total),
  };
}

export function parseSubscriptionLinkResponse(
  value: unknown,
): SubscriptionLinkResponse {
  const object = parseObject(value, ["schemaVersion", "subscribeUrl"]);
  return {
    schemaVersion: parseSchemaVersion(object.schemaVersion),
    subscribeUrl: parseText(object.subscribeUrl),
  };
}

export function parseSubscriptionResponse(
  value: unknown,
): SubscriptionPublicResponse {
  const object = parseObject(value, [
    "schemaVersion",
    "status",
    "planId",
    "expiresAtUnixMs",
    "usedBytes",
    "uploadBytes",
    "downloadBytes",
    "totalBytes",
  ]);
  return {
    schemaVersion: parseSchemaVersion(object.schemaVersion),
    status: parseStatus(object.status, SUBSCRIPTION_STATUSES),
    planId: parseNullable(object.planId, parseText),
    expiresAtUnixMs: parseNullable(object.expiresAtUnixMs, parseSafeInteger),
    usedBytes: parseSafeInteger(object.usedBytes),
    uploadBytes: parseNullable(object.uploadBytes, parseSafeInteger),
    downloadBytes: parseNullable(object.downloadBytes, parseSafeInteger),
    totalBytes: parseNullable(object.totalBytes, parseSafeInteger),
  };
}

function parsePlan(value: unknown): Plan {
  const object = parseObject(value, [
    "planId",
    "name",
    "price",
    "billingPeriodDays",
    "trafficBytes",
  ]);
  return {
    planId: parseString(object.planId),
    name: parseString(object.name),
    price: parseMoney(object.price),
    billingPeriodDays: parseSafeInteger(object.billingPeriodDays),
    trafficBytes: parseNullable(object.trafficBytes, parseSafeInteger),
  };
}

export function parsePlansResponse(value: unknown): PlansResponse {
  const object = parseObject(value, ["schemaVersion", "plans"]);
  return {
    schemaVersion: parseSchemaVersion(object.schemaVersion),
    plans: parseItems(object.plans, parsePlan),
  };
}

export function parseCreateOrderResponse(value: unknown): CreateOrderResponse {
  const object = parseObject(value, ["schemaVersion", "orderId"]);
  return {
    schemaVersion: parseSchemaVersion(object.schemaVersion),
    orderId: parseString(object.orderId),
  };
}

function parseOrder(value: unknown): Order {
  const object = parseObject(value, [
    "orderId",
    "planId",
    "status",
    "amount",
    "createdAtUnixMs",
    "paidAtUnixMs",
  ]);
  return {
    orderId: parseString(object.orderId),
    planId: parseString(object.planId),
    status: parseStatus(object.status, ORDER_STATUSES),
    amount: parseMoney(object.amount),
    createdAtUnixMs: parseSafeInteger(object.createdAtUnixMs),
    paidAtUnixMs: parseNullable(object.paidAtUnixMs, parseSafeInteger),
  };
}

export function parseOrderResponse(value: unknown): OrderResponse {
  const object = parseObject(value, ["schemaVersion", "order"]);
  return {
    schemaVersion: parseSchemaVersion(object.schemaVersion),
    order: parseOrder(object.order),
  };
}

function parseOrderSummary(value: unknown): OrderSummary {
  const object = parseObject(value, [
    "orderId",
    "planId",
    "planName",
    "billingPeriodDays",
    "status",
    "amount",
    "createdAtUnixMs",
    "paidAtUnixMs",
  ]);
  return {
    orderId: parseString(object.orderId),
    planId: parseString(object.planId),
    planName: parseString(object.planName),
    billingPeriodDays: parseNullable(
      object.billingPeriodDays,
      parseSafeInteger,
    ),
    status: parseStatus(object.status, ORDER_STATUSES),
    amount: parseMoney(object.amount),
    createdAtUnixMs: parseSafeInteger(object.createdAtUnixMs),
    paidAtUnixMs: parseNullable(object.paidAtUnixMs, parseSafeInteger),
  };
}

export function parseOrdersResponse(value: unknown): OrdersResponse {
  const object = parseObject(value, ["schemaVersion", "orders"]);
  return {
    schemaVersion: parseSchemaVersion(object.schemaVersion),
    orders: parseItems(object.orders, parseOrderSummary),
  };
}

function parseOrderDetail(value: unknown): OrderDetail {
  const object = parseObject(value, [
    "orderId",
    "planId",
    "planName",
    "billingPeriodDays",
    "trafficBytes",
    "status",
    "amount",
    "createdAtUnixMs",
    "updatedAtUnixMs",
    "paidAtUnixMs",
  ]);
  return {
    orderId: parseString(object.orderId),
    planId: parseString(object.planId),
    planName: parseString(object.planName),
    billingPeriodDays: parseNullable(
      object.billingPeriodDays,
      parseSafeInteger,
    ),
    trafficBytes: parseNullable(object.trafficBytes, parseSafeInteger),
    status: parseStatus(object.status, ORDER_STATUSES),
    amount: parseMoney(object.amount),
    createdAtUnixMs: parseSafeInteger(object.createdAtUnixMs),
    updatedAtUnixMs: parseNullable(object.updatedAtUnixMs, parseSafeInteger),
    paidAtUnixMs: parseNullable(object.paidAtUnixMs, parseSafeInteger),
  };
}

export function parseOrderDetailResponse(value: unknown): OrderDetailResponse {
  const object = parseObject(value, ["schemaVersion", "order"]);
  return {
    schemaVersion: parseSchemaVersion(object.schemaVersion),
    order: parseOrderDetail(object.order),
  };
}

export function parseCancelOrderResponse(value: unknown): CancelOrderResponse {
  const object = parseObject(value, ["schemaVersion", "orderId", "status"]);
  return {
    schemaVersion: parseSchemaVersion(object.schemaVersion),
    orderId: parseString(object.orderId),
    status: parseStatus(object.status, ORDER_STATUSES),
  };
}

function parsePaymentMethod(value: unknown): PaymentMethod {
  const object = parseObject(value, [
    "paymentMethodId",
    "name",
    "provider",
    "handlingFeePercent",
  ]);
  return {
    paymentMethodId: parseString(object.paymentMethodId, /^[1-9][0-9]*$/),
    name: parseString(object.name),
    provider: parseString(object.provider, /^[A-Za-z0-9._-]+$/),
    handlingFeePercent: parseString(
      object.handlingFeePercent,
      /^\d+(?:\.\d{1,6})?$/,
    ),
  };
}

export function parsePaymentMethodsResponse(
  value: unknown,
): PaymentMethodsResponse {
  const object = parseObject(value, ["schemaVersion", "paymentMethods"]);
  return {
    schemaVersion: parseSchemaVersion(object.schemaVersion),
    paymentMethods: parseItems(object.paymentMethods, parsePaymentMethod),
  };
}

export function parsePaymentResponse(value: unknown): PaymentPublicResponse {
  const object = parseObject(value, [
    "schemaVersion",
    "orderId",
    "status",
    "available",
    "targetHost",
    "expiresAtUnixMs",
  ]);
  return {
    schemaVersion: parseSchemaVersion(object.schemaVersion),
    orderId: parseString(object.orderId),
    status: parseStatus(object.status, PAYMENT_STATUSES),
    available: parseBoolean(object.available),
    targetHost: parseNullable(object.targetHost, (candidate) =>
      parseString(candidate, /^[a-z0-9.-]+$/),
    ),
    expiresAtUnixMs: parseNullable(object.expiresAtUnixMs, parseSafeInteger),
  };
}

function parseInvitationCode(value: unknown): InvitationCode {
  const object = parseObject(value, [
    "code",
    "status",
    "views",
    "createdAtUnixMs",
  ]);
  return {
    code: parseString(object.code, /^[A-Za-z0-9_-]{1,64}$/),
    status: parseStatus(object.status, INVITATION_CODE_STATUSES),
    views: parseSafeInteger(object.views),
    createdAtUnixMs: parseNullable(object.createdAtUnixMs, parseSafeInteger),
  };
}

function parseInvitationStats(value: unknown): InvitationStats {
  const object = parseObject(value, [
    "registeredUsers",
    "pendingCommission",
    "totalCommission",
    "commissionRatePercent",
  ]);
  return {
    registeredUsers: parseSafeInteger(object.registeredUsers),
    pendingCommission: parseMoney(object.pendingCommission),
    totalCommission: parseMoney(object.totalCommission),
    commissionRatePercent: parseSafeInteger(object.commissionRatePercent),
  };
}

export function parseInvitationCenterResponse(
  value: unknown,
): InvitationCenterResponse {
  const object = parseObject(value, ["schemaVersion", "stats", "codes"]);
  return {
    schemaVersion: parseSchemaVersion(object.schemaVersion),
    stats: parseInvitationStats(object.stats),
    codes: parseItems(object.codes, parseInvitationCode),
  };
}

function parseTicket(value: unknown): Ticket {
  const object = parseObject(value, [
    "ticketId",
    "status",
    "subject",
    "lastMessageAtUnixMs",
    "closedAtUnixMs",
  ]);
  return {
    ticketId: parseString(object.ticketId),
    status: parseStatus(object.status, TICKET_STATUSES),
    subject: parseString(object.subject),
    lastMessageAtUnixMs: parseSafeInteger(object.lastMessageAtUnixMs),
    closedAtUnixMs: parseNullable(object.closedAtUnixMs, parseSafeInteger),
  };
}

export function parseTicketsResponse(value: unknown): TicketsResponse {
  const object = parseObject(value, ["schemaVersion", "tickets"]);
  return {
    schemaVersion: parseSchemaVersion(object.schemaVersion),
    tickets: parseItems(object.tickets, parseTicket),
  };
}

function parseTicketText(value: unknown, maximumLength: number): string {
  const text = parseString(value);
  if (
    text.length > maximumLength ||
    /[\u0000-\u0008\u000b\u000c\u000e-\u001f\u007f]/.test(text)
  ) {
    throw new Error(CONTRACT_ERROR);
  }
  return text;
}

function parseTicketMessage(value: unknown): TicketMessage {
  const object = parseObject(value, [
    "messageId",
    "fromUser",
    "body",
    "createdAtUnixMs",
  ]);
  return {
    messageId: parseString(object.messageId, /^[1-9][0-9]{0,19}$/),
    fromUser: parseBoolean(object.fromUser),
    body: parseTicketText(object.body, 64 * 1024),
    createdAtUnixMs: parseSafeInteger(object.createdAtUnixMs),
  };
}

function parseTicketDetail(value: unknown): TicketDetail {
  const object = parseObject(value, [
    "ticketId",
    "status",
    "subject",
    "createdAtUnixMs",
    "updatedAtUnixMs",
    "messages",
  ]);
  return {
    ticketId: parseString(object.ticketId, /^[1-9][0-9]{0,19}$/),
    status: parseStatus(object.status, TICKET_STATUSES),
    subject: parseTicketText(object.subject, 512),
    createdAtUnixMs: parseSafeInteger(object.createdAtUnixMs),
    updatedAtUnixMs: parseSafeInteger(object.updatedAtUnixMs),
    messages: parseItems(object.messages, parseTicketMessage),
  };
}

export function parseTicketDetailResponse(
  value: unknown,
): TicketDetailResponse {
  const object = parseObject(value, ["schemaVersion", "ticket"]);
  return {
    schemaVersion: parseSchemaVersion(object.schemaVersion),
    ticket: parseTicketDetail(object.ticket),
  };
}

export function parseUpdateResponse(value: unknown): UpdateResponse {
  const object = parseObject(value, [
    "schemaVersion",
    "latestVersion",
    "mandatory",
    "releaseNotes",
  ]);
  return {
    schemaVersion: parseSchemaVersion(object.schemaVersion),
    latestVersion: parseString(object.latestVersion),
    mandatory: parseBoolean(object.mandatory),
    releaseNotes: parseNullable(object.releaseNotes, parseText),
  };
}

export const BUSINESS_API_SCHEMA_VERSION = 1 as const;
export const MAX_BUSINESS_API_INTEGER = Number.MAX_SAFE_INTEGER;
export const MAX_BUSINESS_API_ITEMS = 256;

export const BUSINESS_API_OPERATIONS = [
  "config",
  "login",
  "register",
  "account",
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
export const TICKET_STATUSES = ["open", "answered", "closed"] as const;

export type BusinessApiOperation = (typeof BUSINESS_API_OPERATIONS)[number];
export type AccountStatus = (typeof ACCOUNT_STATUSES)[number] | "unknown";
export type SubscriptionStatus =
  (typeof SUBSCRIPTION_STATUSES)[number] | "unknown";
export type OrderStatus = (typeof ORDER_STATUSES)[number] | "unknown";
export type PaymentStatus = (typeof PAYMENT_STATUSES)[number] | "unknown";
export type TicketStatus = (typeof TICKET_STATUSES)[number] | "unknown";

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

export interface ConfigResponse {
  schemaVersion: typeof BUSINESS_API_SCHEMA_VERSION;
  minimumSupportedVersion: string;
  maintenance: boolean;
  notice: string | null;
}

export interface AccountResponse {
  schemaVersion: typeof BUSINESS_API_SCHEMA_VERSION;
  user: UserProfile;
}

export interface SubscriptionPublicResponse {
  schemaVersion: typeof BUSINESS_API_SCHEMA_VERSION;
  status: SubscriptionStatus;
  planId: string | null;
  expiresAtUnixMs: number | null;
  usedBytes: number;
  totalBytes: number | null;
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

export interface PaymentPublicResponse {
  schemaVersion: typeof BUSINESS_API_SCHEMA_VERSION;
  orderId: string;
  status: PaymentStatus;
  available: boolean;
  targetHost: string | null;
  expiresAtUnixMs: number | null;
}

export interface InviteResponse {
  schemaVersion: typeof BUSINESS_API_SCHEMA_VERSION;
  inviteCode: string;
  invitedUsers: number;
  commission: Money;
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

export interface UpdateResponse {
  schemaVersion: typeof BUSINESS_API_SCHEMA_VERSION;
  latestVersion: string;
  mandatory: boolean;
  releaseNotes: string | null;
}

export interface BusinessApiPublicResponses {
  config: ConfigResponse;
  login: AuthPublicResponse;
  register: AuthPublicResponse;
  account: AccountResponse;
  subscription: SubscriptionPublicResponse;
  plans: PlansResponse;
  orders: OrderResponse;
  payment: PaymentPublicResponse;
  invite: InviteResponse;
  tickets: TicketsResponse;
  update: UpdateResponse;
}

export interface BusinessApiPublicFixture {
  schemaVersion: typeof BUSINESS_API_SCHEMA_VERSION;
  environment: "development";
  responses: BusinessApiPublicResponses;
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

function parseAuthResponse(value: unknown): AuthPublicResponse {
  const object = parseObject(value, ["schemaVersion", "authenticated", "user"]);
  return {
    schemaVersion: parseSchemaVersion(object.schemaVersion),
    authenticated: parseBoolean(object.authenticated),
    user: parseUserProfile(object.user),
  };
}

function parseConfigResponse(value: unknown): ConfigResponse {
  const object = parseObject(value, [
    "schemaVersion",
    "minimumSupportedVersion",
    "maintenance",
    "notice",
  ]);
  return {
    schemaVersion: parseSchemaVersion(object.schemaVersion),
    minimumSupportedVersion: parseString(object.minimumSupportedVersion),
    maintenance: parseBoolean(object.maintenance),
    notice: parseNullable(object.notice, parseText),
  };
}

function parseAccountResponse(value: unknown): AccountResponse {
  const object = parseObject(value, ["schemaVersion", "user"]);
  return {
    schemaVersion: parseSchemaVersion(object.schemaVersion),
    user: parseUserProfile(object.user),
  };
}

function parseSubscriptionResponse(value: unknown): SubscriptionPublicResponse {
  const object = parseObject(value, [
    "schemaVersion",
    "status",
    "planId",
    "expiresAtUnixMs",
    "usedBytes",
    "totalBytes",
  ]);
  return {
    schemaVersion: parseSchemaVersion(object.schemaVersion),
    status: parseStatus(object.status, SUBSCRIPTION_STATUSES),
    planId: parseNullable(object.planId, parseText),
    expiresAtUnixMs: parseNullable(object.expiresAtUnixMs, parseSafeInteger),
    usedBytes: parseSafeInteger(object.usedBytes),
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

function parsePlansResponse(value: unknown): PlansResponse {
  const object = parseObject(value, ["schemaVersion", "plans"]);
  return {
    schemaVersion: parseSchemaVersion(object.schemaVersion),
    plans: parseItems(object.plans, parsePlan),
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

function parseOrderResponse(value: unknown): OrderResponse {
  const object = parseObject(value, ["schemaVersion", "order"]);
  return {
    schemaVersion: parseSchemaVersion(object.schemaVersion),
    order: parseOrder(object.order),
  };
}

function parsePaymentResponse(value: unknown): PaymentPublicResponse {
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

function parseInviteResponse(value: unknown): InviteResponse {
  const object = parseObject(value, [
    "schemaVersion",
    "inviteCode",
    "invitedUsers",
    "commission",
  ]);
  return {
    schemaVersion: parseSchemaVersion(object.schemaVersion),
    inviteCode: parseString(object.inviteCode),
    invitedUsers: parseSafeInteger(object.invitedUsers),
    commission: parseMoney(object.commission),
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

function parseTicketsResponse(value: unknown): TicketsResponse {
  const object = parseObject(value, ["schemaVersion", "tickets"]);
  return {
    schemaVersion: parseSchemaVersion(object.schemaVersion),
    tickets: parseItems(object.tickets, parseTicket),
  };
}

function parseUpdateResponse(value: unknown): UpdateResponse {
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

export function parseBusinessApiPublicFixture(
  value: unknown,
): BusinessApiPublicFixture {
  const fixture = parseObject(value, [
    "schemaVersion",
    "environment",
    "responses",
  ]);
  if (fixture.environment !== "development") {
    throw new Error(CONTRACT_ERROR);
  }
  const responses = parseObject(fixture.responses, BUSINESS_API_OPERATIONS);
  return {
    schemaVersion: parseSchemaVersion(fixture.schemaVersion),
    environment: "development",
    responses: {
      config: parseConfigResponse(responses.config),
      login: parseAuthResponse(responses.login),
      register: parseAuthResponse(responses.register),
      account: parseAccountResponse(responses.account),
      subscription: parseSubscriptionResponse(responses.subscription),
      plans: parsePlansResponse(responses.plans),
      orders: parseOrderResponse(responses.orders),
      payment: parsePaymentResponse(responses.payment),
      invite: parseInviteResponse(responses.invite),
      tickets: parseTicketsResponse(responses.tickets),
      update: parseUpdateResponse(responses.update),
    },
  };
}

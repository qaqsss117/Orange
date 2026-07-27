import { describe, expect, it } from "vitest";
import publicFixture from "../contracts/business-api/fixtures/public-success.v1.json";
import schema from "../contracts/business-api/business-api.schema.v1.json";
import {
  ACCOUNT_STATUSES,
  BUSINESS_API_OPERATIONS,
  BUSINESS_API_SCHEMA_VERSION,
  MAX_BUSINESS_API_INTEGER,
  ORDER_STATUSES,
  PAYMENT_STATUSES,
  SUBSCRIPTION_STATUSES,
  TICKET_STATUSES,
  parseBusinessApiPublicFixture,
} from "./businessApi";

function fixtureCopy(): typeof publicFixture {
  return structuredClone(publicFixture);
}

describe("business API public contract", () => {
  it("strictly parses every public success response", () => {
    expect(parseBusinessApiPublicFixture(publicFixture)).toEqual(publicFixture);
  });

  it("matches the schema operation and status registries", () => {
    expect(schema.schemaVersion).toBe(BUSINESS_API_SCHEMA_VERSION);
    expect(schema.units.integerMaximum).toBe(MAX_BUSINESS_API_INTEGER);
    expect(
      schema["x-orange-operations"].map((operation) => operation.name),
    ).toEqual(BUSINESS_API_OPERATIONS);

    const statuses = [
      [schema.$defs.AccountStatus, ACCOUNT_STATUSES],
      [schema.$defs.SubscriptionStatus, SUBSCRIPTION_STATUSES],
      [schema.$defs.OrderStatus, ORDER_STATUSES],
      [schema.$defs.PaymentStatus, PAYMENT_STATUSES],
      [schema.$defs.TicketStatus, TICKET_STATUSES],
    ] as const;
    for (const [definition, knownValues] of statuses) {
      expect(definition["x-knownValues"]).toEqual(knownValues);
      expect(definition["x-unknownValueMapping"]).toBe("unknown");
    }
  });

  it("rejects extra or missing fields at every object boundary", () => {
    const extra = fixtureCopy();
    Object.assign(extra.responses.account.user, { futureField: true });
    expect(() => parseBusinessApiPublicFixture(extra)).toThrow(
      "Business API public contract violation",
    );

    const missing = fixtureCopy();
    delete (missing.responses.subscription as { planId?: string | null })
      .planId;
    expect(() => parseBusinessApiPublicFixture(missing)).toThrow(
      "Business API public contract violation",
    );
  });

  it("rejects unsafe units, malformed currency, and invalid nullability", () => {
    const unsafe = fixtureCopy();
    unsafe.responses.account.user.balance.minorUnits =
      MAX_BUSINESS_API_INTEGER + 1;
    expect(() => parseBusinessApiPublicFixture(unsafe)).toThrow(
      "Business API public contract violation",
    );

    const currency = fixtureCopy();
    currency.responses.plans.plans[0]!.price.currency = "cny";
    expect(() => parseBusinessApiPublicFixture(currency)).toThrow(
      "Business API public contract violation",
    );

    const nullable = fixtureCopy();
    nullable.responses.orders.order.paidAtUnixMs = undefined as unknown as null;
    expect(() => parseBusinessApiPublicFixture(nullable)).toThrow(
      "Business API public contract violation",
    );
  });

  it("maps future status values to the typed unknown state", () => {
    const future = fixtureCopy();
    future.responses.account.user.status = "future_status";
    future.responses.subscription.status = "future_status";
    future.responses.orders.order.status = "future_status";
    future.responses.payment.status = "future_status";
    future.responses.tickets.tickets[0]!.status = "future_status";

    const parsed = parseBusinessApiPublicFixture(future);
    expect(parsed.responses.account.user.status).toBe("unknown");
    expect(parsed.responses.subscription.status).toBe("unknown");
    expect(parsed.responses.orders.order.status).toBe("unknown");
    expect(parsed.responses.payment.status).toBe("unknown");
    expect(parsed.responses.tickets.tickets[0]!.status).toBe("unknown");
  });

  it("accepts empty nullable text where the schema has no minimum length", () => {
    const emptyText = fixtureCopy();
    (emptyText.responses.config as { notice: string | null }).notice = "";
    emptyText.responses.subscription.planId = "";
    (
      emptyText.responses.update as { releaseNotes: string | null }
    ).releaseNotes = "";
    expect(parseBusinessApiPublicFixture(emptyText)).toEqual(emptyText);
  });
});

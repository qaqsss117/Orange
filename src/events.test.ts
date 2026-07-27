import { describe, expect, it } from "vitest";
import stateFixture from "../contracts/observability/fixtures/data-state-event.v1.json";
import trafficFixture from "../contracts/observability/fixtures/traffic-event.v1.json";
import schema from "../contracts/observability/event-envelope.schema.v1.json";
import {
  EVENT_SCHEMA_VERSION,
  EventCursor,
  MAX_EVENT_INTEGER,
  parseEventEnvelope,
} from "./events";

describe("native event consumer", () => {
  it("round-trips strict state and traffic fixtures", () => {
    expect(parseEventEnvelope(stateFixture)).toEqual(stateFixture);
    expect(parseEventEnvelope(trafficFixture)).toEqual(trafficFixture);
  });

  it("rejects unknown fields, enum drift, and unsafe integers", () => {
    expect(() =>
      parseEventEnvelope({ ...stateFixture, path: "/tmp/private" }),
    ).toThrow("EventEnvelope contract violation");
    expect(() =>
      parseEventEnvelope({
        ...stateFixture,
        event: { ...stateFixture.event, futureField: true },
      }),
    ).toThrow("EventEnvelope contract violation");
    expect(() =>
      parseEventEnvelope({
        ...stateFixture,
        event: { kind: "data_state", state: "future" },
      }),
    ).toThrow("EventEnvelope contract violation");
    for (const sequence of [0, 1.5, MAX_EVENT_INTEGER + 1]) {
      expect(() => parseEventEnvelope({ ...stateFixture, sequence })).toThrow(
        "EventEnvelope contract violation",
      );
    }
  });

  it("discards old instances, duplicate sequences, and reordered events", () => {
    const cursor = new EventCursor();
    cursor.selectInstance(stateFixture.instanceId);
    expect(cursor.accept(stateFixture).status).toBe("applied");
    expect(cursor.accept(stateFixture).status).toBe("duplicate");
    expect(
      cursor.accept({ ...stateFixture, sequence: stateFixture.sequence - 1 })
        .status,
    ).toBe("stale_sequence");
    expect(
      cursor.accept({ ...trafficFixture, instanceId: 6, sequence: 99 }).status,
    ).toBe("stale_instance");

    cursor.selectInstance(8);
    expect(cursor.accept(stateFixture).status).toBe("stale_instance");
    expect(
      cursor.accept({ ...stateFixture, instanceId: 8, sequence: 1 }).status,
    ).toBe("applied");
    expect(cursor.snapshot()).toEqual({ activeInstanceId: 8, lastSequence: 1 });
  });

  it("matches the schema version and JavaScript-safe integer boundary", () => {
    expect(schema.properties.schemaVersion.const).toBe(EVENT_SCHEMA_VERSION);
    expect(schema.properties.instanceId.maximum).toBe(MAX_EVENT_INTEGER);
    expect(schema.properties.sequence.maximum).toBe(MAX_EVENT_INTEGER);
    expect(schema.properties.occurredAtUnixMs.maximum).toBe(MAX_EVENT_INTEGER);
    expect(schema.additionalProperties).toBe(false);
  });
});

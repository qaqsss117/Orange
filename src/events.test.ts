import { describe, expect, it } from "vitest";
import snapshotFixture from "../contracts/observability/fixtures/data-plane-event-snapshot.v1.json";
import stateFixture from "../contracts/observability/fixtures/data-state-event.v1.json";
import trafficFixture from "../contracts/observability/fixtures/traffic-event.v1.json";
import schema from "../contracts/observability/event-envelope.schema.v1.json";
import {
  DataPlaneEventConsumer,
  EVENT_SCHEMA_VERSION,
  EventCursor,
  MAX_DATA_PLANE_EVENT_CAPACITY,
  MAX_EVENT_INTEGER,
  parseDataPlaneEventSnapshot,
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

  it("parses the bounded desktop snapshot fixture strictly", () => {
    expect(parseDataPlaneEventSnapshot(snapshotFixture)).toEqual(
      snapshotFixture,
    );
    expect(() =>
      parseDataPlaneEventSnapshot({ ...snapshotFixture, diagnostic: "secret" }),
    ).toThrow("DataPlaneEventSnapshot contract violation");
    expect(() =>
      parseDataPlaneEventSnapshot({
        ...snapshotFixture,
        capacity: MAX_DATA_PLANE_EVENT_CAPACITY + 1,
      }),
    ).toThrow("DataPlaneEventSnapshot contract violation");
    expect(() =>
      parseDataPlaneEventSnapshot({
        ...snapshotFixture,
        droppedCount: MAX_EVENT_INTEGER + 1,
      }),
    ).toThrow("DataPlaneEventSnapshot contract violation");
    expect(() =>
      parseDataPlaneEventSnapshot({
        ...snapshotFixture,
        capacity: 1,
      }),
    ).toThrow("DataPlaneEventSnapshot contract violation");
    expect(() =>
      parseDataPlaneEventSnapshot({
        ...snapshotFixture,
        streamInstanceId: 8,
      }),
    ).toThrow("DataPlaneEventSnapshot contract violation");
  });

  it("lets late consumers catch up once and ignores stale snapshot entries", () => {
    const consumer = new DataPlaneEventConsumer();
    const first = consumer.consume(snapshotFixture, "online");
    expect(first).toEqual({
      streamInstanceId: 7,
      lastSequence: 12,
      droppedCount: 0,
      traffic: trafficFixture.event.sample,
    });

    const duplicate = consumer.consume(snapshotFixture, "online");
    expect(duplicate).toEqual(first);
    const staleAndFresh = {
      ...snapshotFixture,
      events: [
        { ...trafficFixture, sequence: 10 },
        {
          ...trafficFixture,
          sequence: 13,
          event: {
            ...trafficFixture.event,
            sample: {
              ...trafficFixture.event.sample,
              uploadBytesPerSecond: 512,
            },
          },
        },
      ],
    };
    expect(consumer.consume(staleAndFresh, "online").traffic).toEqual({
      ...trafficFixture.event.sample,
      uploadBytesPerSecond: 512,
    });
  });

  it("resets traffic for a new stream and zeroes speeds outside online", () => {
    const consumer = new DataPlaneEventConsumer();
    consumer.consume(snapshotFixture, "online");
    const nextInstance = {
      ...snapshotFixture,
      streamInstanceId: 8,
      events: [
        {
          ...stateFixture,
          instanceId: 8,
          sequence: 1,
          event: { kind: "data_state" as const, state: "starting" as const },
        },
      ],
    };
    expect(consumer.consume(nextInstance, "starting")).toEqual({
      streamInstanceId: 8,
      lastSequence: 1,
      droppedCount: 0,
      traffic: {
        uploadBytesTotal: 0,
        downloadBytesTotal: 0,
        uploadBytesPerSecond: 0,
        downloadBytesPerSecond: 0,
      },
    });

    const onlineTraffic = {
      ...snapshotFixture,
      streamInstanceId: 8,
      events: [
        {
          ...trafficFixture,
          instanceId: 8,
          sequence: 2,
        },
      ],
    };
    expect(consumer.consume(onlineTraffic, "stopping").traffic).toEqual({
      ...trafficFixture.event.sample,
      uploadBytesPerSecond: 0,
      downloadBytesPerSecond: 0,
    });
  });
});

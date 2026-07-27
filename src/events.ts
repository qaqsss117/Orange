import {
  CONTROL_PLANE_STATES,
  DATA_PLANE_STATES,
  type ControlPlaneState,
  type DataPlaneState,
} from "./ipc";

export const EVENT_SCHEMA_VERSION = 1 as const;
export const MAX_EVENT_INTEGER = Number.MAX_SAFE_INTEGER;

export interface TrafficSample {
  uploadBytesTotal: number;
  downloadBytesTotal: number;
  uploadBytesPerSecond: number;
  downloadBytesPerSecond: number;
}

export type PlatformEvent =
  | { kind: "control_state"; state: ControlPlaneState }
  | { kind: "data_state"; state: DataPlaneState }
  | { kind: "traffic"; sample: TrafficSample };

export interface EventEnvelope {
  schemaVersion: typeof EVENT_SCHEMA_VERSION;
  instanceId: number;
  sequence: number;
  occurredAtUnixMs: number;
  event: PlatformEvent;
}

export type EventAcceptance =
  | { status: "applied"; envelope: EventEnvelope }
  | { status: "duplicate" }
  | { status: "stale_instance" }
  | { status: "stale_sequence" };

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function hasOnlyKeys(
  value: Record<string, unknown>,
  allowedKeys: readonly string[],
): boolean {
  return Object.keys(value).every((key) => allowedKeys.includes(key));
}

function isSafeInteger(value: unknown, minimum: number): value is number {
  return (
    typeof value === "number" &&
    Number.isSafeInteger(value) &&
    value >= minimum &&
    value <= MAX_EVENT_INTEGER
  );
}

function isControlPlaneState(value: unknown): value is ControlPlaneState {
  return (
    typeof value === "string" &&
    (CONTROL_PLANE_STATES as readonly string[]).includes(value)
  );
}

function isDataPlaneState(value: unknown): value is DataPlaneState {
  return (
    typeof value === "string" &&
    (DATA_PLANE_STATES as readonly string[]).includes(value)
  );
}

function parseTrafficSample(value: unknown): TrafficSample {
  if (
    !isRecord(value) ||
    !hasOnlyKeys(value, [
      "uploadBytesTotal",
      "downloadBytesTotal",
      "uploadBytesPerSecond",
      "downloadBytesPerSecond",
    ]) ||
    !isSafeInteger(value.uploadBytesTotal, 0) ||
    !isSafeInteger(value.downloadBytesTotal, 0) ||
    !isSafeInteger(value.uploadBytesPerSecond, 0) ||
    !isSafeInteger(value.downloadBytesPerSecond, 0)
  ) {
    throw new Error("EventEnvelope contract violation");
  }
  return {
    uploadBytesTotal: value.uploadBytesTotal,
    downloadBytesTotal: value.downloadBytesTotal,
    uploadBytesPerSecond: value.uploadBytesPerSecond,
    downloadBytesPerSecond: value.downloadBytesPerSecond,
  };
}

function parsePlatformEvent(value: unknown): PlatformEvent {
  if (!isRecord(value) || typeof value.kind !== "string") {
    throw new Error("EventEnvelope contract violation");
  }
  if (
    value.kind === "control_state" &&
    hasOnlyKeys(value, ["kind", "state"]) &&
    isControlPlaneState(value.state)
  ) {
    return { kind: "control_state", state: value.state };
  }
  if (
    value.kind === "data_state" &&
    hasOnlyKeys(value, ["kind", "state"]) &&
    isDataPlaneState(value.state)
  ) {
    return { kind: "data_state", state: value.state };
  }
  if (value.kind === "traffic" && hasOnlyKeys(value, ["kind", "sample"])) {
    return { kind: "traffic", sample: parseTrafficSample(value.sample) };
  }
  throw new Error("EventEnvelope contract violation");
}

export function parseEventEnvelope(value: unknown): EventEnvelope {
  if (
    !isRecord(value) ||
    !hasOnlyKeys(value, [
      "schemaVersion",
      "instanceId",
      "sequence",
      "occurredAtUnixMs",
      "event",
    ]) ||
    value.schemaVersion !== EVENT_SCHEMA_VERSION ||
    !isSafeInteger(value.instanceId, 1) ||
    !isSafeInteger(value.sequence, 1) ||
    !isSafeInteger(value.occurredAtUnixMs, 1)
  ) {
    throw new Error("EventEnvelope contract violation");
  }
  return {
    schemaVersion: EVENT_SCHEMA_VERSION,
    instanceId: value.instanceId,
    sequence: value.sequence,
    occurredAtUnixMs: value.occurredAtUnixMs,
    event: parsePlatformEvent(value.event),
  };
}

export class EventCursor {
  private activeInstanceId: number | null = null;
  private lastSequence = 0;

  selectInstance(instanceId: number): void {
    if (!isSafeInteger(instanceId, 1)) {
      throw new Error("EventCursor instance violation");
    }
    if (this.activeInstanceId !== instanceId) {
      this.activeInstanceId = instanceId;
      this.lastSequence = 0;
    }
  }

  accept(value: unknown): EventAcceptance {
    const envelope = parseEventEnvelope(value);
    if (envelope.instanceId !== this.activeInstanceId) {
      return { status: "stale_instance" };
    }
    if (envelope.sequence === this.lastSequence) {
      return { status: "duplicate" };
    }
    if (envelope.sequence < this.lastSequence) {
      return { status: "stale_sequence" };
    }
    this.lastSequence = envelope.sequence;
    return { status: "applied", envelope };
  }

  snapshot(): { activeInstanceId: number | null; lastSequence: number } {
    return {
      activeInstanceId: this.activeInstanceId,
      lastSequence: this.lastSequence,
    };
  }
}

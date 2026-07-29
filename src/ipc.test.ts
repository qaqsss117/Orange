import { describe, expect, it } from "vitest";
import commandErrorFixture from "../contracts/fixtures/command-error.v2.json";
import dataPlaneControlRequestFixture from "../contracts/fixtures/data-plane-control.request.v2.json";
import dataPlaneControlResponseFixture from "../contracts/fixtures/data-plane-control.response.v2.json";
import snapshotFixture from "../contracts/observability/fixtures/data-plane-event-snapshot.v1.json";
import planeRequestFixture from "../contracts/fixtures/plane-state.request.v2.json";
import planeResponseFixture from "../contracts/fixtures/plane-state.response.v2.json";
import requestFixture from "../contracts/fixtures/runtime-info.request.v2.json";
import responseFixture from "../contracts/fixtures/runtime-info.response.v2.json";
import schema from "../contracts/orange-ipc.schema.json";
import { parseDataPlaneEventSnapshot } from "./events";
import {
  COMMANDS,
  CONTROL_PLANE_STATES,
  DATA_PLANE_CONTROL_ACTIONS,
  DATA_PLANE_STATES,
  ERROR_DEFINITIONS,
  ERROR_CODES,
  parseCommandError,
  parseDataPlaneControlRequest,
  parseDataPlaneControlResponse,
  parsePlaneStateRequest,
  parsePlaneStateResponse,
  parseRuntimeInfoRequest,
  parseRuntimeInfoResponse,
} from "./ipc";

describe("IPC contracts", () => {
  it("round-trips the shared request and response fixtures", () => {
    expect(parseRuntimeInfoRequest(requestFixture)).toEqual(requestFixture);
    expect(parseRuntimeInfoResponse(responseFixture)).toEqual(responseFixture);
    expect(parseCommandError(commandErrorFixture)).toEqual(commandErrorFixture);
    expect(parsePlaneStateRequest(planeRequestFixture)).toEqual(
      planeRequestFixture,
    );
    expect(parsePlaneStateResponse(planeResponseFixture)).toEqual(
      planeResponseFixture,
    );
    expect(parseDataPlaneEventSnapshot(snapshotFixture)).toEqual(
      snapshotFixture,
    );
    expect(
      parseDataPlaneControlRequest(dataPlaneControlRequestFixture),
    ).toEqual(dataPlaneControlRequestFixture);
    expect(
      parseDataPlaneControlResponse(dataPlaneControlResponseFixture),
    ).toEqual(dataPlaneControlResponseFixture);
  });

  it("rejects unknown request fields and unknown enum values", () => {
    expect(() =>
      parseRuntimeInfoRequest({ ...requestFixture, filePath: "C:/secret" }),
    ).toThrow("RuntimeInfoRequest contract violation");
    expect(() =>
      parseCommandError({ ...commandErrorFixture, code: "future_error" }),
    ).toThrow("CommandError contract violation");
    expect(() =>
      parsePlaneStateRequest({ ...planeRequestFixture, path: "/tmp/private" }),
    ).toThrow("PlaneStateRequest contract violation");
    expect(() =>
      parsePlaneStateResponse({
        ...planeResponseFixture,
        dataPlane: "future_state",
      }),
    ).toThrow("PlaneStateResponse contract violation");
    expect(() =>
      parseCommandError({
        ...commandErrorFixture,
        message: "secret diagnostic detail",
      }),
    ).toThrow("CommandError contract violation");
    expect(() =>
      parseDataPlaneControlRequest({
        ...dataPlaneControlRequestFixture,
        revision: 7,
      }),
    ).toThrow("DataPlaneControlRequest contract violation");
    expect(() =>
      parseDataPlaneControlRequest({
        ...dataPlaneControlRequestFixture,
        action: "restart",
      }),
    ).toThrow("DataPlaneControlRequest contract violation");
    expect(() =>
      parseDataPlaneControlResponse({
        ...dataPlaneControlResponseFixture,
        canStart: "yes",
      }),
    ).toThrow("DataPlaneControlResponse contract violation");
  });

  it("accepts unknown response fields for forward compatibility", () => {
    expect(
      parseRuntimeInfoResponse({ ...responseFixture, futureField: true }),
    ).toEqual(responseFixture);
    expect(
      parsePlaneStateResponse({ ...planeResponseFixture, futureField: true }),
    ).toEqual(planeResponseFixture);
    expect(
      parseDataPlaneControlResponse({
        ...dataPlaneControlResponseFixture,
        futureField: true,
      }),
    ).toEqual(dataPlaneControlResponseFixture);
  });

  it("matches the canonical schema command and error registries", () => {
    expect(schema["x-orange-commands"].map((command) => command.name)).toEqual(
      Object.values(COMMANDS),
    );
    expect(schema.$defs.ErrorCode.enum).toEqual(ERROR_CODES);
    expect(schema.$defs.ControlPlaneState.enum).toEqual(CONTROL_PLANE_STATES);
    expect(schema.$defs.DataPlaneState.enum).toEqual(DATA_PLANE_STATES);
    expect(schema.$defs.DataPlaneControlAction.enum).toEqual(
      DATA_PLANE_CONTROL_ACTIONS,
    );
    expect(schema["x-orange-error-definitions"]).toEqual(
      ERROR_CODES.map((code) => ({ code, ...ERROR_DEFINITIONS[code] })),
    );
    expect(
      schema["x-orange-commands"].find(
        (command) => command.name === COMMANDS.getDataPlaneEventSnapshot,
      ),
    ).toEqual({
      name: COMMANDS.getDataPlaneEventSnapshot,
      request: "#/$defs/DataPlaneEventSnapshotRequest",
      response: "#/$defs/DataPlaneEventSnapshotResponse",
    });
    expect(
      schema["x-orange-commands"].find(
        (command) => command.name === COMMANDS.controlDataPlane,
      ),
    ).toEqual({
      name: COMMANDS.controlDataPlane,
      request: "#/$defs/DataPlaneControlRequest",
      response: "#/$defs/DataPlaneControlResponse",
    });
  });

  it("does not include rejected input in validation errors", () => {
    const secret = "do-not-log-this-token";
    expect(() =>
      parseCommandError({ ...commandErrorFixture, message: secret }),
    ).toThrow("CommandError contract violation");
  });
});

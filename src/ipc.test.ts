import { describe, expect, it } from "vitest";
import commandErrorFixture from "../contracts/fixtures/command-error.v1.json";
import requestFixture from "../contracts/fixtures/runtime-info.request.v1.json";
import responseFixture from "../contracts/fixtures/runtime-info.response.v1.json";
import schema from "../contracts/orange-ipc.schema.json";
import {
  COMMANDS,
  ERROR_DEFINITIONS,
  ERROR_CODES,
  parseCommandError,
  parseRuntimeInfoRequest,
  parseRuntimeInfoResponse,
} from "./ipc";

describe("IPC contracts", () => {
  it("round-trips the shared request and response fixtures", () => {
    expect(parseRuntimeInfoRequest(requestFixture)).toEqual(requestFixture);
    expect(parseRuntimeInfoResponse(responseFixture)).toEqual(responseFixture);
    expect(parseCommandError(commandErrorFixture)).toEqual(commandErrorFixture);
  });

  it("rejects unknown request fields and unknown enum values", () => {
    expect(() =>
      parseRuntimeInfoRequest({ ...requestFixture, filePath: "C:/secret" }),
    ).toThrow("RuntimeInfoRequest contract violation");
    expect(() =>
      parseCommandError({ ...commandErrorFixture, code: "future_error" }),
    ).toThrow("CommandError contract violation");
    expect(() =>
      parseCommandError({
        ...commandErrorFixture,
        message: "secret diagnostic detail",
      }),
    ).toThrow("CommandError contract violation");
  });

  it("accepts unknown response fields for forward compatibility", () => {
    expect(
      parseRuntimeInfoResponse({ ...responseFixture, futureField: true }),
    ).toEqual(responseFixture);
  });

  it("matches the canonical schema command and error registries", () => {
    expect(schema["x-orange-commands"].map((command) => command.name)).toEqual(
      Object.values(COMMANDS),
    );
    expect(schema.$defs.ErrorCode.enum).toEqual(ERROR_CODES);
    expect(schema["x-orange-error-definitions"]).toEqual(
      ERROR_CODES.map((code) => ({ code, ...ERROR_DEFINITIONS[code] })),
    );
  });

  it("does not include rejected input in validation errors", () => {
    const secret = "do-not-log-this-token";
    expect(() =>
      parseCommandError({ ...commandErrorFixture, message: secret }),
    ).toThrow("CommandError contract violation");
  });
});

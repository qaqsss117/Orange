import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import responseFixture from "../contracts/fixtures/data-plane-control.response.v2.json";
import {
  COMMANDS,
  controlDataPlane,
  getConnectionMode,
  setConnectionMode,
} from "./ipc";

const { invokeMock } = vi.hoisted(() => ({ invokeMock: vi.fn() }));

vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));

describe("fixed Data Plane control command", () => {
  beforeEach(() => {
    invokeMock.mockReset();
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("sends only a versioned action and strictly parses native readiness", async () => {
    invokeMock.mockResolvedValueOnce(responseFixture);

    await expect(controlDataPlane("status")).resolves.toEqual(responseFixture);
    expect(invokeMock).toHaveBeenCalledWith(COMMANDS.controlDataPlane, {
      request: { schemaVersion: 2, action: "status" },
    });
  });

  it("rejects malformed native control responses", async () => {
    invokeMock.mockResolvedValueOnce({
      ...responseFixture,
      canStart: true,
      dataPlane: "future_state",
    });

    await expect(controlDataPlane("start")).rejects.toThrow(
      "DataPlaneControlResponse contract violation",
    );
  });

  it("gets and sets only a closed connection mode", async () => {
    invokeMock
      .mockResolvedValueOnce({ schemaVersion: 2, mode: "system_proxy" })
      .mockResolvedValueOnce({ schemaVersion: 2, mode: "tun" });

    await expect(getConnectionMode()).resolves.toEqual({
      schemaVersion: 2,
      mode: "system_proxy",
    });
    await expect(setConnectionMode("tun")).resolves.toEqual({
      schemaVersion: 2,
      mode: "tun",
    });
    expect(invokeMock).toHaveBeenNthCalledWith(1, COMMANDS.getConnectionMode, {
      request: { schemaVersion: 2 },
    });
    expect(invokeMock).toHaveBeenNthCalledWith(2, COMMANDS.setConnectionMode, {
      request: { schemaVersion: 2, mode: "tun" },
    });
  });
});

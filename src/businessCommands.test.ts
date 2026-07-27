import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import publicFixture from "../contracts/business-api/fixtures/public-success.v1.json";
import {
  AuthFormError,
  COMMANDS,
  getAuthSession,
  initializeBusiness,
  login,
  parseLoginCommandRequest,
  parseRegisterCommandRequest,
  register,
} from "./ipc";

const { invokeMock } = vi.hoisted(() => ({ invokeMock: vi.fn() }));

vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));

const authResponse = publicFixture.responses.login;
const configResponse = publicFixture.responses.config;

describe("fixed business commands", () => {
  beforeEach(() => {
    invokeMock.mockReset();
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("invokes only fixed command names and strictly parses public responses", async () => {
    invokeMock
      .mockResolvedValueOnce({
        schemaVersion: 1,
        config: configResponse,
        session: { schemaVersion: 1, status: "signed_out", user: null },
      })
      .mockResolvedValueOnce(authResponse)
      .mockResolvedValueOnce(authResponse)
      .mockResolvedValueOnce({
        schemaVersion: 1,
        status: "authenticated",
        user: authResponse.user,
      });

    await initializeBusiness();
    await login({ email: "member@example.invalid", password: "password-123" });
    await register({
      email: "new-member@example.invalid",
      password: "password-456",
      inviteCode: "INVITE_001",
    });
    await getAuthSession();

    expect(invokeMock.mock.calls.map(([command]) => command)).toEqual([
      COMMANDS.initializeBusiness,
      COMMANDS.login,
      COMMANDS.register,
      COMMANDS.getAuthSession,
    ]);
    expect(invokeMock.mock.calls[1]?.[1]).toEqual({
      request: {
        schemaVersion: 1,
        email: "member@example.invalid",
        password: "password-123",
      },
    });
  });

  it("rejects malformed form fields before native invocation", () => {
    expect(() =>
      parseLoginCommandRequest({ email: "invalid", password: "password-123" }),
    ).toThrow(AuthFormError);
    expect(() =>
      parseLoginCommandRequest({
        email: "member@example.invalid",
        password: "short",
      }),
    ).toThrow(AuthFormError);
    expect(() =>
      parseRegisterCommandRequest({
        email: "member@example.invalid",
        password: "password-123",
        inviteCode: "bad invite",
      }),
    ).toThrow(AuthFormError);
    expect(invokeMock).not.toHaveBeenCalled();
  });

  it("preserves retry input on failure without browser persistence or logging", async () => {
    const input = {
      email: "member@example.invalid",
      password: "retry-password",
    };
    const snapshot = { ...input };
    const storageSpy = vi.spyOn(Storage.prototype, "setItem");
    const logger = globalThis["console"];
    const logSpies = [
      vi.spyOn(logger, "log"),
      vi.spyOn(logger, "warn"),
      vi.spyOn(logger, "error"),
      vi.spyOn(logger, "debug"),
    ];
    invokeMock.mockRejectedValueOnce(new Error("network"));

    await expect(login(input)).rejects.toThrow("network");
    expect(input).toEqual(snapshot);
    expect(storageSpy).not.toHaveBeenCalled();
    for (const spy of logSpies) {
      expect(spy).not.toHaveBeenCalled();
    }
  });

  it("rejects native responses that try to add authentication secrets", async () => {
    invokeMock.mockResolvedValueOnce({
      ...authResponse,
      credentials: { access: "not-public" },
    });
    await expect(
      login({ email: "member@example.invalid", password: "password-123" }),
    ).rejects.toThrow("Business API public contract violation");
  });
});

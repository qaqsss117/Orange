import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import publicFixture from "../contracts/business-api/fixtures/public-success.v1.json";
import {
  AuthFormError,
  COMMANDS,
  getAuthSession,
  initializeBusiness,
  login,
  logout,
  parseAccountRefreshRequest,
  parseLoginCommandRequest,
  parseLogoutRequest,
  parseRegisterCommandRequest,
  parseSubscriptionRefreshRequest,
  refreshAccount,
  refreshSubscription,
  register,
} from "./ipc";

const { invokeMock } = vi.hoisted(() => ({ invokeMock: vi.fn() }));

vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));

const authResponse = publicFixture.responses.login;
const configResponse = publicFixture.responses.config;
const accountResponse = publicFixture.responses.account;
const subscriptionResponse = publicFixture.responses.subscription;

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
      })
      .mockResolvedValueOnce({
        schemaVersion: 1,
        status: "signed_out",
        user: null,
      })
      .mockResolvedValueOnce(accountResponse)
      .mockResolvedValueOnce(subscriptionResponse);

    await initializeBusiness();
    await login({ email: "member@example.invalid", password: "password-123" });
    await register({
      email: "new-member@example.invalid",
      password: "password-456",
      inviteCode: "INVITE_001",
    });
    await getAuthSession();
    await logout();
    await refreshAccount();
    await refreshSubscription();

    expect(invokeMock.mock.calls.map(([command]) => command)).toEqual([
      COMMANDS.initializeBusiness,
      COMMANDS.login,
      COMMANDS.register,
      COMMANDS.getAuthSession,
      COMMANDS.logout,
      COMMANDS.refreshAccount,
      COMMANDS.refreshSubscription,
    ]);
    expect(invokeMock.mock.calls[1]?.[1]).toEqual({
      request: {
        schemaVersion: 1,
        email: "member@example.invalid",
        password: "password-123",
      },
    });
    expect(invokeMock.mock.calls[4]?.[1]).toEqual({
      request: { schemaVersion: 1 },
    });
    expect(invokeMock.mock.calls[5]?.[1]).toEqual({
      request: { schemaVersion: 1 },
    });
    expect(invokeMock.mock.calls[6]?.[1]).toEqual({
      request: { schemaVersion: 1 },
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

  it("rejects URL, token, subscription credential, and extra refresh fields", () => {
    for (const injected of [
      { schemaVersion: 1, url: "https://evil.invalid" },
      { schemaVersion: 1, token: "not-allowed" },
      { schemaVersion: 1, subscriptionCredential: "not-allowed" },
      { schemaVersion: 1, extra: true },
    ]) {
      expect(() => parseAccountRefreshRequest(injected)).toThrow(
        "AccountRefreshRequest contract violation",
      );
      expect(() => parseSubscriptionRefreshRequest(injected)).toThrow(
        "SubscriptionRefreshRequest contract violation",
      );
      expect(() => parseLogoutRequest(injected)).toThrow(
        "LogoutRequest contract violation",
      );
    }
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

  it("rejects subscription credentials without browser persistence or logging", async () => {
    const storageSpy = vi.spyOn(Storage.prototype, "setItem");
    const logger = globalThis["console"];
    const logSpies = [
      vi.spyOn(logger, "log"),
      vi.spyOn(logger, "warn"),
      vi.spyOn(logger, "error"),
      vi.spyOn(logger, "debug"),
    ];
    invokeMock.mockResolvedValueOnce({
      ...subscriptionResponse,
      subscriptionCredential: "not-public",
    });

    await expect(refreshSubscription()).rejects.toThrow(
      "Business API public contract violation",
    );
    expect(storageSpy).not.toHaveBeenCalled();
    for (const spy of logSpies) {
      expect(spy).not.toHaveBeenCalled();
    }
  });
});

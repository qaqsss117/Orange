import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import App from "./App";
import type {
  AuthSessionResponse,
  BusinessInitializationResponse,
  UserProfile,
} from "./businessApi";
import { ERROR_DEFINITIONS } from "./ipc";
import { readShellPreview, type ShellServices } from "./shellServices";
import { SafeErrorBoundary } from "./ui/AsyncState";
import { UI_TEXT } from "./uiContent";
import { readUiPreview } from "./uiPreview";

const USER: UserProfile = {
  userId: "user-1",
  email: "person@example.com",
  status: "active",
  balance: { minorUnits: 0, currency: "CNY" },
};

function initialization(
  status: AuthSessionResponse["status"],
  options: { maintenance?: boolean; inviteRequired?: boolean } = {},
): BusinessInitializationResponse {
  return {
    schemaVersion: 1,
    config: {
      schemaVersion: 1,
      minimumSupportedVersion: "0.1.0",
      maintenance: options.maintenance ?? false,
      notice: options.maintenance ? "计划维护" : null,
      registrationRequiresInvite: options.inviteRequired ?? false,
    },
    session: {
      schemaVersion: 1,
      status,
      user: status === "signed_out" ? null : USER,
    },
  };
}

function shellServices(
  initial: BusinessInitializationResponse = initialization("signed_out"),
): ShellServices {
  return {
    initializeBusiness: vi.fn().mockResolvedValue(initial),
    login: vi.fn().mockResolvedValue({
      schemaVersion: 1,
      authenticated: true,
      user: USER,
    }),
    register: vi.fn().mockResolvedValue({
      schemaVersion: 1,
      authenticated: true,
      user: USER,
    }),
    logout: vi.fn().mockResolvedValue({
      schemaVersion: 1,
      status: "signed_out",
      user: null,
    }),
    getPlaneState: vi.fn().mockResolvedValue({
      schemaVersion: 1,
      controlPlane: "ready",
      dataPlane: "unconfigured",
    }),
    getDataPlaneEventSnapshot: vi.fn().mockResolvedValue({
      schemaVersion: 1,
      capacity: 64,
      droppedCount: 0,
      streamInstanceId: null,
      events: [],
    }),
    controlDataPlane: vi.fn().mockResolvedValue({
      schemaVersion: 1,
      controlPlane: "ready",
      dataPlane: "unconfigured",
      canStart: false,
      canStop: false,
    }),
  };
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((resolvePromise, rejectPromise) => {
    resolve = resolvePromise;
    reject = rejectPromise;
  });
  return { promise, resolve, reject };
}

function open(path: string) {
  window.history.replaceState({}, "", `/${path === "/" ? "" : `#${path}`}`);
}

afterEach(() => {
  window.history.replaceState({}, "", "/");
  vi.restoreAllMocks();
});

describe("App shell", () => {
  it("does not mount protected account content while restoring a signed-out session", async () => {
    open("/account");
    const startup = deferred<BusinessInitializationResponse>();
    const services = shellServices();
    services.initializeBusiness = vi.fn(() => startup.promise);
    render(<App services={services} developmentEnabled={false} />);

    expect(
      screen.getByRole("heading", { name: "正在启动安全服务" }),
    ).toBeTruthy();
    expect(screen.queryByText(USER.email)).toBeNull();

    await act(async () => startup.resolve(initialization("signed_out")));
    expect(
      await screen.findByRole("heading", { name: "登录 Orange" }),
    ).toBeTruthy();
    expect(screen.queryByText(USER.email)).toBeNull();
  });

  it("shows a safe startup failure and retries initialization", async () => {
    open("/login");
    const services = shellServices();
    vi.mocked(services.initializeBusiness)
      .mockRejectedValueOnce(new Error("secret bootstrap trace"))
      .mockResolvedValueOnce(initialization("signed_out"));
    render(<App services={services} developmentEnabled={false} />);

    expect(
      await screen.findByRole("heading", { name: "安全服务暂不可用" }),
    ).toBeTruthy();
    expect(screen.queryByText(/secret bootstrap trace/)).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "重试" }));
    expect(
      await screen.findByRole("heading", { name: "登录 Orange" }),
    ).toBeTruthy();
    expect(services.initializeBusiness).toHaveBeenCalledTimes(2);
  });

  it("restores an authenticated session directly into the protected home", async () => {
    open("/app");
    render(
      <App
        services={shellServices(initialization("authenticated"))}
        developmentEnabled={false}
      />,
    );

    expect(await screen.findByRole("heading", { name: "连接" })).toBeTruthy();
    expect(screen.getByText("尚未配置可用订阅")).toBeTruthy();
    expect(
      (
        (await screen.findByRole("button", {
          name: UI_TEXT.connectUnavailable,
        })) as HTMLButtonElement
      ).disabled,
    ).toBe(true);
  });

  it("renders authoritative online state and native traffic rates", async () => {
    open("/app");
    const services = shellServices(initialization("authenticated"));
    vi.mocked(services.controlDataPlane).mockResolvedValue({
      schemaVersion: 1,
      controlPlane: "ready",
      dataPlane: "online",
      canStart: false,
      canStop: true,
    });
    vi.mocked(services.getDataPlaneEventSnapshot).mockResolvedValue({
      schemaVersion: 1,
      capacity: 64,
      droppedCount: 0,
      streamInstanceId: 7,
      events: [
        {
          schemaVersion: 1,
          instanceId: 7,
          sequence: 1,
          occurredAtUnixMs: 1_785_157_200_000,
          event: {
            kind: "traffic",
            sample: {
              uploadBytesTotal: 4_194_304,
              downloadBytesTotal: 12_582_912,
              uploadBytesPerSecond: 786_432,
              downloadBytesPerSecond: 2_621_440,
            },
          },
        },
      ],
    });
    render(<App services={services} developmentEnabled={false} />);

    expect(await screen.findByText("订阅已配置")).toBeTruthy();
    expect(screen.getByText("已选择")).toBeTruthy();
    expect(await screen.findByText("本机流量正在受保护")).toBeTruthy();
    expect(screen.getAllByText("768 KiB/s")).toHaveLength(2);
    expect(screen.getAllByText("2.5 MiB/s")).toHaveLength(2);
    expect(services.controlDataPlane).toHaveBeenCalledTimes(1);
    expect(services.getDataPlaneEventSnapshot).toHaveBeenCalledTimes(1);
  });

  it("waits for native start readback and locks duplicate actions", async () => {
    open("/app");
    const services = shellServices(initialization("authenticated"));
    const start =
      deferred<Awaited<ReturnType<ShellServices["controlDataPlane"]>>>();
    vi.mocked(services.controlDataPlane).mockImplementation((action) => {
      if (action === "status") {
        return Promise.resolve({
          schemaVersion: 1,
          controlPlane: "ready",
          dataPlane: "unconfigured",
          canStart: true,
          canStop: false,
        });
      }
      return start.promise;
    });
    render(<App services={services} developmentEnabled={false} />);

    const button = (await screen.findByRole("button", {
      name: UI_TEXT.connection,
    })) as HTMLButtonElement;
    expect(button.disabled).toBe(false);
    fireEvent.click(button);
    fireEvent.click(button);
    expect(services.controlDataPlane).toHaveBeenCalledTimes(2);
    expect(services.controlDataPlane).toHaveBeenNthCalledWith(1, "status");
    expect(services.controlDataPlane).toHaveBeenNthCalledWith(2, "start");
    expect(screen.getByText(UI_TEXT.disconnected)).toBeTruthy();

    await act(async () =>
      start.resolve({
        schemaVersion: 1,
        controlPlane: "ready",
        dataPlane: "starting",
        canStart: false,
        canStop: false,
      }),
    );
    expect(await screen.findByText(UI_TEXT.connecting)).toBeTruthy();
  });

  it("shows only a fixed local message when native control fails", async () => {
    open("/app");
    const services = shellServices(initialization("authenticated"));
    vi.mocked(services.controlDataPlane).mockImplementation((action) =>
      action === "status"
        ? Promise.resolve({
            schemaVersion: 1,
            controlPlane: "ready",
            dataPlane: "online",
            canStart: false,
            canStop: true,
          })
        : Promise.reject(new Error("secret native lifecycle detail")),
    );
    render(<App services={services} developmentEnabled={false} />);

    fireEvent.click(
      await screen.findByRole("button", { name: UI_TEXT.disconnect }),
    );
    expect(
      await screen.findByText(UI_TEXT.connectionActionFailed),
    ).toBeTruthy();
    expect(screen.queryByText(/secret native lifecycle detail/)).toBeNull();
  });

  it("keeps connection failures safe and clears displayed speeds", async () => {
    open("/app");
    const services = shellServices(initialization("authenticated"));
    vi.mocked(services.controlDataPlane).mockRejectedValue(
      new Error("secret native state detail"),
    );
    vi.mocked(services.getDataPlaneEventSnapshot).mockRejectedValue(
      new Error("secret traffic detail"),
    );
    render(<App services={services} developmentEnabled={false} />);

    expect(await screen.findByText("本机服务暂未返回状态")).toBeTruthy();
    expect(screen.getAllByText("0 B/s")).toHaveLength(4);
    expect(screen.queryByText(/secret native|secret traffic/)).toBeNull();
  });

  it("keeps an unverified session outside protected account routes", async () => {
    open("/account");
    render(
      <App
        services={shellServices(initialization("unverified"))}
        developmentEnabled={false}
      />,
    );

    expect(await screen.findByText("登录状态尚未验证")).toBeTruthy();
    expect(screen.queryByText(USER.email)).toBeNull();
    expect(screen.getByRole("button", { name: "重新验证" })).toBeTruthy();
  });

  it("switches explicit themes through an accessible icon control", async () => {
    window.history.replaceState({}, "", "/?theme=dark#/login");
    const { container } = render(
      <App services={shellServices()} developmentEnabled={false} />,
    );

    await screen.findByRole("heading", { name: "登录 Orange" });
    const app = container.querySelector(".orange-app");
    expect(app?.getAttribute("data-theme")).toBe("dark");
    fireEvent.click(screen.getByRole("button", { name: "切换到亮色模式" }));
    expect(app?.getAttribute("data-theme")).toBe("light");
  });

  it("opens and closes the notification status", async () => {
    open("/app");
    render(
      <App
        services={shellServices(initialization("authenticated"))}
        developmentEnabled={false}
      />,
    );
    const notification = await screen.findByRole("button", { name: "通知" });

    fireEvent.click(notification);
    expect(screen.getByRole("status").textContent).toBe("暂无新通知");
    expect(notification.getAttribute("aria-expanded")).toBe("true");
    fireEvent.click(notification);
    expect(screen.queryByText("暂无新通知")).toBeNull();
  });
});

describe("Authentication", () => {
  it("validates fields locally and toggles password visibility", async () => {
    open("/login");
    const services = shellServices();
    render(<App services={services} developmentEnabled={false} />);
    const email = await screen.findByLabelText("邮箱");
    const password = screen.getByLabelText("密码");

    fireEvent.change(email, { target: { value: "invalid" } });
    fireEvent.change(password, { target: { value: "password1" } });
    fireEvent.click(screen.getByRole("button", { name: "登录" }));
    expect(screen.getByText("请输入有效邮箱。")).toBeTruthy();
    expect(services.login).not.toHaveBeenCalled();

    expect((password as HTMLInputElement).type).toBe("password");
    fireEvent.click(screen.getByRole("button", { name: "显示密码" }));
    expect((password as HTMLInputElement).type).toBe("text");
  });

  it("requires matching registration passwords and a required invite", async () => {
    open("/register");
    const services = shellServices(
      initialization("signed_out", { inviteRequired: true }),
    );
    render(<App services={services} developmentEnabled={false} />);
    const email = await screen.findByLabelText("邮箱");
    const password = screen.getByLabelText("密码");
    const confirmation = screen.getByLabelText("确认密码");

    fireEvent.change(email, { target: { value: "person@example.com" } });
    fireEvent.change(password, { target: { value: "password1" } });
    fireEvent.change(confirmation, { target: { value: "password2" } });
    fireEvent.click(screen.getByRole("button", { name: "注册" }));
    expect(screen.getByText("两次输入的密码不一致。")).toBeTruthy();

    fireEvent.change(confirmation, { target: { value: "password1" } });
    fireEvent.click(screen.getByRole("button", { name: "注册" }));
    expect(screen.getByText("请输入邀请码。")).toBeTruthy();
    expect(services.register).not.toHaveBeenCalled();
  });

  it("locks duplicate login submissions and enters the protected route", async () => {
    open("/login");
    const login = deferred<Awaited<ReturnType<ShellServices["login"]>>>();
    const services = shellServices();
    services.login = vi.fn(() => login.promise);
    render(<App services={services} developmentEnabled={false} />);

    fireEvent.change(await screen.findByLabelText("邮箱"), {
      target: { value: "person@example.com" },
    });
    fireEvent.change(screen.getByLabelText("密码"), {
      target: { value: "password1" },
    });
    const submit = screen.getByRole("button", { name: "登录" });
    fireEvent.click(submit);
    fireEvent.click(submit);
    expect(services.login).toHaveBeenCalledTimes(1);
    expect(
      (screen.getByRole("button", { name: "正在登录" }) as HTMLButtonElement)
        .disabled,
    ).toBe(true);

    await act(async () =>
      login.resolve({ schemaVersion: 1, authenticated: true, user: USER }),
    );
    expect(await screen.findByRole("heading", { name: "连接" })).toBeTruthy();
    expect(screen.getByText("登录成功")).toBeTruthy();
  });

  it("redacts unknown service errors while preserving form values", async () => {
    open("/login");
    const services = shellServices();
    vi.mocked(services.login).mockRejectedValueOnce(
      new Error("secret-token=never-render-this"),
    );
    render(<App services={services} developmentEnabled={false} />);

    const email = await screen.findByLabelText("邮箱");
    const password = screen.getByLabelText("密码");
    fireEvent.change(email, { target: { value: "person@example.com" } });
    fireEvent.change(password, { target: { value: "password1" } });
    fireEvent.click(screen.getByRole("button", { name: "登录" }));

    expect(await screen.findByText("操作未完成，请稍后重试。")).toBeTruthy();
    expect(screen.queryByText(/secret-token/)).toBeNull();
    expect((email as HTMLInputElement).value).toBe("person@example.com");
    expect((password as HTMLInputElement).value).toBe("password1");
  });

  it("disables authentication during maintenance", async () => {
    open("/login");
    const services = shellServices(
      initialization("signed_out", { maintenance: true }),
    );
    render(<App services={services} developmentEnabled={false} />);

    expect(await screen.findByText("服务维护中")).toBeTruthy();
    expect(
      (screen.getByRole("button", { name: "登录" }) as HTMLButtonElement)
        .disabled,
    ).toBe(true);
    expect(screen.getByText("计划维护")).toBeTruthy();
  });
});

describe("Logout and safe failure", () => {
  it("closes logout with Escape, restores focus, and retries a safe failure", async () => {
    open("/account");
    const services = shellServices(initialization("authenticated"));
    vi.mocked(services.logout)
      .mockRejectedValueOnce({
        schemaVersion: 1,
        code: "network",
        ...ERROR_DEFINITIONS.network,
      })
      .mockResolvedValueOnce({
        schemaVersion: 1,
        status: "signed_out",
        user: null,
      });
    render(<App services={services} developmentEnabled={false} />);

    const logout = await screen.findByRole("button", { name: "退出登录" });
    logout.focus();
    fireEvent.click(logout);
    expect(screen.getByRole("dialog")).toBeTruthy();
    expect(document.activeElement).toBe(
      screen.getByRole("button", { name: "取消" }),
    );
    fireEvent.keyDown(document, { key: "Escape" });
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(document.activeElement).toBe(logout);

    fireEvent.click(logout);
    fireEvent.click(screen.getByRole("button", { name: "确认退出" }));
    expect(
      await screen.findByText(ERROR_DEFINITIONS.network.message),
    ).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "确认退出" }));
    expect(
      await screen.findByRole("heading", { name: "登录 Orange" }),
    ).toBeTruthy();
    expect(screen.getByText("已安全退出")).toBeTruthy();
  });

  it("does not expose thrown render secrets from the error boundary", () => {
    function ThrowingPage(): never {
      throw new Error("secret stack contents");
    }

    render(
      <SafeErrorBoundary>
        <ThrowingPage />
      </SafeErrorBoundary>,
    );
    expect(
      screen.getByRole("heading", { name: "页面暂时无法显示" }),
    ).toBeTruthy();
    expect(screen.queryByText(/secret stack contents/)).toBeNull();
    expect(screen.getByRole("button", { name: "重新加载页面" })).toBeTruthy();
  });
});

describe("Preview configuration", () => {
  it("accepts only fixed UI examples", () => {
    expect(readUiPreview("?theme=light&scale=large&motion=reduced")).toEqual({
      theme: "light",
      fontScale: "large",
      motion: "reduced",
    });
    expect(readUiPreview("?theme=unknown&scale=2&motion=spin")).toEqual({
      theme: "system",
      fontScale: "normal",
      motion: "full",
    });
  });

  it("keeps shell preview modes behind the development gate", () => {
    expect(readShellPreview("?shell=authenticated", false)).toBeNull();
    expect(readShellPreview("?shell=authenticated", true)).toBe(
      "authenticated",
    );
    expect(readShellPreview("?shell=arbitrary", true)).toBeNull();
  });
});

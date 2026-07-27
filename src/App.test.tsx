import { fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import App from "./App";
import { readUiPreview } from "./uiPreview";

afterEach(() => {
  window.history.replaceState({}, "", "/");
});

describe("App", () => {
  it("renders the responsive connection baseline without faking a connection", () => {
    render(<App />);

    expect(screen.getAllByText("Orange").length).toBeGreaterThan(0);
    expect(screen.getByRole("heading", { name: "连接" })).toBeTruthy();
    expect(screen.getByText("尚未配置可用订阅")).toBeTruthy();
    expect(screen.getByText("当前未连接")).toBeTruthy();

    const connection = screen.getByRole("button", { name: "连接不可用" });
    expect((connection as HTMLButtonElement).disabled).toBe(true);
  });

  it("switches explicit themes through an accessible icon control", () => {
    window.history.replaceState({}, "", "/?theme=dark");
    const { container } = render(<App />);

    const app = container.querySelector(".orange-app");
    expect(app?.getAttribute("data-theme")).toBe("dark");
    fireEvent.click(screen.getByRole("button", { name: "切换到亮色模式" }));
    expect(app?.getAttribute("data-theme")).toBe("light");
    expect(screen.getByRole("button", { name: "切换到暗色模式" })).toBeTruthy();
  });

  it("opens and closes the notification status", () => {
    render(<App />);
    const notification = screen.getByRole("button", { name: "通知" });

    fireEvent.click(notification);
    expect(screen.getByRole("status").textContent).toBe("暂无新通知");
    expect(notification.getAttribute("aria-expanded")).toBe("true");

    fireEvent.click(notification);
    expect(screen.queryByRole("status")).toBeNull();
  });
});

describe("UI preview configuration", () => {
  it("accepts only the fixed theme, font, and motion examples", () => {
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
});

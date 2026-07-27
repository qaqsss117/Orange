import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import App from "./App";

describe("App", () => {
  it("renders the product and startup state", () => {
    render(<App />);

    expect(screen.getByRole("heading", { name: "Orange" })).toBeTruthy();
    expect(screen.getByText("正在初始化安全连接")).toBeTruthy();
  });
});

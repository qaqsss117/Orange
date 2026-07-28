import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import App from "./App";
import "./styles.css";

const root = document.getElementById("root");

if (!root) {
  throw new Error("Orange root element is missing");
}

createRoot(root, {
  onCaughtError: () => undefined,
  onRecoverableError: () => undefined,
  onUncaughtError: () => undefined,
}).render(
  <StrictMode>
    <App />
  </StrictMode>,
);

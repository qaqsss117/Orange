import { isTauri } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Minus, Square, Copy, X } from "lucide-react";
import { useEffect, useState } from "react";
import orangeIcon from "../../assets/product/brand/orange-development-mark.png";
import { UI_TEXT } from "../uiContent";

const appWindow = isTauri() ? getCurrentWindow() : null;

export function Titlebar() {
  const [maximized, setMaximized] = useState(false);

  useEffect(() => {
    if (appWindow === null) return;

    let disposed = false;
    const sync = () => {
      void appWindow.isMaximized().then((value) => {
        if (!disposed) setMaximized(value);
      });
    };
    sync();
    const unlisten = appWindow.onResized(() => sync());
    return () => {
      disposed = true;
      void unlisten.then((off) => off());
    };
  }, []);

  return (
    <header className="titlebar" data-tauri-drag-region>
      <div className="titlebar-brand" data-tauri-drag-region>
        <img src={orangeIcon} alt="" aria-hidden="true" draggable={false} />
        <span data-tauri-drag-region>{UI_TEXT.brand}</span>
      </div>
      {appWindow !== null && (
        <div className="titlebar-controls">
          <button
            type="button"
            className="titlebar-button"
            aria-label="最小化"
            title="最小化"
            onClick={() => void appWindow.minimize()}
          >
            <Minus aria-hidden="true" />
          </button>
          <button
            type="button"
            className="titlebar-button"
            aria-label={maximized ? "还原" : "最大化"}
            title={maximized ? "还原" : "最大化"}
            onClick={() => void appWindow.toggleMaximize()}
          >
            {maximized ? (
              <Copy aria-hidden="true" />
            ) : (
              <Square aria-hidden="true" />
            )}
          </button>
          <button
            type="button"
            className="titlebar-button titlebar-button-close"
            aria-label="关闭"
            title="关闭"
            onClick={() => void appWindow.close()}
          >
            <X aria-hidden="true" />
          </button>
        </div>
      )}
    </header>
  );
}

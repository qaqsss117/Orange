import { ChatboxColors, ChatboxPosition, Crisp } from "crisp-sdk-web";

const CRISP_WEBSITE_ID = "127061de-943f-459b-922e-b958ccd6844c";
const CRISP_CLIENT_URL = "https://client.crisp.chat/l.js";
const OPEN_TIMEOUT_MS = 15_000;

let configured = false;
let injected = false;
let injectionFailed = false;
let clientScript: HTMLScriptElement | null = null;
let pendingOpen: Promise<void> | null = null;

function configureCrisp() {
  if (configured) return;

  Crisp.configure(CRISP_WEBSITE_ID, {
    autoload: false,
    locale: "zh",
    safeMode: true,
  });
  Crisp.setColorTheme(ChatboxColors.Orange);
  Crisp.setPosition(ChatboxPosition.Right);
  Crisp.setZIndex(1000);
  Crisp.chat.onChatClosed(() => Crisp.chat.hide());
  configured = true;
}

function trackClientScript(script: HTMLScriptElement | null) {
  clientScript = script;
  script?.addEventListener(
    "error",
    () => {
      injectionFailed = true;
    },
    { once: true },
  );
}

// 注入客户端脚本。首次交给 SDK 处理，因为它同时写入 CRISP_WEBSITE_ID 等全局变量；
// 之后 SDK 认为已注入不会再动，重试时自己补一个脚本标签即可。
function injectClient() {
  injectionFailed = false;

  if (injected) {
    const retry = document.createElement("script");
    retry.src = CRISP_CLIENT_URL;
    retry.async = true;
    trackClientScript(retry);
    document.head.appendChild(retry);
    return;
  }

  Crisp.load();
  injected = true;
  trackClientScript(
    document.querySelector<HTMLScriptElement>(
      `script[src="${CRISP_CLIENT_URL}"]`,
    ),
  );
}

/**
 * 静默预加载客服：注入客户端脚本并建立会话，但不显示气泡入口，
 * 用户点击客服按钮时即可直接打开。重复调用只加载一次。
 */
export function preloadCrispSupportChat(): void {
  if (injected) return;

  configureCrisp();
  injectClient();
  // 隐藏指令先排进 $crisp 队列，客户端异步启动后立即执行，默认气泡不会闪出。
  Crisp.chat.hide();
}

export function openCrispSupportChat(): Promise<void> {
  configureCrisp();

  if (Crisp.chat.isChatOpened()) {
    Crisp.chat.show();
    return Promise.resolve();
  }

  if (pendingOpen !== null) return pendingOpen;

  // 尚未预加载，或上次注入失败（例如线路未连通时预加载被拦截），此时再加载一次。
  if (!injected || injectionFailed) {
    injectClient();
  }

  pendingOpen = new Promise<void>((resolve, reject) => {
    let settled = false;
    const complete = (error?: Error) => {
      if (settled) return;
      settled = true;
      window.clearTimeout(timeout);
      if (error) {
        reject(error);
      } else {
        resolve();
      }
    };
    const timeout = window.setTimeout(
      () => complete(new Error("Crisp chat did not open in time")),
      OPEN_TIMEOUT_MS,
    );

    Crisp.chat.onChatOpened(() => complete());

    try {
      Crisp.chat.show();
      Crisp.chat.open();
      clientScript?.addEventListener(
        "error",
        () => complete(new Error("Crisp client failed to load")),
        { once: true },
      );
    } catch (error) {
      complete(
        error instanceof Error
          ? error
          : new Error("Crisp chat could not be opened"),
      );
    }
  }).finally(() => {
    pendingOpen = null;
  });

  return pendingOpen;
}

import { ChatboxColors, ChatboxPosition, Crisp } from "crisp-sdk-web";

const CRISP_WEBSITE_ID = "5546c6ea-4b1e-41bc-80e4-4b6648cbca76";
const CRISP_CLIENT_URL = "https://client.crisp.chat/l.js";
const OPEN_TIMEOUT_MS = 15_000;

let configured = false;
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

export function openCrispSupportChat(): Promise<void> {
  configureCrisp();

  if (Crisp.chat.isChatOpened()) {
    Crisp.chat.show();
    return Promise.resolve();
  }

  if (pendingOpen !== null) return pendingOpen;

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
      document
        .querySelector<HTMLScriptElement>(`script[src="${CRISP_CLIENT_URL}"]`)
        ?.addEventListener(
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

import { useState } from "react";
import { LoaderCircle, MessagesSquare } from "lucide-react";
import { openCrispSupportChat } from "../crispSupport";

export function SupportChatButton() {
  const [opening, setOpening] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const openChat = async () => {
    if (opening) return;
    setOpening(true);
    setError(null);
    try {
      await openCrispSupportChat();
    } catch {
      setError("客服暂时无法连接，请稍后重试");
    } finally {
      setOpening(false);
    }
  };

  return (
    <div className="support-chat-control">
      <button
        type="button"
        className="icon-button support-chat-button"
        aria-label={opening ? "正在打开在线客服" : "在线客服"}
        title={opening ? "正在打开在线客服" : "在线客服"}
        aria-busy={opening}
        disabled={opening}
        onClick={() => void openChat()}
      >
        {opening ? (
          <LoaderCircle className="spinning" aria-hidden="true" />
        ) : (
          <MessagesSquare aria-hidden="true" />
        )}
      </button>
      {error !== null && (
        <span className="support-chat-error" role="alert">
          {error}
        </span>
      )}
    </div>
  );
}

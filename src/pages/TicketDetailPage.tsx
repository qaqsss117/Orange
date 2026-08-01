import {
  AlertCircle,
  ArrowLeft,
  CalendarDays,
  LoaderCircle,
  MessageSquareText,
  RefreshCw,
  Send,
  XCircle,
} from "lucide-react";
import { type FormEvent, useCallback, useEffect, useState } from "react";
import { Link, useParams } from "react-router-dom";
import type { TicketDetail } from "../businessApi";
import { type ShellServices, toPublicUiError } from "../shellServices";
import { ConfirmDialog } from "../ui/AsyncState";

const STATUS_LABELS: Record<TicketDetail["status"], string> = {
  open: "待回复",
  answered: "已回复",
  closed: "已关闭",
  unknown: "状态未知",
};

function formatDate(value: number): string {
  return new Intl.DateTimeFormat("zh-CN", {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  }).format(new Date(value));
}

function utf8Length(value: string): number {
  return new TextEncoder().encode(value).length;
}

function validateReply(value: string): string | null {
  if (value.length === 0) return "请输入回复内容";
  if (utf8Length(value) > 4 * 1024) return "回复内容不能超过 4096 字节";
  if (
    [...value].some((character) => {
      const code = character.charCodeAt(0);
      return (
        (code <= 31 &&
          character !== "\n" &&
          character !== "\r" &&
          character !== "\t") ||
        code === 127
      );
    })
  ) {
    return "回复内容包含不可用字符";
  }
  return null;
}

export function TicketDetailPage({ services }: { services: ShellServices }) {
  const params = useParams<{ ticketId: string }>();
  const ticketId = params.ticketId ?? "";
  const [ticket, setTicket] = useState<TicketDetail | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [reply, setReply] = useState("");
  const [replying, setReplying] = useState(false);
  const [replyError, setReplyError] = useState<string | null>(null);
  const [closeDialogOpen, setCloseDialogOpen] = useState(false);
  const [closing, setClosing] = useState(false);
  const [closeError, setCloseError] = useState<string | null>(null);

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const response = await services.fetchTicketDetail(ticketId);
      setTicket(response.ticket);
    } catch (reason) {
      setError(toPublicUiError(reason).message);
    } finally {
      setLoading(false);
    }
  }, [services, ticketId]);

  useEffect(() => {
    setTicket(null);
    setReply("");
    setReplyError(null);
    setCloseDialogOpen(false);
    setCloseError(null);
    void load();
  }, [load]);

  const submitReply = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (ticket === null || ticket.status !== "open" || replying) return;

    const message = reply.trim();
    const validationError = validateReply(message);
    setReplyError(validationError);
    if (validationError !== null) return;

    setReplying(true);
    try {
      const response = await services.replyTicket(ticket.ticketId, message);
      setTicket(response.ticket);
      setReply("");
      setError(null);
      setReplyError(null);
    } catch (reason) {
      setReplyError(toPublicUiError(reason).message);
    } finally {
      setReplying(false);
    }
  };

  const confirmClose = async () => {
    if (ticket === null || ticket.status !== "open" || closing) return;
    setClosing(true);
    setCloseError(null);
    try {
      const response = await services.closeTicket(ticket.ticketId);
      setTicket(response.ticket);
      setReply("");
      setReplyError(null);
      setError(null);
      setCloseDialogOpen(false);
    } catch (reason) {
      setCloseError(toPublicUiError(reason).message);
    } finally {
      setClosing(false);
    }
  };

  return (
    <main className="management-page ticket-detail-page">
      <header className="management-heading ticket-detail-heading">
        <div>
          <Link className="ticket-back-link" to="/tickets">
            <ArrowLeft aria-hidden="true" />
            返回工单
          </Link>
          <span>客户支持</span>
          <h2>工单详情</h2>
          <p>工单 #{ticketId}</p>
        </div>
        <div className="ticket-detail-actions">
          <button
            type="button"
            className="secondary-action"
            disabled={loading || replying || closing}
            onClick={() => void load()}
          >
            <RefreshCw
              className={loading ? "spinning" : ""}
              aria-hidden="true"
            />
            {loading ? "正在刷新" : "刷新详情"}
          </button>
          {ticket?.status === "open" && (
            <button
              type="button"
              className="danger-action"
              disabled={replying || closing}
              onClick={() => {
                setCloseError(null);
                setCloseDialogOpen(true);
              }}
            >
              <XCircle aria-hidden="true" />
              关闭工单
            </button>
          )}
        </div>
      </header>

      {loading && ticket === null ? (
        <div className="page-state" role="status">
          <RefreshCw className="spinning" aria-hidden="true" />
          <span>正在读取工单</span>
        </div>
      ) : error !== null && ticket === null ? (
        <div className="page-state page-state-error" role="alert">
          <AlertCircle aria-hidden="true" />
          <span>{error}</span>
          <button
            type="button"
            className="inline-action"
            onClick={() => void load()}
          >
            重试
          </button>
        </div>
      ) : ticket !== null ? (
        <>
          {error !== null && (
            <div className="inline-notice inline-notice-error" role="alert">
              <AlertCircle aria-hidden="true" />
              <span>{error}</span>
            </div>
          )}

          <section
            className="ticket-detail-summary"
            aria-labelledby="ticket-subject"
          >
            <div>
              <span>工单主题</span>
              <h3 id="ticket-subject">{ticket.subject}</h3>
            </div>
            <dl>
              <div>
                <dt>
                  <MessageSquareText aria-hidden="true" />
                  当前状态
                </dt>
                <dd className={`status-${ticket.status}`}>
                  {STATUS_LABELS[ticket.status]}
                </dd>
              </div>
              <div>
                <dt>
                  <CalendarDays aria-hidden="true" />
                  创建时间
                </dt>
                <dd>{formatDate(ticket.createdAtUnixMs)}</dd>
              </div>
              <div>
                <dt>
                  <RefreshCw aria-hidden="true" />
                  最近更新
                </dt>
                <dd>{formatDate(ticket.updatedAtUnixMs)}</dd>
              </div>
            </dl>
          </section>

          <section
            className="ticket-conversation"
            aria-labelledby="ticket-conversation-title"
          >
            <header>
              <span>会话记录</span>
              <h3 id="ticket-conversation-title">消息</h3>
            </header>
            {ticket.messages.length === 0 ? (
              <div className="page-state compact">
                <MessageSquareText aria-hidden="true" />
                <strong>暂无消息</strong>
              </div>
            ) : (
              <ol>
                {ticket.messages.map((message) => (
                  <li
                    className={
                      message.fromUser
                        ? "message-from-user"
                        : "message-from-support"
                    }
                    key={message.messageId}
                  >
                    <div>
                      <span>{message.fromUser ? "我" : "客户支持"}</span>
                      <p>{message.body}</p>
                      <time
                        dateTime={new Date(
                          message.createdAtUnixMs,
                        ).toISOString()}
                      >
                        {formatDate(message.createdAtUnixMs)}
                      </time>
                    </div>
                  </li>
                ))}
              </ol>
            )}

            {ticket.status === "open" && (
              <form
                className="ticket-reply-form"
                onSubmit={(event) => void submitReply(event)}
              >
                <label className="field-group" htmlFor="ticket-reply">
                  <span className="ticket-field-label">
                    <strong>回复工单</strong>
                    <small>{utf8Length(reply.trim())} / 4096</small>
                  </span>
                  <span className="ticket-message-shell">
                    <textarea
                      id="ticket-reply"
                      name="reply"
                      rows={4}
                      value={reply}
                      disabled={replying}
                      aria-invalid={replyError !== null}
                      aria-describedby={
                        replyError === null ? undefined : "ticket-reply-error"
                      }
                      onChange={(event) => {
                        setReply(event.target.value);
                        setReplyError(null);
                      }}
                    />
                  </span>
                </label>
                {replyError !== null && (
                  <div
                    className="form-error"
                    id="ticket-reply-error"
                    role="alert"
                  >
                    <AlertCircle aria-hidden="true" />
                    <span>{replyError}</span>
                  </div>
                )}
                <div className="ticket-reply-actions">
                  <button
                    type="submit"
                    className="primary-action"
                    disabled={replying}
                  >
                    {replying ? (
                      <LoaderCircle className="spinning" aria-hidden="true" />
                    ) : (
                      <Send aria-hidden="true" />
                    )}
                    {replying ? "正在回复" : "发送回复"}
                  </button>
                </div>
              </form>
            )}
          </section>

          {closeDialogOpen && (
            <ConfirmDialog
              title="关闭工单"
              detail={`确认关闭工单 #${ticket.ticketId}？关闭后不能继续回复。`}
              confirmLabel={closing ? "正在关闭" : "确认关闭"}
              cancelLabel="返回"
              busy={closing}
              error={closeError}
              onConfirm={() => void confirmClose()}
              onCancel={() => {
                if (!closing) {
                  setCloseDialogOpen(false);
                  setCloseError(null);
                }
              }}
            />
          )}
        </>
      ) : null}
    </main>
  );
}

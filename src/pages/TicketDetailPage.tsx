import {
  AlertCircle,
  ArrowLeft,
  CalendarDays,
  MessageSquareText,
  RefreshCw,
} from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { Link, useParams } from "react-router-dom";
import type { TicketDetail } from "../businessApi";
import { type ShellServices, toPublicUiError } from "../shellServices";

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

export function TicketDetailPage({ services }: { services: ShellServices }) {
  const params = useParams<{ ticketId: string }>();
  const ticketId = params.ticketId ?? "";
  const [ticket, setTicket] = useState<TicketDetail | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

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
    void load();
  }, [load]);

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
        <button
          type="button"
          className="secondary-action"
          disabled={loading}
          onClick={() => void load()}
        >
          <RefreshCw className={loading ? "spinning" : ""} aria-hidden="true" />
          {loading ? "正在刷新" : "刷新详情"}
        </button>
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
          </section>
        </>
      ) : null}
    </main>
  );
}

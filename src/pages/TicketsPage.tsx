import {
  AlertCircle,
  Clock3,
  LifeBuoy,
  MessageSquareText,
  RefreshCw,
} from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import type { Ticket } from "../businessApi";
import { type ShellServices, toPublicUiError } from "../shellServices";

const STATUS_LABELS: Record<Ticket["status"], string> = {
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

export function TicketsPage({ services }: { services: ShellServices }) {
  const [tickets, setTickets] = useState<Ticket[] | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const response = await services.fetchTickets();
      setTickets(response.tickets);
    } catch (reason) {
      setError(toPublicUiError(reason).message);
    } finally {
      setLoading(false);
    }
  }, [services]);

  useEffect(() => {
    void load();
  }, [load]);

  return (
    <main className="management-page tickets-page">
      <header className="management-heading">
        <div>
          <span>客户支持</span>
          <h2>我的工单</h2>
          <p>支持请求及其处理状态。</p>
        </div>
        <button
          type="button"
          className="secondary-action"
          disabled={loading}
          onClick={() => void load()}
        >
          <RefreshCw className={loading ? "spinning" : ""} aria-hidden="true" />
          {loading ? "正在刷新" : "刷新工单"}
        </button>
      </header>

      {loading && tickets === null ? (
        <div className="page-state" role="status">
          <RefreshCw className="spinning" aria-hidden="true" />
          <span>正在读取工单</span>
        </div>
      ) : error !== null && tickets === null ? (
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
      ) : tickets?.length === 0 ? (
        <div className="page-state">
          <LifeBuoy aria-hidden="true" />
          <strong>暂无工单</strong>
        </div>
      ) : (
        <>
          {error !== null && (
            <div className="inline-notice inline-notice-error" role="alert">
              <AlertCircle aria-hidden="true" />
              <span>{error}</span>
            </div>
          )}
          <div className="ticket-list">
            {tickets?.map((ticket) => (
              <article className="ticket-row" key={ticket.ticketId}>
                <div className="ticket-subject">
                  <span>工单 #{ticket.ticketId}</span>
                  <h3>{ticket.subject}</h3>
                </div>
                <div className="ticket-updated">
                  <Clock3 aria-hidden="true" />
                  <span>最近更新</span>
                  <strong>{formatDate(ticket.lastMessageAtUnixMs)}</strong>
                </div>
                <div className="ticket-state">
                  <MessageSquareText aria-hidden="true" />
                  <strong className={`status-${ticket.status}`}>
                    {STATUS_LABELS[ticket.status]}
                  </strong>
                </div>
              </article>
            ))}
          </div>
        </>
      )}
    </main>
  );
}

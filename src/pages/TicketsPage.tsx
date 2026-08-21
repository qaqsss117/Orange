import {
  AlertCircle,
  ChevronRight,
  Clock3,
  LifeBuoy,
  LoaderCircle,
  MessageSquareText,
  Plus,
  RefreshCw,
  Send,
  X,
} from "lucide-react";
import { type FormEvent, useCallback, useEffect, useState } from "react";
import { Link } from "react-router-dom";
import type { Ticket } from "../businessApi";
import { MAX_TICKET_MESSAGE_CHARS, MAX_TICKET_SUBJECT_CHARS } from "../ipc";
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

function charLength(value: string): number {
  return [...value].length;
}

function validateSubject(value: string): string | null {
  if (value.length === 0) return "请输入工单主题";
  if (charLength(value) > MAX_TICKET_SUBJECT_CHARS) {
    return `工单主题不能超过 ${MAX_TICKET_SUBJECT_CHARS} 个字`;
  }
  if (
    [...value].some(
      (character) =>
        character.charCodeAt(0) <= 31 || character.charCodeAt(0) === 127,
    )
  ) {
    return "工单主题包含不可用字符";
  }
  return null;
}

function validateMessage(value: string): string | null {
  if (value.length === 0) return "请输入问题描述";
  if (charLength(value) > MAX_TICKET_MESSAGE_CHARS) {
    return `问题描述不能超过 ${MAX_TICKET_MESSAGE_CHARS} 个字`;
  }
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
    return "问题描述包含不可用字符";
  }
  return null;
}

export function TicketsPage({ services }: { services: ShellServices }) {
  const [tickets, setTickets] = useState<Ticket[] | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [creatingOpen, setCreatingOpen] = useState(false);
  const [creating, setCreating] = useState(false);
  const [subject, setSubject] = useState("");
  const [message, setMessage] = useState("");
  const [subjectError, setSubjectError] = useState<string | null>(null);
  const [messageError, setMessageError] = useState<string | null>(null);
  const [createError, setCreateError] = useState<string | null>(null);

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

  const closeCreateForm = () => {
    if (creating) return;
    setCreatingOpen(false);
    setSubject("");
    setMessage("");
    setSubjectError(null);
    setMessageError(null);
    setCreateError(null);
  };

  const submitTicket = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (creating) return;

    const normalizedSubject = subject.trim();
    const normalizedMessage = message.trim();
    const nextSubjectError = validateSubject(normalizedSubject);
    const nextMessageError = validateMessage(normalizedMessage);
    setSubjectError(nextSubjectError);
    setMessageError(nextMessageError);
    setCreateError(null);
    if (nextSubjectError !== null || nextMessageError !== null) return;

    setCreating(true);
    try {
      const response = await services.createTicket(
        normalizedSubject,
        normalizedMessage,
      );
      setTickets(response.tickets);
      setError(null);
      setCreatingOpen(false);
      setSubject("");
      setMessage("");
    } catch (reason) {
      setCreateError(toPublicUiError(reason).message);
    } finally {
      setCreating(false);
    }
  };

  return (
    <main className="management-page tickets-page">
      <header className="management-heading">
        <div>
          <span>客户支持</span>
          <h2>我的工单</h2>
          <p>支持请求及其处理状态。</p>
        </div>
        <div className="ticket-heading-actions">
          <button
            type="button"
            className={creatingOpen ? "secondary-action" : "primary-action"}
            disabled={creating}
            onClick={() => {
              if (creatingOpen) closeCreateForm();
              else setCreatingOpen(true);
            }}
          >
            {creatingOpen ? (
              <X aria-hidden="true" />
            ) : (
              <Plus aria-hidden="true" />
            )}
            {creatingOpen ? "取消新建" : "新建工单"}
          </button>
          <button
            type="button"
            className="secondary-action"
            disabled={loading || creating}
            onClick={() => void load()}
          >
            <RefreshCw
              className={loading ? "spinning" : ""}
              aria-hidden="true"
            />
            {loading ? "正在刷新" : "刷新工单"}
          </button>
        </div>
      </header>

      {creatingOpen && (
        <form
          className="ticket-create-form"
          onSubmit={(event) => void submitTicket(event)}
        >
          <header>
            <span>新工单</span>
            <h3>提交支持请求</h3>
          </header>

          <label className="field-group" htmlFor="ticket-subject">
            <span className="ticket-field-label">
              <strong>主题</strong>
              <small>
                {charLength(subject.trim())} / {MAX_TICKET_SUBJECT_CHARS}
              </small>
            </span>
            <span className="input-shell">
              <MessageSquareText aria-hidden="true" />
              <input
                id="ticket-subject"
                name="subject"
                type="text"
                value={subject}
                disabled={creating}
                aria-invalid={subjectError !== null}
                aria-describedby={
                  subjectError === null ? undefined : "ticket-subject-error"
                }
                onChange={(event) => {
                  setSubject(event.target.value);
                  setSubjectError(null);
                  setCreateError(null);
                }}
              />
            </span>
            {subjectError !== null && (
              <span className="field-error" id="ticket-subject-error">
                {subjectError}
              </span>
            )}
          </label>

          <label className="field-group" htmlFor="ticket-message">
            <span className="ticket-field-label">
              <strong>问题描述</strong>
              <small>
                {charLength(message.trim())} / {MAX_TICKET_MESSAGE_CHARS}
              </small>
            </span>
            <span className="ticket-message-shell">
              <textarea
                id="ticket-message"
                name="message"
                rows={6}
                value={message}
                disabled={creating}
                aria-invalid={messageError !== null}
                aria-describedby={
                  messageError === null ? undefined : "ticket-message-error"
                }
                onChange={(event) => {
                  setMessage(event.target.value);
                  setMessageError(null);
                  setCreateError(null);
                }}
              />
            </span>
            {messageError !== null && (
              <span className="field-error" id="ticket-message-error">
                {messageError}
              </span>
            )}
          </label>

          {createError !== null && (
            <div className="form-error" role="alert">
              <AlertCircle aria-hidden="true" />
              <span>{createError}</span>
            </div>
          )}

          <div className="ticket-create-actions">
            <button
              type="button"
              className="secondary-action"
              disabled={creating}
              onClick={closeCreateForm}
            >
              取消
            </button>
            <button
              type="submit"
              className="primary-action"
              disabled={creating}
            >
              {creating ? (
                <LoaderCircle className="spinning" aria-hidden="true" />
              ) : (
                <Send aria-hidden="true" />
              )}
              {creating ? "正在提交" : "提交工单"}
            </button>
          </div>
        </form>
      )}

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
              <Link
                className="ticket-row"
                key={ticket.ticketId}
                to={`/tickets/${encodeURIComponent(ticket.ticketId)}`}
                aria-label={`查看工单 ${ticket.subject}`}
              >
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
                  <ChevronRight aria-hidden="true" />
                </div>
              </Link>
            ))}
          </div>
        </>
      )}
    </main>
  );
}

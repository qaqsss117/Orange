import {
  Component,
  type ReactNode,
  useCallback,
  useEffect,
  useId,
  useRef,
} from "react";
import { AlertTriangle, Inbox, LoaderCircle, RotateCw, X } from "lucide-react";
import { SHELL_TEXT } from "../shellContent";

interface StatusScreenProps {
  kind: "empty" | "error" | "loading";
  title: string;
  detail: string;
  actionLabel?: string;
  onAction?: () => void;
}

export function StatusScreen({
  kind,
  title,
  detail,
  actionLabel,
  onAction,
}: StatusScreenProps) {
  const Icon =
    kind === "loading"
      ? LoaderCircle
      : kind === "error"
        ? AlertTriangle
        : Inbox;
  return (
    <main
      className={`status-screen status-${kind}`}
      role={kind === "error" ? "alert" : "status"}
      aria-live="polite"
    >
      <Icon className="status-icon" aria-hidden="true" />
      <h1>{title}</h1>
      <p>{detail}</p>
      {actionLabel && onAction && (
        <button type="button" className="secondary-action" onClick={onAction}>
          <RotateCw aria-hidden="true" />
          {actionLabel}
        </button>
      )}
    </main>
  );
}

export interface ToastMessage {
  id: number;
  text: string;
  kind: "error" | "success";
}

export function ToastRegion({
  message,
  onDismiss,
}: {
  message: ToastMessage | null;
  onDismiss: () => void;
}) {
  if (message === null) {
    return <div className="toast-region" aria-live="polite" />;
  }
  return (
    <div className="toast-region" aria-live="polite">
      <div className={`toast toast-${message.kind}`} role="status">
        <span>{message.text}</span>
        <button
          type="button"
          className="toast-dismiss"
          aria-label={SHELL_TEXT.dismiss}
          title={SHELL_TEXT.dismiss}
          onClick={onDismiss}
        >
          <X aria-hidden="true" />
        </button>
      </div>
    </div>
  );
}

interface ConfirmDialogProps {
  title: string;
  detail: string;
  confirmLabel: string;
  cancelLabel: string;
  busy: boolean;
  error: string | null;
  onConfirm: () => void;
  onCancel: () => void;
  children?: ReactNode;
}

export function ConfirmDialog({
  title,
  detail,
  confirmLabel,
  cancelLabel,
  busy,
  error,
  onConfirm,
  onCancel,
  children,
}: ConfirmDialogProps) {
  const titleId = useId();
  const detailId = useId();
  const dialogId = useId();
  const dialogRef = useRef<HTMLDivElement>(null);
  const cancelRef = useRef<HTMLButtonElement>(null);
  const onCancelRef = useRef(onCancel);
  useEffect(() => {
    onCancelRef.current = onCancel;
  }, [onCancel]);
  const requestCancel = useCallback(() => {
    if (window.history.state?.orangeDialog === dialogId) {
      window.history.back();
    } else {
      onCancelRef.current();
    }
  }, [dialogId]);

  useEffect(() => {
    const previousFocus = document.activeElement;
    const currentState =
      typeof window.history.state === "object" && window.history.state !== null
        ? window.history.state
        : {};
    window.history.pushState({ ...currentState, orangeDialog: dialogId }, "");
    cancelRef.current?.focus();

    const handlePopState = () => {
      if (window.history.state?.orangeDialog !== dialogId) {
        onCancelRef.current();
      }
    };
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        requestCancel();
        return;
      }
      if (event.key !== "Tab") {
        return;
      }
      const focusable = Array.from(
        dialogRef.current?.querySelectorAll<HTMLButtonElement>(
          "button:not([disabled])",
        ) ?? [],
      );
      if (focusable.length === 0) {
        event.preventDefault();
        return;
      }
      const first = focusable[0];
      const last = focusable.at(-1);
      if (event.shiftKey && document.activeElement === first) {
        event.preventDefault();
        last?.focus();
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault();
        first?.focus();
      }
    };
    window.addEventListener("popstate", handlePopState);
    document.addEventListener("keydown", handleKeyDown);
    return () => {
      window.removeEventListener("popstate", handlePopState);
      document.removeEventListener("keydown", handleKeyDown);
      if (window.history.state?.orangeDialog === dialogId) {
        const nextState = { ...window.history.state };
        delete nextState.orangeDialog;
        window.history.replaceState(nextState, "");
      }
      if (previousFocus instanceof HTMLElement && previousFocus.isConnected) {
        previousFocus.focus();
      }
    };
  }, [dialogId, requestCancel]);

  return (
    <div
      className="dialog-backdrop"
      onMouseDown={(event) => {
        if (event.currentTarget === event.target && !busy) {
          requestCancel();
        }
      }}
    >
      <div
        ref={dialogRef}
        className="confirm-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        aria-describedby={detailId}
      >
        <h2 id={titleId}>{title}</h2>
        <p id={detailId}>{detail}</p>
        {children}
        {error && (
          <div className="dialog-error" role="alert">
            {error}
          </div>
        )}
        <div className="dialog-actions">
          <button
            ref={cancelRef}
            type="button"
            className="secondary-action"
            disabled={busy}
            onClick={requestCancel}
          >
            {cancelLabel}
          </button>
          <button
            type="button"
            className="danger-action"
            disabled={busy}
            onClick={onConfirm}
          >
            {confirmLabel}
          </button>
        </div>
      </div>
    </div>
  );
}

interface SafeErrorBoundaryProps {
  children: ReactNode;
}

interface SafeErrorBoundaryState {
  failed: boolean;
}

export class SafeErrorBoundary extends Component<
  SafeErrorBoundaryProps,
  SafeErrorBoundaryState
> {
  state: SafeErrorBoundaryState = { failed: false };

  static getDerivedStateFromError(): SafeErrorBoundaryState {
    return { failed: true };
  }

  componentDidCatch(): void {
    // Intentionally do not log potentially sensitive render errors.
  }

  render() {
    if (this.state.failed) {
      return (
        <div className="orange-app app-public" data-theme="system">
          <StatusScreen
            kind="error"
            title={SHELL_TEXT.safeFailureTitle}
            detail={SHELL_TEXT.safeFailureDetail}
            actionLabel={SHELL_TEXT.retryPage}
            onAction={() => this.setState({ failed: false })}
          />
        </div>
      );
    }
    return this.props.children;
  }
}

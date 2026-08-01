import {
  AlertCircle,
  ChevronRight,
  CircleUserRound,
  CreditCard,
  LogOut,
  ReceiptText,
  RefreshCw,
  Server,
  Settings,
  UserRoundPlus,
  WalletCards,
} from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { Link } from "react-router-dom";
import type {
  AuthSessionResponse,
  Money,
  SubscriptionPublicResponse,
  UserProfile,
} from "../businessApi";
import type { SubscriptionSnapshotResponse } from "../ipc";
import { type ShellServices, toPublicUiError } from "../shellServices";
import { SHELL_TEXT } from "../shellContent";
import { ConfirmDialog } from "../ui/AsyncState";

const STATUS_LABELS: Record<UserProfile["status"], string> = {
  active: "正常",
  disabled: "已停用",
  unknown: "状态未知",
};

const SUBSCRIPTION_STATUS_LABELS: Record<
  SubscriptionPublicResponse["status"],
  string
> = {
  none: "暂无订阅",
  trial: "试用中",
  active: "有效",
  expired: "已到期",
  exhausted: "流量已用尽",
  unknown: "状态未知",
};

function formatMoney(value: Money): string {
  try {
    return new Intl.NumberFormat("zh-CN", {
      style: "currency",
      currency: value.currency,
      minimumFractionDigits: 2,
      maximumFractionDigits: 2,
    }).format(value.minorUnits / 100);
  } catch {
    return `${value.currency} ${(value.minorUnits / 100).toFixed(2)}`;
  }
}

function formatBytes(value: number | null): string {
  if (value === null) return "不限量";
  if (value === 0) return "0 B";
  const units = ["B", "KiB", "MiB", "GiB", "TiB"];
  const index = Math.min(Math.floor(Math.log(value) / Math.log(1024)), 4);
  const scaled = value / 1024 ** index;
  return `${scaled >= 10 || index === 0 ? scaled.toFixed(0) : scaled.toFixed(1)} ${units[index]}`;
}

function formatExpiry(value: number | null): string {
  if (value === null) return "长期有效";
  return new Intl.DateTimeFormat("zh-CN", {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
  }).format(new Date(value));
}

export function AccountPage({
  user,
  services,
  onUserUpdated,
  onLoggedOut,
}: {
  user: UserProfile;
  services: ShellServices;
  onUserUpdated: (user: UserProfile) => void;
  onLoggedOut: (session: AuthSessionResponse) => void;
}) {
  const [account, setAccount] = useState(user);
  const [snapshot, setSnapshot] = useState<SubscriptionSnapshotResponse | null>(
    null,
  );
  const [loadingSubscription, setLoadingSubscription] = useState(true);
  const [refreshing, setRefreshing] = useState(false);
  const [dialogOpen, setDialogOpen] = useState(false);
  const [loggingOut, setLoggingOut] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    setAccount(user);
  }, [user]);

  const loadSubscription = useCallback(async () => {
    setLoadingSubscription(true);
    try {
      setSnapshot(await services.getSubscriptionSnapshot());
    } catch (reason) {
      setError(toPublicUiError(reason).message);
    } finally {
      setLoadingSubscription(false);
    }
  }, [services]);

  useEffect(() => {
    void loadSubscription();
  }, [loadSubscription]);

  const refresh = async () => {
    if (refreshing) return;
    setRefreshing(true);
    setError(null);
    try {
      const response = await services.refreshAccount();
      setAccount(response.user);
      onUserUpdated(response.user);
      setSnapshot(await services.getSubscriptionSnapshot());
    } catch (reason) {
      setError(toPublicUiError(reason).message);
    } finally {
      setRefreshing(false);
    }
  };

  const confirmLogout = async () => {
    if (loggingOut) return;
    setLoggingOut(true);
    setError(null);
    try {
      const session = await services.logout();
      setDialogOpen(false);
      onLoggedOut(session);
    } catch (reason) {
      setError(toPublicUiError(reason).message);
    } finally {
      setLoggingOut(false);
    }
  };

  const subscription = snapshot?.subscription ?? null;
  const usedBytes = subscription?.usedBytes ?? 0;
  const totalBytes = subscription?.totalBytes ?? null;
  const usage =
    totalBytes === null || totalBytes === 0
      ? 0
      : Math.min(100, (usedBytes / totalBytes) * 100);

  return (
    <main className="account-page">
      <header className="management-heading account-heading">
        <div>
          <span>账户中心</span>
          <h2>我的账户</h2>
        </div>
        <button
          type="button"
          className="secondary-action"
          disabled={refreshing}
          onClick={() => void refresh()}
        >
          <RefreshCw
            className={refreshing ? "spinning" : ""}
            aria-hidden="true"
          />
          {refreshing ? "正在刷新" : "刷新"}
        </button>
      </header>

      {error !== null && !dialogOpen && (
        <div className="inline-notice inline-notice-error" role="alert">
          <AlertCircle aria-hidden="true" />
          <span>{error}</span>
        </div>
      )}

      <section className="account-identity" aria-labelledby="account-email">
        <div className="account-avatar" aria-hidden="true">
          <CircleUserRound />
        </div>
        <div className="account-profile-copy">
          <span>Orange ID</span>
          <h3 id="account-email">{account.email}</h3>
          <span className={`account-state state-${account.status}`}>
            {STATUS_LABELS[account.status]}
          </span>
        </div>
        <dl className="account-facts">
          <div>
            <dt>账户余额</dt>
            <dd>{formatMoney(account.balance)}</dd>
          </div>
          <div>
            <dt>账户编号</dt>
            <dd>{account.userId}</dd>
          </div>
        </dl>
      </section>

      <section
        className="account-section account-subscription"
        aria-labelledby="account-subscription-title"
      >
        <div className="account-section-heading">
          <div>
            <CreditCard aria-hidden="true" />
            <div>
              <span>当前服务</span>
              <h3 id="account-subscription-title">我的订阅</h3>
            </div>
          </div>
          <Link to="/subscription">
            管理
            <ChevronRight aria-hidden="true" />
          </Link>
        </div>

        {loadingSubscription ? (
          <div className="account-inline-state" role="status">
            <RefreshCw className="spinning" aria-hidden="true" />
            <span>正在读取订阅</span>
          </div>
        ) : subscription === null ? (
          <div className="account-inline-state">
            <span>暂无可用订阅</span>
          </div>
        ) : (
          <div className="account-subscription-body">
            <div className="account-subscription-summary">
              <div>
                <span>套餐</span>
                <strong>{subscription.planId ?? "未命名套餐"}</strong>
              </div>
              <div>
                <span>状态</span>
                <strong className={`status-${subscription.status}`}>
                  {SUBSCRIPTION_STATUS_LABELS[subscription.status]}
                </strong>
              </div>
              <div>
                <span>到期时间</span>
                <strong>{formatExpiry(subscription.expiresAtUnixMs)}</strong>
              </div>
            </div>
            <div className="account-usage">
              <div>
                <span>已使用 {formatBytes(usedBytes)}</span>
                <strong>{formatBytes(totalBytes)}</strong>
              </div>
              {totalBytes !== null && (
                <div
                  className="usage-track"
                  role="progressbar"
                  aria-label={`已使用 ${usage.toFixed(0)}%`}
                  aria-valuemin={0}
                  aria-valuemax={100}
                  aria-valuenow={Math.round(usage)}
                >
                  <span style={{ width: `${usage}%` }} />
                </div>
              )}
            </div>
          </div>
        )}
      </section>

      <nav
        className="account-section account-service-list"
        aria-label="账户服务"
      >
        <Link to="/subscription">
          <CreditCard aria-hidden="true" />
          <span>
            <strong>订阅管理</strong>
            <small>{subscription?.planId ?? "查看套餐状态"}</small>
          </span>
          <ChevronRight aria-hidden="true" />
        </Link>
        <Link to="/orders">
          <ReceiptText aria-hidden="true" />
          <span>
            <strong>我的订单</strong>
            <small>查看购买记录和订单状态</small>
          </span>
          <ChevronRight aria-hidden="true" />
        </Link>
        <Link to="/invitation">
          <UserRoundPlus aria-hidden="true" />
          <span>
            <strong>我的邀请</strong>
            <small>邀请码、邀请注册和佣金记录</small>
          </span>
          <ChevronRight aria-hidden="true" />
        </Link>
        <Link to="/nodes">
          <Server aria-hidden="true" />
          <span>
            <strong>节点管理</strong>
            <small>选择当前使用的节点</small>
          </span>
          <ChevronRight aria-hidden="true" />
        </Link>
        <Link to="/settings">
          <Settings aria-hidden="true" />
          <span>
            <strong>连接设置</strong>
            <small>系统代理与 TUN</small>
          </span>
          <ChevronRight aria-hidden="true" />
        </Link>
      </nav>

      <section
        className="account-section account-actions"
        aria-labelledby="account-actions-title"
      >
        <div>
          <WalletCards aria-hidden="true" />
          <h3 id="account-actions-title">账户操作</h3>
        </div>
        <button
          type="button"
          className="danger-action account-logout"
          onClick={() => {
            setError(null);
            setDialogOpen(true);
          }}
        >
          <LogOut aria-hidden="true" />
          {SHELL_TEXT.logout}
        </button>
      </section>

      {dialogOpen && (
        <ConfirmDialog
          title={SHELL_TEXT.logoutDialogTitle}
          detail={SHELL_TEXT.logoutDialogDetail}
          confirmLabel={
            loggingOut ? SHELL_TEXT.loggingOut : SHELL_TEXT.confirmLogout
          }
          cancelLabel={SHELL_TEXT.cancel}
          busy={loggingOut}
          error={error}
          onConfirm={() => void confirmLogout()}
          onCancel={() => {
            if (!loggingOut) setDialogOpen(false);
          }}
        />
      )}
    </main>
  );
}

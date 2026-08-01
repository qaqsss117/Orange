import {
  AlertCircle,
  CalendarDays,
  Database,
  Package,
  RefreshCw,
} from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import type { Money, Plan, SubscriptionPublicResponse } from "../businessApi";
import type { SubscriptionSnapshotResponse } from "../ipc";
import { toPublicUiError, type ShellServices } from "../shellServices";

const STATUS_LABELS: Record<SubscriptionPublicResponse["status"], string> = {
  none: "无可用订阅",
  trial: "试用中",
  active: "有效",
  expired: "已到期",
  exhausted: "流量已用尽",
  unknown: "状态未知",
};

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
    hour: "2-digit",
    minute: "2-digit",
  }).format(new Date(value));
}

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

function formatBillingPeriod(days: number): string {
  const labels: Record<number, string> = {
    0: "一次性",
    30: "月付",
    90: "季付",
    180: "半年付",
    365: "年付",
    730: "两年付",
    1095: "三年付",
  };
  return labels[days] ?? `${days} 天`;
}

interface PlanGroup {
  id: string;
  name: string;
  trafficBytes: number | null;
  options: Plan[];
}

function groupPlans(plans: Plan[]): PlanGroup[] {
  const groups = new Map<string, PlanGroup>();
  for (const plan of plans) {
    const separator = plan.planId.lastIndexOf(":");
    const id = separator === -1 ? plan.planId : plan.planId.slice(0, separator);
    const current = groups.get(id);
    if (current === undefined) {
      groups.set(id, {
        id,
        name: plan.name,
        trafficBytes: plan.trafficBytes,
        options: [plan],
      });
    } else {
      current.options.push(plan);
    }
  }
  return [...groups.values()];
}

function PlansSection({ services }: { services: ShellServices }) {
  const [plans, setPlans] = useState<Plan[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  const loadPlans = useCallback(async () => {
    setError(null);
    try {
      const response = await services.fetchPlans();
      setPlans(response.plans);
    } catch (reason) {
      setError(toPublicUiError(reason).message);
    }
  }, [services]);

  useEffect(() => {
    void loadPlans();
  }, [loadPlans]);

  const groups = plans === null ? [] : groupPlans(plans);

  return (
    <section className="plans-section" aria-labelledby="plans-title">
      <div className="section-heading">
        <Package aria-hidden="true" />
        <div>
          <h3 id="plans-title">可选套餐</h3>
          <p>当前可购买的流量额度和计费周期。</p>
        </div>
      </div>

      {plans === null && error === null ? (
        <div className="page-state compact" role="status">
          <RefreshCw className="spinning" aria-hidden="true" />
          <span>正在读取套餐</span>
        </div>
      ) : error !== null ? (
        <div className="plan-catalog-state" role="alert">
          <AlertCircle aria-hidden="true" />
          <span>{error}</span>
          <button
            type="button"
            className="inline-action"
            onClick={() => void loadPlans()}
          >
            重试
          </button>
        </div>
      ) : groups.length === 0 ? (
        <div className="plan-catalog-state">
          <Package aria-hidden="true" />
          <span>暂无可购买套餐</span>
        </div>
      ) : (
        <div className="plan-catalog">
          {groups.map((group) => (
            <article className="plan-card" key={group.id}>
              <header>
                <div>
                  <span>订阅套餐</span>
                  <h4>{group.name}</h4>
                </div>
                <strong>{formatBytes(group.trafficBytes)}</strong>
              </header>
              <dl>
                {group.options.map((option) => (
                  <div key={option.planId}>
                    <dt>{formatBillingPeriod(option.billingPeriodDays)}</dt>
                    <dd>{formatMoney(option.price)}</dd>
                  </div>
                ))}
              </dl>
            </article>
          ))}
        </div>
      )}
    </section>
  );
}

export function SubscriptionPage({ services }: { services: ShellServices }) {
  const [snapshot, setSnapshot] = useState<SubscriptionSnapshotResponse | null>(
    null,
  );
  const [loading, setLoading] = useState(true);
  const [refreshing, setRefreshing] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      setSnapshot(await services.getSubscriptionSnapshot());
    } catch (reason) {
      setError(toPublicUiError(reason).message);
    } finally {
      setLoading(false);
    }
  }, [services]);

  useEffect(() => {
    let active = true;
    void services.getSubscriptionSnapshot().then(
      (value) => {
        if (active) {
          setSnapshot(value);
          setLoading(false);
        }
      },
      (reason) => {
        if (active) {
          setError(toPublicUiError(reason).message);
          setLoading(false);
        }
      },
    );
    return () => {
      active = false;
    };
  }, [services]);

  const refresh = async () => {
    if (refreshing) return;
    setRefreshing(true);
    setError(null);
    try {
      await services.refreshSubscription();
      setSnapshot(await services.getSubscriptionSnapshot());
    } catch (reason) {
      setError(toPublicUiError(reason).message);
    } finally {
      setRefreshing(false);
    }
  };

  const subscription = snapshot?.subscription ?? null;
  const total = subscription?.totalBytes ?? null;
  const used = subscription?.usedBytes ?? 0;
  const progress =
    total === null || total === 0 ? 0 : Math.min(100, (used / total) * 100);

  return (
    <main className="management-page subscription-page">
      <div className="management-heading">
        <div>
          <span>账户服务</span>
          <h2>订阅</h2>
          <p>查看当前套餐状态、流量额度和到期时间。</p>
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
          {refreshing ? "正在刷新" : "刷新订阅"}
        </button>
      </div>

      {loading ? (
        <div className="page-state" role="status">
          <RefreshCw className="spinning" aria-hidden="true" />
          <span>正在读取订阅</span>
        </div>
      ) : error !== null && snapshot === null ? (
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
      ) : subscription !== null ? (
        <>
          {error !== null && (
            <div className="inline-notice inline-notice-error" role="alert">
              <AlertCircle aria-hidden="true" />
              <span>{error}</span>
            </div>
          )}
          <section
            className="subscription-overview"
            aria-labelledby="subscription-plan"
          >
            <div>
              <span className="status-label">订阅状态</span>
              <strong
                className={`subscription-status status-${subscription.status}`}
              >
                {STATUS_LABELS[subscription.status]}
              </strong>
            </div>
            <div>
              <span>套餐</span>
              <strong id="subscription-plan">
                {subscription.planId ?? "未命名套餐"}
              </strong>
            </div>
            <div>
              <span>本地配置</span>
              <strong>
                {snapshot?.localRevision === null ? "未配置" : "已同步"}
              </strong>
            </div>
          </section>

          <section className="usage-section" aria-labelledby="usage-title">
            <div className="section-heading">
              <Database aria-hidden="true" />
              <div>
                <h3 id="usage-title">流量额度</h3>
                <p>
                  {formatBytes(used)} / {formatBytes(total)}
                </p>
              </div>
            </div>
            {total !== null && (
              <div
                className="usage-track"
                role="progressbar"
                aria-label={`已使用 ${progress.toFixed(0)}%`}
                aria-valuemin={0}
                aria-valuemax={100}
                aria-valuenow={Math.round(progress)}
              >
                <span style={{ width: `${progress}%` }} />
              </div>
            )}
          </section>

          <section className="expiry-section" aria-labelledby="expiry-title">
            <CalendarDays aria-hidden="true" />
            <div>
              <h3 id="expiry-title">到期时间</h3>
              <p>{formatExpiry(subscription.expiresAtUnixMs)}</p>
            </div>
          </section>
        </>
      ) : (
        <div className="page-state" role="status">
          <Database aria-hidden="true" />
          <strong>
            {snapshot?.localRevision === null
              ? "暂无可用订阅"
              : "正在使用本地订阅"}
          </strong>
          <span>
            {snapshot?.localRevision === null
              ? "刷新订阅后才能连接。"
              : "远端状态暂不可用，本机保留了最近一次有效配置。"}
          </span>
          {error !== null && <span className="field-error">{error}</span>}
        </div>
      )}

      <PlansSection services={services} />
    </main>
  );
}

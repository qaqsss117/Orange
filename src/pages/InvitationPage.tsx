import {
  AlertCircle,
  Check,
  Copy,
  Eye,
  Gift,
  Percent,
  Plus,
  RefreshCw,
  UserRoundPlus,
  WalletCards,
} from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import type {
  InvitationCenterResponse,
  InvitationCode,
  Money,
} from "../businessApi";
import { type ShellServices, toPublicUiError } from "../shellServices";

const STATUS_LABELS: Record<InvitationCode["status"], string> = {
  available: "可用",
  used: "已使用",
  disabled: "已停用",
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

function formatDate(value: number | null): string {
  if (value === null) return "时间未知";
  return new Intl.DateTimeFormat("zh-CN", {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  }).format(new Date(value));
}

export function InvitationPage({ services }: { services: ShellServices }) {
  const [center, setCenter] = useState<InvitationCenterResponse | null>(null);
  const [loading, setLoading] = useState(true);
  const [generating, setGenerating] = useState(false);
  const [copiedCode, setCopiedCode] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      setCenter(await services.fetchInvitationCenter());
    } catch (reason) {
      setError(toPublicUiError(reason).message);
    } finally {
      setLoading(false);
    }
  }, [services]);

  useEffect(() => {
    void load();
  }, [load]);

  const generate = async () => {
    if (generating) return;
    setGenerating(true);
    setError(null);
    try {
      setCenter(await services.generateInvitationCode());
    } catch (reason) {
      setError(toPublicUiError(reason).message);
    } finally {
      setGenerating(false);
    }
  };

  const copyCode = async (code: string) => {
    setError(null);
    try {
      await navigator.clipboard.writeText(code);
      setCopiedCode(code);
    } catch {
      setError("邀请码复制失败，请稍后重试。");
    }
  };

  return (
    <main className="management-page invitation-page">
      <header className="management-heading invitation-heading">
        <div>
          <span>账户服务</span>
          <h2>我的邀请</h2>
          <p>邀请注册、佣金和邀请码记录。</p>
        </div>
        <div className="invitation-actions">
          <button
            type="button"
            className="secondary-action"
            disabled={loading || generating}
            onClick={() => void load()}
          >
            <RefreshCw
              className={loading ? "spinning" : ""}
              aria-hidden="true"
            />
            {loading ? "正在刷新" : "刷新"}
          </button>
          <button
            type="button"
            className="primary-action"
            disabled={generating || loading}
            onClick={() => void generate()}
          >
            {generating ? (
              <RefreshCw className="spinning" aria-hidden="true" />
            ) : (
              <Plus aria-hidden="true" />
            )}
            {generating ? "正在生成" : "生成邀请码"}
          </button>
        </div>
      </header>

      {loading && center === null ? (
        <div className="page-state" role="status">
          <RefreshCw className="spinning" aria-hidden="true" />
          <span>正在读取邀请信息</span>
        </div>
      ) : error !== null && center === null ? (
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
      ) : center !== null ? (
        <>
          {error !== null && (
            <div className="inline-notice inline-notice-error" role="alert">
              <AlertCircle aria-hidden="true" />
              <span>{error}</span>
            </div>
          )}

          <dl className="invitation-stats">
            <div>
              <dt>
                <UserRoundPlus aria-hidden="true" />
                邀请注册
              </dt>
              <dd>{center.stats.registeredUsers} 人</dd>
            </div>
            <div>
              <dt>
                <Percent aria-hidden="true" />
                佣金比例
              </dt>
              <dd>{center.stats.commissionRatePercent}%</dd>
            </div>
            <div>
              <dt>
                <WalletCards aria-hidden="true" />
                确认中佣金
              </dt>
              <dd>{formatMoney(center.stats.pendingCommission)}</dd>
            </div>
            <div>
              <dt>
                <Gift aria-hidden="true" />
                累计佣金
              </dt>
              <dd>{formatMoney(center.stats.totalCommission)}</dd>
            </div>
          </dl>

          <section
            className="invitation-code-section"
            aria-labelledby="invitation-code-title"
          >
            <header>
              <div>
                <span>邀请码管理</span>
                <h3 id="invitation-code-title">邀请码</h3>
              </div>
              <strong>{center.codes.length} 个</strong>
            </header>

            {center.codes.length === 0 ? (
              <div className="page-state compact">
                <Gift aria-hidden="true" />
                <strong>暂无邀请码</strong>
              </div>
            ) : (
              <div className="invitation-code-list">
                {center.codes.map((item) => (
                  <article className="invitation-code-row" key={item.code}>
                    <div className="invitation-code-copy">
                      <span>邀请码</span>
                      <strong>{item.code}</strong>
                    </div>
                    <dl>
                      <div>
                        <dt>
                          <Eye aria-hidden="true" />
                          访问量
                        </dt>
                        <dd>{item.views}</dd>
                      </div>
                      <div>
                        <dt>创建时间</dt>
                        <dd>{formatDate(item.createdAtUnixMs)}</dd>
                      </div>
                    </dl>
                    <div className="invitation-code-controls">
                      <span
                        className={`invitation-status status-${item.status}`}
                      >
                        {STATUS_LABELS[item.status]}
                      </span>
                      <button
                        type="button"
                        className="icon-button"
                        aria-label={`复制邀请码 ${item.code}`}
                        title={
                          copiedCode === item.code ? "已复制" : "复制邀请码"
                        }
                        onClick={() => void copyCode(item.code)}
                      >
                        {copiedCode === item.code ? (
                          <Check aria-hidden="true" />
                        ) : (
                          <Copy aria-hidden="true" />
                        )}
                      </button>
                    </div>
                  </article>
                ))}
              </div>
            )}
          </section>
        </>
      ) : null}
    </main>
  );
}

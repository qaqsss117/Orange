import {
  AlertCircle,
  ArrowLeftRight,
  Banknote,
  Check,
  Copy,
  Eye,
  Gift,
  Percent,
  Plus,
  RefreshCw,
  Share2,
  UserRoundPlus,
  WalletCards,
} from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import type {
  CommissionConfigResponse,
  InvitationCenterResponse,
  InvitationCode,
  Money,
} from "../businessApi";
import { type ShellServices, toPublicUiError } from "../shellServices";
import { ConfirmDialog } from "../ui/AsyncState";

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
  const [sharing, setSharing] = useState(false);
  const [shareCopied, setShareCopied] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [config, setConfig] = useState<CommissionConfigResponse | null>(null);
  const [transferAmount, setTransferAmount] = useState("");
  const [withdrawMethod, setWithdrawMethod] = useState("");
  const [withdrawAccount, setWithdrawAccount] = useState("");
  const [commissionPending, setCommissionPending] = useState<
    "transfer" | "withdraw" | null
  >(null);
  const [commissionMessage, setCommissionMessage] = useState<string | null>(
    null,
  );
  const [confirmingAction, setConfirmingAction] = useState<
    "transfer" | "withdraw" | null
  >(null);

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

  useEffect(() => {
    let active = true;
    void services.fetchCommissionConfig().then(
      (response) => {
        if (active) {
          setConfig(response);
          setWithdrawMethod((current) =>
            current === "" ? (response.withdrawMethods[0] ?? "") : current,
          );
        }
      },
      () => {
        if (active) setConfig(null);
      },
    );
    return () => {
      active = false;
    };
  }, [services]);

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

  const shareInvite = async () => {
    if (sharing) return;
    const code = center?.codes.find(
      (candidate) => candidate.status === "available",
    )?.code;
    if (code === undefined) {
      setError("暂无可用邀请码，请先生成邀请码。");
      return;
    }
    setSharing(true);
    setError(null);
    setShareCopied(false);
    try {
      const { url } = await services.getServicePortalUrl();
      const message = [
        "目前为止我用过最好的加速器，播放高清视频从未如此轻松。",
        "",
        `下载链接（推荐使用 Chrome 浏览器访问）：${url}`,
        "",
        `安装后打开填写我的邀请码：${code}，你能多得 3 天会员！`,
      ].join("\n");
      await navigator.clipboard.writeText(message);
      setShareCopied(true);
    } catch (reason) {
      setError(toPublicUiError(reason).message);
    } finally {
      setSharing(false);
    }
  };

  const availableMinor = center?.stats.totalCommission.minorUnits ?? 0;

  const transferCommission = async () => {
    if (commissionPending !== null) return;
    const yuan = Number.parseFloat(transferAmount);
    if (!Number.isFinite(yuan) || yuan <= 0) {
      setError("请输入有效的划转金额。");
      return;
    }
    const amountMinor = Math.round(yuan * 100);
    if (amountMinor <= 0 || amountMinor > availableMinor) {
      setError("划转金额不能超过可划转佣金余额。");
      return;
    }
    setCommissionPending("transfer");
    setError(null);
    setCommissionMessage(null);
    try {
      await services.transferCommission(amountMinor);
      setCommissionMessage("佣金已划转到账户余额。");
      setTransferAmount("");
      setConfirmingAction(null);
      await load();
    } catch (reason) {
      setError(toPublicUiError(reason).message);
    } finally {
      setCommissionPending(null);
    }
  };

  const withdrawCommission = async () => {
    if (commissionPending !== null) return;
    if (withdrawMethod === "" || withdrawAccount.trim() === "") {
      setError("请选择提现方式并填写提现账号。");
      return;
    }
    setCommissionPending("withdraw");
    setError(null);
    setCommissionMessage(null);
    try {
      await services.withdrawCommission(
        withdrawMethod,
        withdrawAccount.trim(),
      );
      setCommissionMessage("提现申请已提交，请等待客服审核。");
      setWithdrawAccount("");
      setConfirmingAction(null);
    } catch (reason) {
      setError(toPublicUiError(reason).message);
    } finally {
      setCommissionPending(null);
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
          <button
            type="button"
            className="secondary-action"
            disabled={sharing || loading || generating}
            onClick={() => void shareInvite()}
          >
            {sharing ? (
              <RefreshCw className="spinning" aria-hidden="true" />
            ) : shareCopied ? (
              <Check aria-hidden="true" />
            ) : (
              <Share2 aria-hidden="true" />
            )}
            {sharing ? "正在复制" : shareCopied ? "已复制" : "复制推荐文案"}
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
            className="commission-section"
            aria-labelledby="commission-title"
          >
            <header>
              <div>
                <span>佣金管理</span>
                <h3 id="commission-title">划转与提现</h3>
              </div>
            </header>

            {commissionMessage !== null && (
              <div className="inline-notice" role="status">
                <span>{commissionMessage}</span>
              </div>
            )}

            <div className="commission-cards">
              <div className="commission-card">
                <div className="commission-card-heading">
                  <ArrowLeftRight aria-hidden="true" />
                  <div>
                    <strong>划转至余额</strong>
                    <small>佣金转入账户余额，可用于购买套餐</small>
                  </div>
                </div>
                <div className="commission-form">
                  <div className="input-shell">
                    <input
                      type="text"
                      inputMode="decimal"
                      value={transferAmount}
                      placeholder={`可划转 ${formatMoney(center.stats.totalCommission)}`}
                      disabled={commissionPending !== null}
                      aria-label="划转金额（元）"
                      onChange={(event) =>
                        setTransferAmount(event.target.value)
                      }
                    />
                  </div>
                  <button
                    type="button"
                    className="secondary-action"
                    disabled={commissionPending !== null || availableMinor <= 0}
                    onClick={() =>
                      setTransferAmount((availableMinor / 100).toFixed(2))
                    }
                  >
                    全部
                  </button>
                  <button
                    type="button"
                    className="primary-action"
                    disabled={commissionPending !== null || availableMinor <= 0}
                    onClick={() => setConfirmingAction("transfer")}
                  >
                    划转
                  </button>
                </div>
              </div>

              {config !== null &&
                !config.withdrawClosed &&
                config.withdrawMethods.length > 0 && (
                  <div className="commission-card">
                    <div className="commission-card-heading">
                      <Banknote aria-hidden="true" />
                      <div>
                        <strong>申请提现</strong>
                        <small>提交后由客服审核打款</small>
                      </div>
                    </div>
                    <div className="commission-form">
                      <select
                        className="commission-select"
                        value={withdrawMethod}
                        disabled={commissionPending !== null}
                        aria-label="提现方式"
                        onChange={(event) =>
                          setWithdrawMethod(event.target.value)
                        }
                      >
                        {config.withdrawMethods.map((method) => (
                          <option key={method} value={method}>
                            {method}
                          </option>
                        ))}
                      </select>
                      <div className="input-shell">
                        <input
                          type="text"
                          value={withdrawAccount}
                          placeholder="提现账号"
                          disabled={commissionPending !== null}
                          aria-label="提现账号"
                          onChange={(event) =>
                            setWithdrawAccount(event.target.value)
                          }
                        />
                      </div>
                      <button
                        type="button"
                        className="primary-action"
                        disabled={
                          commissionPending !== null ||
                          withdrawMethod === "" ||
                          withdrawAccount.trim() === ""
                        }
                        onClick={() => setConfirmingAction("withdraw")}
                      >
                        提现
                      </button>
                    </div>
                  </div>
                )}
            </div>
          </section>

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

      {confirmingAction === "transfer" && (
        <ConfirmDialog
          title="确认划转佣金"
          detail={`将 ${transferAmount || "0"} 元佣金划转到账户余额，划转后可用于购买套餐。`}
          confirmLabel={
            commissionPending === "transfer" ? "正在划转" : "确认划转"
          }
          cancelLabel="取消"
          busy={commissionPending === "transfer"}
          error={error}
          onConfirm={() => void transferCommission()}
          onCancel={() => {
            if (commissionPending !== "transfer") setConfirmingAction(null);
          }}
        />
      )}

      {confirmingAction === "withdraw" && (
        <ConfirmDialog
          title="确认申请提现"
          detail={`通过${withdrawMethod}提现累计佣金到账号 ${withdrawAccount}，提交后由客服审核打款。`}
          confirmLabel={
            commissionPending === "withdraw" ? "正在提交" : "确认提现"
          }
          cancelLabel="取消"
          busy={commissionPending === "withdraw"}
          error={error}
          onConfirm={() => void withdrawCommission()}
          onCancel={() => {
            if (commissionPending !== "withdraw") setConfirmingAction(null);
          }}
        />
      )}
    </main>
  );
}

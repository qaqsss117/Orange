import {
  AlertCircle,
  CalendarDays,
  ChevronRight,
  CreditCard,
  ReceiptText,
  RefreshCw,
} from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { Link } from "react-router-dom";
import type { Money, OrderSummary } from "../businessApi";
import { toPublicUiError, type ShellServices } from "../shellServices";

const STATUS_LABELS: Record<OrderSummary["status"], string> = {
  pending: "待支付",
  paid: "已支付",
  cancelled: "已取消",
  closed: "已关闭",
  refunded: "已退款",
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

function formatBillingPeriod(days: number | null): string {
  if (days === null) return "未知周期";
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

function formatDate(value: number): string {
  return new Intl.DateTimeFormat("zh-CN", {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  }).format(new Date(value));
}

export function OrdersPage({ services }: { services: ShellServices }) {
  const [orders, setOrders] = useState<OrderSummary[] | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const response = await services.fetchOrders();
      setOrders(response.orders);
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
    <main className="management-page orders-page">
      <header className="management-heading">
        <div>
          <span>账户服务</span>
          <h2>我的订单</h2>
          <p>查看套餐订单的金额、状态和时间。</p>
        </div>
        <button
          type="button"
          className="secondary-action"
          disabled={loading}
          onClick={() => void load()}
        >
          <RefreshCw className={loading ? "spinning" : ""} aria-hidden="true" />
          {loading ? "正在刷新" : "刷新订单"}
        </button>
      </header>

      {loading && orders === null ? (
        <div className="page-state" role="status">
          <RefreshCw className="spinning" aria-hidden="true" />
          <span>正在读取订单</span>
        </div>
      ) : error !== null && orders === null ? (
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
      ) : orders?.length === 0 ? (
        <div className="page-state">
          <ReceiptText aria-hidden="true" />
          <strong>暂无订单</strong>
        </div>
      ) : (
        <>
          {error !== null && (
            <div className="inline-notice inline-notice-error" role="alert">
              <AlertCircle aria-hidden="true" />
              <span>{error}</span>
            </div>
          )}
          <div className="order-list">
            {orders?.map((order) => (
              <Link
                className="order-row"
                key={order.orderId}
                to={`/orders/${encodeURIComponent(order.orderId)}`}
                aria-label={`查看 ${order.planName} 订单详情`}
              >
                <header>
                  <div>
                    <span>套餐订单</span>
                    <h3>{order.planName}</h3>
                  </div>
                  <strong className={`order-status status-${order.status}`}>
                    {STATUS_LABELS[order.status]}
                  </strong>
                </header>
                <dl className="order-facts">
                  <div>
                    <dt>
                      <CreditCard aria-hidden="true" />
                      订单金额
                    </dt>
                    <dd>{formatMoney(order.amount)}</dd>
                  </div>
                  <div>
                    <dt>
                      <ReceiptText aria-hidden="true" />
                      计费周期
                    </dt>
                    <dd>{formatBillingPeriod(order.billingPeriodDays)}</dd>
                  </div>
                  <div>
                    <dt>
                      <CalendarDays aria-hidden="true" />
                      创建时间
                    </dt>
                    <dd>{formatDate(order.createdAtUnixMs)}</dd>
                  </div>
                </dl>
                <footer>
                  <span>订单号 {order.orderId}</span>
                  {order.paidAtUnixMs !== null ? (
                    <span>支付于 {formatDate(order.paidAtUnixMs)}</span>
                  ) : (
                    <span className="order-open">
                      查看详情
                      <ChevronRight aria-hidden="true" />
                    </span>
                  )}
                </footer>
              </Link>
            ))}
          </div>
        </>
      )}
    </main>
  );
}

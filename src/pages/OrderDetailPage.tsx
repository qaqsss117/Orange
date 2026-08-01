import {
  AlertCircle,
  ArrowLeft,
  CalendarDays,
  Clock3,
  CreditCard,
  Gauge,
  Package,
  ReceiptText,
  RefreshCw,
} from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { Link, useParams } from "react-router-dom";
import type { Money, OrderDetail } from "../businessApi";
import { toPublicUiError, type ShellServices } from "../shellServices";

const STATUS_LABELS: Record<OrderDetail["status"], string> = {
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

function formatTraffic(value: number | null): string {
  if (value === null) return "不限流量";
  const gibibytes = value / (1024 * 1024 * 1024);
  return `${new Intl.NumberFormat("zh-CN", { maximumFractionDigits: 1 }).format(gibibytes)} GB`;
}

export function OrderDetailPage({ services }: { services: ShellServices }) {
  const { orderId = "" } = useParams();
  const [order, setOrder] = useState<OrderDetail | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const response = await services.fetchOrderDetail(orderId);
      setOrder(response.order);
    } catch (reason) {
      setError(toPublicUiError(reason).message);
    } finally {
      setLoading(false);
    }
  }, [orderId, services]);

  useEffect(() => {
    void load();
  }, [load]);

  return (
    <main className="management-page order-detail-page">
      <header className="management-heading order-detail-heading">
        <div>
          <Link className="order-back-link" to="/orders">
            <ArrowLeft aria-hidden="true" />
            返回订单
          </Link>
          <span>订单服务</span>
          <h2>订单详情</h2>
          <p>订单号 {orderId}</p>
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

      {loading && order === null ? (
        <div className="page-state" role="status">
          <RefreshCw className="spinning" aria-hidden="true" />
          <span>正在读取订单详情</span>
        </div>
      ) : error !== null && order === null ? (
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
      ) : order !== null ? (
        <>
          {error !== null && (
            <div className="inline-notice inline-notice-error" role="alert">
              <AlertCircle aria-hidden="true" />
              <span>{error}</span>
            </div>
          )}
          <section className="order-detail-overview">
            <div>
              <span>套餐订单</span>
              <h3>{order.planName}</h3>
              <p>套餐编号 {order.planId}</p>
            </div>
            <div className="order-detail-total">
              <span>订单金额</span>
              <strong>{formatMoney(order.amount)}</strong>
              <em className={`order-status status-${order.status}`}>
                {STATUS_LABELS[order.status]}
              </em>
            </div>
          </section>

          <dl className="order-detail-facts">
            <div>
              <dt>
                <ReceiptText aria-hidden="true" />
                订单号
              </dt>
              <dd>{order.orderId}</dd>
            </div>
            <div>
              <dt>
                <Package aria-hidden="true" />
                计费周期
              </dt>
              <dd>{formatBillingPeriod(order.billingPeriodDays)}</dd>
            </div>
            <div>
              <dt>
                <Gauge aria-hidden="true" />
                套餐流量
              </dt>
              <dd>{formatTraffic(order.trafficBytes)}</dd>
            </div>
            <div>
              <dt>
                <CreditCard aria-hidden="true" />
                支付状态
              </dt>
              <dd>{STATUS_LABELS[order.status]}</dd>
            </div>
            <div>
              <dt>
                <CalendarDays aria-hidden="true" />
                创建时间
              </dt>
              <dd>{formatDate(order.createdAtUnixMs)}</dd>
            </div>
            {order.updatedAtUnixMs !== null && (
              <div>
                <dt>
                  <Clock3 aria-hidden="true" />
                  更新时间
                </dt>
                <dd>{formatDate(order.updatedAtUnixMs)}</dd>
              </div>
            )}
            {order.paidAtUnixMs !== null && (
              <div>
                <dt>
                  <CreditCard aria-hidden="true" />
                  支付时间
                </dt>
                <dd>{formatDate(order.paidAtUnixMs)}</dd>
              </div>
            )}
          </dl>
        </>
      ) : null}
    </main>
  );
}

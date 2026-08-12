import {
  AlertCircle,
  ArrowLeft,
  CalendarDays,
  Clock3,
  CreditCard,
  Gauge,
  LoaderCircle,
  Package,
  QrCode,
  ReceiptText,
  RefreshCw,
  XCircle,
} from "lucide-react";
import { QRCodeSVG } from "qrcode.react";
import { useCallback, useEffect, useState } from "react";
import { Link, useParams } from "react-router-dom";
import type { Money, OrderDetail, PaymentMethod } from "../businessApi";
import { toPublicUiError, type ShellServices } from "../shellServices";
import { ConfirmDialog } from "../ui/AsyncState";

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
  const [paymentMethods, setPaymentMethods] = useState<PaymentMethod[] | null>(
    null,
  );
  const [selectedPaymentMethod, setSelectedPaymentMethod] = useState<
    string | null
  >(null);
  const [paymentsLoading, setPaymentsLoading] = useState(false);
  const [paymentError, setPaymentError] = useState<string | null>(null);
  const [checkoutLoading, setCheckoutLoading] = useState(false);
  const [qrCode, setQrCode] = useState<string | null>(null);
  const [cancelDialogOpen, setCancelDialogOpen] = useState(false);
  const [cancelLoading, setCancelLoading] = useState(false);
  const [cancelError, setCancelError] = useState<string | null>(null);

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const response = await services.fetchOrderDetail(orderId);
      setOrder(response.order);
      if (response.order.status !== "pending") {
        setPaymentMethods(null);
        setSelectedPaymentMethod(null);
      }
    } catch (reason) {
      setError(toPublicUiError(reason).message);
    } finally {
      setLoading(false);
    }
  }, [orderId, services]);

  const loadPaymentMethods = useCallback(async () => {
    setPaymentsLoading(true);
    setPaymentError(null);
    try {
      const response = await services.fetchPaymentMethods();
      setPaymentMethods(response.paymentMethods);
      setSelectedPaymentMethod((current) =>
        response.paymentMethods.some(
          (method) => method.paymentMethodId === current,
        )
          ? current
          : (response.paymentMethods[0]?.paymentMethodId ?? null),
      );
    } catch (reason) {
      setPaymentError(toPublicUiError(reason).message);
    } finally {
      setPaymentsLoading(false);
    }
  }, [services]);

  const startCheckout = useCallback(async () => {
    if (selectedPaymentMethod === null || checkoutLoading) return;
    setCheckoutLoading(true);
    setPaymentError(null);
    setQrCode(null);
    try {
      const response = await services.checkoutOrder(
        orderId,
        selectedPaymentMethod,
      );
      setQrCode(response.qrCode);
      if (response.qrCode === null) {
        await load();
      }
    } catch (reason) {
      setPaymentError(toPublicUiError(reason).message);
    } finally {
      setCheckoutLoading(false);
    }
  }, [checkoutLoading, load, orderId, selectedPaymentMethod, services]);

  const confirmCancellation = useCallback(async () => {
    if (cancelLoading) return;
    setCancelLoading(true);
    setCancelError(null);
    try {
      const response = await services.cancelOrder(orderId);
      setOrder((current) =>
        current !== null && current.orderId === response.orderId
          ? { ...current, status: response.status }
          : current,
      );
      setPaymentMethods(null);
      setSelectedPaymentMethod(null);
      setCancelDialogOpen(false);
      await load();
    } catch (reason) {
      setCancelError(toPublicUiError(reason).message);
    } finally {
      setCancelLoading(false);
    }
  }, [cancelLoading, load, orderId, services]);

  useEffect(() => {
    setOrder(null);
    setError(null);
    setPaymentMethods(null);
    setSelectedPaymentMethod(null);
    setPaymentError(null);
    setQrCode(null);
    setCancelDialogOpen(false);
    setCancelError(null);
  }, [orderId]);

  useEffect(() => {
    void load();
  }, [load]);

  useEffect(() => {
    if (order?.status === "pending" && paymentMethods === null) {
      void loadPaymentMethods();
    }
  }, [loadPaymentMethods, order?.status, paymentMethods]);

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
        <div className="order-detail-actions">
          <button
            type="button"
            className="secondary-action"
            disabled={loading || cancelLoading}
            onClick={() => void load()}
          >
            <RefreshCw
              className={loading ? "spinning" : ""}
              aria-hidden="true"
            />
            {loading ? "正在刷新" : "刷新详情"}
          </button>
          {order?.status === "pending" && (
            <button
              type="button"
              className="danger-action"
              disabled={cancelLoading || checkoutLoading}
              onClick={() => {
                setCancelError(null);
                setCancelDialogOpen(true);
              }}
            >
              <XCircle aria-hidden="true" />
              取消订单
            </button>
          )}
        </div>
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

          {order.status === "pending" && (
            <section className="payment-section">
              <header>
                <div>
                  <span>订单支付</span>
                  <h3>选择支付方式</h3>
                </div>
                <CreditCard aria-hidden="true" />
              </header>

              {paymentsLoading && paymentMethods === null ? (
                <div className="page-state compact" role="status">
                  <LoaderCircle className="spinning" aria-hidden="true" />
                  <span>正在读取支付方式</span>
                </div>
              ) : paymentError !== null && paymentMethods === null ? (
                <div className="inline-notice inline-notice-error" role="alert">
                  <AlertCircle aria-hidden="true" />
                  <span>{paymentError}</span>
                  <button
                    type="button"
                    className="inline-action"
                    onClick={() => void loadPaymentMethods()}
                  >
                    重试
                  </button>
                </div>
              ) : paymentMethods?.length === 0 ? (
                <div className="page-state compact">
                  <CreditCard aria-hidden="true" />
                  <span>暂无可用支付方式</span>
                </div>
              ) : (
                <>
                  <fieldset className="payment-method-list">
                    <legend>支付渠道</legend>
                    {paymentMethods?.map((method) => (
                      <label
                        className="payment-method-option"
                        key={method.paymentMethodId}
                      >
                        <input
                          type="radio"
                          name="payment-method"
                          value={method.paymentMethodId}
                          checked={
                            selectedPaymentMethod === method.paymentMethodId
                          }
                          disabled={checkoutLoading}
                          onChange={() => {
                            setSelectedPaymentMethod(method.paymentMethodId);
                            setQrCode(null);
                          }}
                        />
                        <span>
                          <strong>{method.name}</strong>
                          <small>
                            {Number(method.handlingFeePercent) === 0
                              ? "无手续费"
                              : `手续费 ${method.handlingFeePercent}%`}
                          </small>
                        </span>
                      </label>
                    ))}
                  </fieldset>

                  {paymentError !== null && (
                    <div
                      className="inline-notice inline-notice-error"
                      role="alert"
                    >
                      <AlertCircle aria-hidden="true" />
                      <span>{paymentError}</span>
                    </div>
                  )}
                  {qrCode !== null && (
                    <div className="payment-qr" role="status">
                      <div className="payment-qr-code">
                        <QRCodeSVG
                          value={qrCode}
                          size={224}
                          level="M"
                          marginSize={2}
                          title="订单支付二维码"
                        />
                      </div>
                      <div>
                        <QrCode aria-hidden="true" />
                        <strong>扫码支付</strong>
                        <span>完成支付后刷新订单状态</span>
                      </div>
                    </div>
                  )}
                  <button
                    type="button"
                    className="primary-action payment-action"
                    disabled={selectedPaymentMethod === null || checkoutLoading}
                    onClick={() => void startCheckout()}
                  >
                    {checkoutLoading ? (
                      <LoaderCircle className="spinning" aria-hidden="true" />
                    ) : (
                      <QrCode aria-hidden="true" />
                    )}
                    {checkoutLoading
                      ? "正在获取"
                      : qrCode === null
                        ? "获取支付二维码"
                        : "刷新支付二维码"}
                  </button>
                </>
              )}
            </section>
          )}

          {cancelDialogOpen && (
            <ConfirmDialog
              title="取消订单"
              detail={`确认取消订单 ${order.orderId}？取消后需要重新创建订单。`}
              confirmLabel={cancelLoading ? "正在取消" : "确认取消"}
              cancelLabel="返回"
              busy={cancelLoading}
              error={cancelError}
              onConfirm={() => void confirmCancellation()}
              onCancel={() => {
                if (!cancelLoading) {
                  setCancelDialogOpen(false);
                  setCancelError(null);
                }
              }}
            />
          )}
        </>
      ) : null}
    </main>
  );
}

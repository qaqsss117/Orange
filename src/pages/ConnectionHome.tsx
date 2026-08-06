import { useEffect, useRef, useState } from "react";
import type { LucideIcon } from "lucide-react";
import {
  CircleAlert,
  ChevronRight,
  CircleHelp,
  Download,
  Globe2,
  LoaderCircle,
  Power,
  Route,
  Server,
  ShieldAlert,
  ShieldCheck,
  Upload,
} from "lucide-react";
import { Link } from "react-router-dom";
import orangeIcon from "../../assets/product/brand/orange-development-mark.png";
import { DataPlaneEventConsumer, type TrafficSample } from "../events";
import {
  parseCommandError,
  type DataPlaneControlAction,
  type DataPlaneState,
  type NodeCatalogResponse,
  type PublicNodeProtocol,
  type RoutingMode,
} from "../ipc";
import type { SubscriptionStatus } from "../businessApi";
import type { ShellServices } from "../shellServices";
import { parseNodeName } from "../ui/nodeRegion";
import { UI_TEXT } from "../uiContent";

const DATA_PLANE_UI_POLL_INTERVAL_MS = 500;

const NODE_PROTOCOL_LABELS: Record<PublicNodeProtocol, string> = {
  shadowsocks: "Shadowsocks",
  trojan: "Trojan",
  hysteria2: "Hysteria 2",
  vless: "VLESS",
};

const ROUTING_MODE_LABELS: Record<RoutingMode, string> = {
  smart: UI_TEXT.smartRoute,
  global: UI_TEXT.globalRoute,
  direct: UI_TEXT.directRoute,
};

const ZERO_TRAFFIC: TrafficSample = {
  uploadBytesTotal: 0,
  downloadBytesTotal: 0,
  uploadBytesPerSecond: 0,
  downloadBytesPerSecond: 0,
};

const STATE_PRESENTATION: Record<
  DataPlaneState,
  { label: string; detail: string; icon: LucideIcon }
> = {
  unconfigured: {
    label: UI_TEXT.disconnected,
    detail: UI_TEXT.waitingForConfiguration,
    icon: Power,
  },
  validating: {
    label: UI_TEXT.validatingConfiguration,
    detail: UI_TEXT.validatingConfigurationDetail,
    icon: LoaderCircle,
  },
  permission_required: {
    label: UI_TEXT.permissionRequired,
    detail: UI_TEXT.permissionRequiredDetail,
    icon: ShieldAlert,
  },
  starting: {
    label: UI_TEXT.connecting,
    detail: UI_TEXT.connectingDetail,
    icon: LoaderCircle,
  },
  online: {
    label: UI_TEXT.connected,
    detail: UI_TEXT.connectedDetail,
    icon: ShieldCheck,
  },
  stopping: {
    label: UI_TEXT.disconnecting,
    detail: UI_TEXT.disconnectingDetail,
    icon: LoaderCircle,
  },
  failed: {
    label: UI_TEXT.connectionFailed,
    detail: UI_TEXT.connectionFailedDetail,
    icon: CircleAlert,
  },
  rollback: {
    label: UI_TEXT.restoringConnection,
    detail: UI_TEXT.restoringConnectionDetail,
    icon: LoaderCircle,
  },
};

interface TelemetryState {
  dataPlane: DataPlaneState;
  canStart: boolean;
  canStop: boolean;
  loading: boolean;
  stateUnavailable: boolean;
  trafficUnavailable: boolean;
  traffic: TrafficSample;
  subscriptionStatus: SubscriptionStatus | null;
  subscriptionExpiringSoonDays: number | null;
  subscriptionUsageRatio: number | null;
}

type SelectedNodeState =
  | { status: "loading" }
  | { status: "error" }
  | { status: "ready"; value: string | null };

type RoutingModeState =
  | { status: "loading" }
  | { status: "error" }
  | { status: "ready"; value: RoutingMode };

const EXPIRING_SOON_THRESHOLD_MS = 3 * 24 * 60 * 60 * 1000;
const DAY_MS = 24 * 60 * 60 * 1000;
const TRAFFIC_LOW_THRESHOLD = 0.9;

function formatExpiryCountdown(days: number): string {
  return `距离到期还剩 ${days} 天，请及时续费以免影响使用。`;
}

function formatUsageRatio(ratio: number): string {
  const usedPercent = Math.min(999, Math.floor(ratio * 100));
  return `当前流量已使用 ${usedPercent}%，建议提前续费或购买流量包。`;
}

function subscriptionSnapshotFields(snapshot: {
  subscription: {
    expiresAtUnixMs: number | null;
    usedBytes: number;
    totalBytes: number | null;
  } | null;
}): { expiringSoonDays: number | null; usageRatio: number | null } {
  const subscription = snapshot.subscription;
  if (subscription === null || subscription === undefined) {
    return { expiringSoonDays: null, usageRatio: null };
  }
  const total = subscription.totalBytes;
  const expiresAt = subscription.expiresAtUnixMs;
  const remainingMs = expiresAt === null ? null : expiresAt - Date.now();
  return {
    expiringSoonDays:
      remainingMs !== null &&
      remainingMs > 0 &&
      remainingMs <= EXPIRING_SOON_THRESHOLD_MS
        ? Math.ceil(remainingMs / DAY_MS)
        : null,
    usageRatio:
      total === null || total === 0 ? null : subscription.usedBytes / total,
  };
}

function selectedNodeLabel(catalog: NodeCatalogResponse): string | null {
  const selections = catalog.groups.flatMap((group) => {
    const node = group.nodes.find(
      (candidate) => candidate.id === group.selectedNodeId,
    );
    return node === undefined ? [] : [node];
  });
  const primary = selections[0];
  if (primary === undefined) return null;
  const primaryName = parseNodeName(primary.name).displayName;
  if (selections.length > 1) {
    return `${primaryName} 等 ${selections.length} 个策略组`;
  }
  return `${primaryName} · ${NODE_PROTOCOL_LABELS[primary.protocol]}`;
}

function formatTrafficRate(bytesPerSecond: number): string {
  if (bytesPerSecond < 1_024) {
    return `${bytesPerSecond} B/s`;
  }
  const units = ["KiB/s", "MiB/s", "GiB/s", "TiB/s"] as const;
  let value = bytesPerSecond / 1_024;
  let unitIndex = 0;
  while (value >= 1_024 && unitIndex < units.length - 1) {
    value /= 1_024;
    unitIndex += 1;
  }
  const digits = value >= 100 ? 0 : value >= 10 ? 1 : 2;
  const formatted = value
    .toFixed(digits)
    .replace(/(\.\d*?[1-9])0+$|\.0+$/, "$1");
  return `${formatted} ${units[unitIndex]}`;
}

function ConnectionMetric({
  icon: Icon,
  label,
  value,
}: {
  icon: LucideIcon;
  label: string;
  value: string;
}) {
  return (
    <div className="connection-metric">
      <span className="metric-label">
        <Icon aria-hidden="true" />
        {label}
      </span>
      <strong>{value}</strong>
    </div>
  );
}

function DetailRow({
  icon: Icon,
  label,
  value,
  to,
}: {
  icon: LucideIcon;
  label: string;
  value: string;
  to?: string;
}) {
  const content = (
    <>
      <span className="detail-icon" aria-hidden="true">
        <Icon />
      </span>
      <span className="detail-copy">
        <span>{label}</span>
        <strong>{value}</strong>
      </span>
      <ChevronRight className="detail-chevron" aria-hidden="true" />
    </>
  );
  return to === undefined ? (
    <div className="detail-row" aria-disabled="true">
      {content}
    </div>
  ) : (
    <Link className="detail-row detail-row-link" to={to}>
      {content}
    </Link>
  );
}

export function ConnectionHome({ services }: { services: ShellServices }) {
  const consumer = useRef(new DataPlaneEventConsumer());
  const operationInFlight = useRef(false);
  const componentActive = useRef(true);
  const [operationPending, setOperationPending] = useState(false);
  const [actionError, setActionError] = useState<string | null>(null);
  const [selectedNode, setSelectedNode] = useState<SelectedNodeState>({
    status: "loading",
  });
  const [routingMode, setRoutingMode] = useState<RoutingModeState>({
    status: "loading",
  });
  const [telemetry, setTelemetry] = useState<TelemetryState>({
    dataPlane: "unconfigured",
    canStart: false,
    canStop: false,
    loading: true,
    stateUnavailable: false,
    trafficUnavailable: false,
    traffic: { ...ZERO_TRAFFIC },
    subscriptionStatus: null,
    subscriptionExpiringSoonDays: null,
    subscriptionUsageRatio: null,
  });

  useEffect(() => {
    componentActive.current = true;
    let active = true;
    let timer: number | undefined;
    const poll = async () => {
      const [controlResult, eventResult, subscriptionResult] =
        await Promise.allSettled([
          services.controlDataPlane("status"),
          services.getDataPlaneEventSnapshot(),
          services.getSubscriptionSnapshot(),
        ]);
      if (!active) {
        return;
      }
      if (controlResult.status === "rejected") {
        setTelemetry((current) => ({
          ...current,
          canStart: false,
          canStop: false,
          loading: false,
          stateUnavailable: true,
          trafficUnavailable: true,
          subscriptionStatus:
            subscriptionResult.status === "fulfilled"
              ? (subscriptionResult.value.subscription?.status ?? null)
              : current.subscriptionStatus,
          subscriptionExpiringSoonDays:
            subscriptionResult.status === "fulfilled"
              ? subscriptionSnapshotFields(subscriptionResult.value)
                  .expiringSoonDays
              : current.subscriptionExpiringSoonDays,
          subscriptionUsageRatio:
            subscriptionResult.status === "fulfilled"
              ? subscriptionSnapshotFields(subscriptionResult.value).usageRatio
              : current.subscriptionUsageRatio,
          traffic: {
            ...current.traffic,
            uploadBytesPerSecond: 0,
            downloadBytesPerSecond: 0,
          },
        }));
      } else {
        const dataPlane = controlResult.value.dataPlane;
        const consumed =
          eventResult.status === "fulfilled"
            ? consumer.current.consume(eventResult.value, dataPlane)
            : null;
        setTelemetry({
          dataPlane,
          canStart: controlResult.value.canStart,
          canStop: controlResult.value.canStop,
          loading: false,
          stateUnavailable: false,
          trafficUnavailable: consumed === null,
          traffic: consumed?.traffic ?? { ...ZERO_TRAFFIC },
          subscriptionStatus:
            subscriptionResult.status === "fulfilled"
              ? (subscriptionResult.value.subscription?.status ?? null)
              : null,
          subscriptionExpiringSoonDays:
            subscriptionResult.status === "fulfilled"
              ? subscriptionSnapshotFields(subscriptionResult.value)
                  .expiringSoonDays
              : null,
          subscriptionUsageRatio:
            subscriptionResult.status === "fulfilled"
              ? subscriptionSnapshotFields(subscriptionResult.value).usageRatio
              : null,
        });
      }
      if (active) {
        timer = window.setTimeout(poll, DATA_PLANE_UI_POLL_INTERVAL_MS);
      }
    };
    void poll();
    return () => {
      active = false;
      componentActive.current = false;
      if (timer !== undefined) {
        window.clearTimeout(timer);
      }
    };
  }, [services]);

  useEffect(() => {
    let active = true;
    void Promise.allSettled([
      services.getNodeCatalog(),
      services.getRoutingMode(),
    ]).then(([catalogResult, routingResult]) => {
      if (!active) return;
      setSelectedNode(
        catalogResult.status === "fulfilled"
          ? {
              status: "ready",
              value: selectedNodeLabel(catalogResult.value),
            }
          : { status: "error" },
      );
      setRoutingMode(
        routingResult.status === "fulfilled"
          ? { status: "ready", value: routingResult.value.mode }
          : { status: "error" },
      );
    });
    return () => {
      active = false;
    };
  }, [services]);

  const action: DataPlaneControlAction | null = telemetry.canStop
    ? "stop"
    : telemetry.canStart
      ? "start"
      : null;

  const runConnectionAction = async () => {
    if (action === null || operationInFlight.current) {
      return;
    }
    operationInFlight.current = true;
    setOperationPending(true);
    setActionError(null);
    try {
      const response = await services.controlDataPlane(action);
      if (!componentActive.current) {
        return;
      }
      setTelemetry((current) => ({
        ...current,
        dataPlane: response.dataPlane,
        canStart: response.canStart,
        canStop: response.canStop,
        loading: false,
        stateUnavailable: false,
        traffic:
          response.dataPlane === "online"
            ? current.traffic
            : { ...ZERO_TRAFFIC },
      }));
    } catch (error) {
      if (!componentActive.current) {
        return;
      }
      try {
        setActionError(parseCommandError(error).message);
      } catch {
        setActionError(UI_TEXT.connectionActionFailed);
      }
    } finally {
      operationInFlight.current = false;
      if (componentActive.current) {
        setOperationPending(false);
      }
    }
  };

  const presentation = telemetry.stateUnavailable
    ? {
        label: UI_TEXT.connectionStateUnavailable,
        detail: UI_TEXT.connectionStateUnavailableDetail,
        icon: CircleAlert,
      }
    : telemetry.loading
      ? {
          label: UI_TEXT.readingConnection,
          detail: UI_TEXT.waitingForConfiguration,
          icon: LoaderCircle,
        }
      : STATE_PRESENTATION[telemetry.dataPlane];
  const StatusIcon = presentation.icon;
  const controlLabel = operationPending
    ? UI_TEXT.connectionActionPending
    : action === "stop"
      ? UI_TEXT.disconnect
      : action === "start"
        ? telemetry.dataPlane === "failed"
          ? UI_TEXT.retryConnection
          : UI_TEXT.connection
        : telemetry.loading
          ? UI_TEXT.readingConnection
          : UI_TEXT.connectUnavailable;
  const stateDetail =
    telemetry.dataPlane === "online" &&
    telemetry.subscriptionStatus === "expired"
      ? UI_TEXT.connectedWithExpiredSubscription
      : telemetry.dataPlane === "online" &&
          telemetry.subscriptionStatus === "exhausted"
        ? UI_TEXT.connectedWithExhaustedSubscription
        : !telemetry.loading &&
            !telemetry.stateUnavailable &&
            telemetry.dataPlane === "online" &&
            telemetry.trafficUnavailable
          ? UI_TEXT.connectedTrafficUnavailableDetail
          : presentation.detail;
  const uploadRate = formatTrafficRate(telemetry.traffic.uploadBytesPerSecond);
  const downloadRate = formatTrafficRate(
    telemetry.traffic.downloadBytesPerSecond,
  );
  const visualState = telemetry.stateUnavailable
    ? "unavailable"
    : telemetry.loading
      ? "loading"
      : telemetry.dataPlane;
  const hasConfiguration =
    !telemetry.loading &&
    !telemetry.stateUnavailable &&
    (telemetry.canStart || telemetry.dataPlane !== "unconfigured");
  const expiringSoonDays =
    telemetry.subscriptionStatus === "active" ||
    telemetry.subscriptionStatus === "trial"
      ? telemetry.subscriptionExpiringSoonDays
      : null;
  const trafficLow =
    (telemetry.subscriptionStatus === "active" ||
      telemetry.subscriptionStatus === "trial") &&
    telemetry.subscriptionUsageRatio !== null &&
    telemetry.subscriptionUsageRatio >= TRAFFIC_LOW_THRESHOLD;
  const subscriptionPresentation = telemetry.subscriptionStatus === "expired"
    ? {
        title: UI_TEXT.subscriptionExpired,
        detail: UI_TEXT.subscriptionExpiredDetail,
        renew: true,
      }
    : telemetry.subscriptionStatus === "exhausted"
      ? {
          title: UI_TEXT.subscriptionExhausted,
          detail: UI_TEXT.subscriptionExhaustedDetail,
          renew: true,
        }
      : expiringSoonDays !== null
        ? {
            title: UI_TEXT.subscriptionExpiringSoon,
            detail: formatExpiryCountdown(expiringSoonDays),
            renew: true,
          }
        : trafficLow
          ? {
              title: UI_TEXT.subscriptionTrafficLow,
              detail:
                telemetry.subscriptionUsageRatio === null
                  ? ""
                  : formatUsageRatio(telemetry.subscriptionUsageRatio),
              renew: true,
            }
          : hasConfiguration
            ? {
                title: UI_TEXT.subscriptionReady,
                detail: UI_TEXT.subscriptionReadyDetail,
                renew: false,
              }
            : {
                title: UI_TEXT.subscriptionEmpty,
                detail: UI_TEXT.subscriptionEmptyDetail,
                renew: false,
              };

  return (
    <main className="dashboard">
      <section className="subscription-banner" aria-labelledby="banner-title">
        <div className="banner-copy">
          <span>{UI_TEXT.subscriptionStatus}</span>
          <h2 id="banner-title">{subscriptionPresentation.title}</h2>
          <p>{subscriptionPresentation.detail}</p>
          {subscriptionPresentation.renew && (
            <Link className="banner-action" to="/subscription">
              {UI_TEXT.subscriptionRenewAction}
            </Link>
          )}
        </div>
        <img src={orangeIcon} alt="" aria-hidden="true" />
      </section>

      <div className="connection-layout">
        <section
          className="connection-zone"
          aria-labelledby="connection-status"
          data-state={visualState}
        >
          <div className="connection-metrics connection-metrics-desktop">
            <ConnectionMetric
              icon={Upload}
              label={UI_TEXT.upload}
              value={uploadRate}
            />
            <ConnectionMetric
              icon={Download}
              label={UI_TEXT.download}
              value={downloadRate}
            />
          </div>

          <button
            type="button"
            className="connection-control"
            aria-label={controlLabel}
            aria-busy={
              operationPending ||
              telemetry.loading ||
              ["validating", "starting", "stopping", "rollback"].includes(
                telemetry.dataPlane,
              )
            }
            disabled={
              operationPending ||
              telemetry.loading ||
              telemetry.stateUnavailable ||
              action === null
            }
            onClick={() => void runConnectionAction()}
          >
            <span className="connection-orbit" aria-hidden="true" />
            <span className="connection-core" aria-hidden="true">
              <Power />
            </span>
            <span>{controlLabel}</span>
          </button>

          <div className="connection-state" aria-live="polite">
            <span className="state-symbol" aria-hidden="true">
              <StatusIcon />
            </span>
            <div>
              <strong id="connection-status">{presentation.label}</strong>
              <span role={actionError === null ? undefined : "alert"}>
                {actionError ?? stateDetail}
              </span>
            </div>
          </div>

          <div className="connection-metrics connection-metrics-mobile">
            <ConnectionMetric
              icon={Upload}
              label={UI_TEXT.upload}
              value={uploadRate}
            />
            <ConnectionMetric
              icon={Download}
              label={UI_TEXT.download}
              value={downloadRate}
            />
          </div>
        </section>

        <aside className="connection-details" aria-labelledby="details-title">
          <div className="details-heading">
            <div>
              <span>
                {!hasConfiguration
                  ? UI_TEXT.configurationRequired
                  : UI_TEXT.liveConnectionState}
              </span>
              <h2 id="details-title">{UI_TEXT.connectionOptions}</h2>
            </div>
            <Globe2 aria-hidden="true" />
          </div>
          <div className="details-list">
            <DetailRow
              icon={Route}
              label={UI_TEXT.routeMode}
              value={
                routingMode.status === "loading"
                  ? UI_TEXT.readingRouteMode
                  : routingMode.status === "error"
                    ? UI_TEXT.routeModeUnavailable
                    : ROUTING_MODE_LABELS[routingMode.value]
              }
              to="/settings"
            />
            <DetailRow
              icon={Server}
              label={UI_TEXT.selectedNode}
              value={
                selectedNode.status === "loading"
                  ? UI_TEXT.readingNode
                  : selectedNode.status === "error"
                    ? UI_TEXT.nodeStateUnavailable
                    : (selectedNode.value ?? UI_TEXT.noNodeSelected)
              }
              to="/nodes"
            />
            <DetailRow
              icon={CircleHelp}
              label="问题解答"
              value="无法连接？查看解决方案"
              to="/help"
            />
          </div>
        </aside>
      </div>
    </main>
  );
}

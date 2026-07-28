import { useEffect, useRef, useState } from "react";
import type { LucideIcon } from "lucide-react";
import {
  CircleAlert,
  ChevronRight,
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
import orangeIcon from "../../assets/product/brand/orange-development-mark.png";
import { DataPlaneEventConsumer, type TrafficSample } from "../events";
import type { DataPlaneState } from "../ipc";
import type { ShellServices } from "../shellServices";
import { UI_TEXT } from "../uiContent";

const DATA_PLANE_UI_POLL_INTERVAL_MS = 500;

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
  loading: boolean;
  stateUnavailable: boolean;
  trafficUnavailable: boolean;
  traffic: TrafficSample;
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
}: {
  icon: LucideIcon;
  label: string;
  value: string;
}) {
  return (
    <div className="detail-row" aria-disabled="true">
      <span className="detail-icon" aria-hidden="true">
        <Icon />
      </span>
      <span className="detail-copy">
        <span>{label}</span>
        <strong>{value}</strong>
      </span>
      <ChevronRight className="detail-chevron" aria-hidden="true" />
    </div>
  );
}

export function ConnectionHome({ services }: { services: ShellServices }) {
  const consumer = useRef(new DataPlaneEventConsumer());
  const [telemetry, setTelemetry] = useState<TelemetryState>({
    dataPlane: "unconfigured",
    loading: true,
    stateUnavailable: false,
    trafficUnavailable: false,
    traffic: { ...ZERO_TRAFFIC },
  });

  useEffect(() => {
    let active = true;
    let timer: number | undefined;
    const poll = async () => {
      const [planeResult, eventResult] = await Promise.allSettled([
        services.getPlaneState(),
        services.getDataPlaneEventSnapshot(),
      ]);
      if (!active) {
        return;
      }
      if (planeResult.status === "rejected") {
        setTelemetry((current) => ({
          ...current,
          loading: false,
          stateUnavailable: true,
          trafficUnavailable: true,
          traffic: {
            ...current.traffic,
            uploadBytesPerSecond: 0,
            downloadBytesPerSecond: 0,
          },
        }));
      } else {
        const dataPlane = planeResult.value.dataPlane;
        const consumed =
          eventResult.status === "fulfilled"
            ? consumer.current.consume(eventResult.value, dataPlane)
            : null;
        setTelemetry({
          dataPlane,
          loading: false,
          stateUnavailable: false,
          trafficUnavailable: consumed === null,
          traffic: consumed?.traffic ?? { ...ZERO_TRAFFIC },
        });
      }
      if (active) {
        timer = window.setTimeout(poll, DATA_PLANE_UI_POLL_INTERVAL_MS);
      }
    };
    void poll();
    return () => {
      active = false;
      if (timer !== undefined) {
        window.clearTimeout(timer);
      }
    };
  }, [services]);

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
  const controlLabel = telemetry.loading
    ? UI_TEXT.readingConnection
    : telemetry.stateUnavailable
      ? UI_TEXT.connectUnavailable
      : telemetry.dataPlane === "online"
        ? UI_TEXT.connected
        : presentation.label;
  const stateDetail =
    !telemetry.loading &&
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

  return (
    <main className="dashboard">
      <section className="subscription-banner" aria-labelledby="banner-title">
        <div className="banner-copy">
          <span>{UI_TEXT.subscriptionStatus}</span>
          <h2 id="banner-title">{UI_TEXT.subscriptionEmpty}</h2>
          <p>{UI_TEXT.subscriptionEmptyDetail}</p>
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
              telemetry.loading ||
              ["validating", "starting", "stopping", "rollback"].includes(
                telemetry.dataPlane,
              )
            }
            disabled
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
              <span>{stateDetail}</span>
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
                {telemetry.dataPlane === "unconfigured"
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
              value={UI_TEXT.smartRoute}
            />
            <DetailRow
              icon={Server}
              label={UI_TEXT.selectedNode}
              value={UI_TEXT.noNodeSelected}
            />
          </div>
        </aside>
      </div>
    </main>
  );
}

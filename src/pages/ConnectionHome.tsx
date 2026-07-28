import type { LucideIcon } from "lucide-react";
import {
  ChevronRight,
  Download,
  Globe2,
  Power,
  Route,
  Server,
  ShieldCheck,
  Upload,
} from "lucide-react";
import orangeIcon from "../../assets/product/brand/orange-development-mark.png";
import { UI_TEXT } from "../uiContent";

function ConnectionMetric({
  icon: Icon,
  label,
}: {
  icon: LucideIcon;
  label: string;
}) {
  return (
    <div className="connection-metric">
      <span className="metric-label">
        <Icon aria-hidden="true" />
        {label}
      </span>
      <strong>{UI_TEXT.zeroSpeed}</strong>
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

export function ConnectionHome() {
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
        >
          <div className="connection-metrics connection-metrics-desktop">
            <ConnectionMetric icon={Upload} label={UI_TEXT.upload} />
            <ConnectionMetric icon={Download} label={UI_TEXT.download} />
          </div>

          <button
            type="button"
            className="connection-control"
            aria-label={UI_TEXT.connectUnavailable}
            disabled
          >
            <span className="connection-orbit" aria-hidden="true" />
            <span className="connection-core" aria-hidden="true">
              <Power />
            </span>
            <span>{UI_TEXT.connectUnavailable}</span>
          </button>

          <div className="connection-state" aria-live="polite">
            <span className="state-symbol" aria-hidden="true">
              <ShieldCheck />
            </span>
            <div>
              <strong id="connection-status">{UI_TEXT.disconnected}</strong>
              <span>{UI_TEXT.waitingForConfiguration}</span>
            </div>
          </div>

          <div className="connection-metrics connection-metrics-mobile">
            <ConnectionMetric icon={Upload} label={UI_TEXT.upload} />
            <ConnectionMetric icon={Download} label={UI_TEXT.download} />
          </div>
        </section>

        <aside className="connection-details" aria-labelledby="details-title">
          <div className="details-heading">
            <div>
              <span>{UI_TEXT.configurationRequired}</span>
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

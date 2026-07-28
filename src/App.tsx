import { useState } from "react";
import type { LucideIcon } from "lucide-react";
import {
  Bell,
  ChevronRight,
  Download,
  Globe2,
  Home,
  Layers,
  Moon,
  Power,
  Route,
  Server,
  Settings,
  ShieldCheck,
  Sun,
  Upload,
  User,
} from "lucide-react";
import orangeIcon from "../assets/product/brand/orange-development-mark.png";
import { UI_TEXT } from "./uiContent";
import { readUiPreview, systemTheme, type PreviewTheme } from "./uiPreview";

interface NavigationItem {
  label: string;
  icon: LucideIcon;
  active?: boolean;
}

const NAVIGATION: readonly NavigationItem[] = [
  { label: UI_TEXT.home, icon: Home, active: true },
  { label: UI_TEXT.subscription, icon: Layers },
  { label: UI_TEXT.nodes, icon: Server },
  { label: UI_TEXT.account, icon: User },
  { label: UI_TEXT.settings, icon: Settings },
];

function Navigation({ mobile = false }: { mobile?: boolean }) {
  return (
    <nav
      className={mobile ? "mobile-navigation" : "sidebar-navigation"}
      aria-label={UI_TEXT.navigation}
    >
      <ul>
        {NAVIGATION.map(({ label, icon: Icon, active }) => (
          <li key={label}>
            <span
              className="navigation-item"
              aria-current={active ? "page" : undefined}
            >
              <Icon aria-hidden="true" />
              <span>{label}</span>
            </span>
          </li>
        ))}
      </ul>
    </nav>
  );
}

function Brand({ compact = false }: { compact?: boolean }) {
  return (
    <div className={compact ? "brand brand-compact" : "brand"}>
      <img src={orangeIcon} alt="" aria-hidden="true" />
      <div>
        <strong>{UI_TEXT.brand}</strong>
        {!compact && <span>{UI_TEXT.brandSubtitle}</span>}
      </div>
    </div>
  );
}

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

export default function App() {
  const preview = readUiPreview(window.location.search);
  const [theme, setTheme] = useState<PreviewTheme>(preview.theme);
  const [noticeOpen, setNoticeOpen] = useState(false);
  const resolvedTheme = theme === "system" ? systemTheme() : theme;
  const nextTheme = resolvedTheme === "dark" ? "light" : "dark";
  const themeLabel =
    nextTheme === "light" ? UI_TEXT.switchToLight : UI_TEXT.switchToDark;

  return (
    <div
      className="orange-app"
      data-theme={theme}
      data-font-scale={preview.fontScale}
      data-motion={preview.motion}
    >
      <aside className="desktop-sidebar">
        <Brand />
        <Navigation />
        <div className="sidebar-footer">
          <span>{UI_TEXT.environment}</span>
          <strong>{UI_TEXT.serviceUnconfigured}</strong>
        </div>
      </aside>

      <div className="workspace">
        <header className="topbar">
          <div className="mobile-brand">
            <Brand compact />
          </div>
          <div className="page-heading">
            <span>{UI_TEXT.workspace}</span>
            <h1>{UI_TEXT.connection}</h1>
          </div>
          <div className="topbar-actions">
            <button
              type="button"
              className="theme-button"
              aria-label={themeLabel}
              title={themeLabel}
              onClick={() => setTheme(nextTheme)}
            >
              {resolvedTheme === "dark" ? (
                <Sun aria-hidden="true" />
              ) : (
                <Moon aria-hidden="true" />
              )}
              <span>
                {resolvedTheme === "dark"
                  ? UI_TEXT.darkTheme
                  : UI_TEXT.lightTheme}
              </span>
            </button>
            <button
              type="button"
              className="icon-button notification-button"
              aria-label={UI_TEXT.notification}
              title={UI_TEXT.notification}
              aria-expanded={noticeOpen}
              onClick={() => setNoticeOpen((open) => !open)}
            >
              <Bell aria-hidden="true" />
              <span className="notification-dot" aria-hidden="true" />
            </button>
            {noticeOpen && (
              <div className="notification-popover" role="status">
                {UI_TEXT.noNotifications}
              </div>
            )}
          </div>
        </header>

        <main className="dashboard">
          <section
            className="subscription-banner"
            aria-labelledby="banner-title"
          >
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

            <aside
              className="connection-details"
              aria-labelledby="details-title"
            >
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

        <Navigation mobile />
      </div>
    </div>
  );
}

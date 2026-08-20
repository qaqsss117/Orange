import {
  type ReactNode,
  useCallback,
  useEffect,
  useRef,
  useState,
} from "react";
import type { LucideIcon } from "lucide-react";
import {
  Bell,
  Home,
  Layers,
  LoaderCircle,
  MonitorSmartphone,
  Moon,
  RefreshCw,
  Server,
  Settings,
  ShieldCheck,
  Sun,
  User,
  Zap,
} from "lucide-react";
import {
  HashRouter,
  Navigate,
  NavLink,
  Route,
  Routes,
  useLocation,
} from "react-router-dom";
import orangeIcon from "../assets/product/brand/orange-development-mark.png";
import type {
  AuthSessionResponse,
  BusinessInitializationResponse,
  ConfigResponse,
  Notice,
  UserProfile,
} from "./businessApi";
import { ConnectionHome } from "./pages/ConnectionHome";
import { AccountPage } from "./pages/AccountPage";
import { AuthPage } from "./pages/AuthPage";
import { ForgotPasswordPage } from "./pages/ForgotPasswordPage";
import { HelpPage } from "./pages/HelpPage";
import { LegalPage } from "./pages/LegalPage";
import { SupportChatButton } from "./ui/SupportChatButton";
import { InvitationPage } from "./pages/InvitationPage";
import { NodesPage } from "./pages/NodesPage";
import { OrdersPage } from "./pages/OrdersPage";
import { OrderDetailPage } from "./pages/OrderDetailPage";
import { SettingsPage } from "./pages/SettingsPage";
import { SubscriptionPage } from "./pages/SubscriptionPage";
import { TicketDetailPage } from "./pages/TicketDetailPage";
import { TicketsPage } from "./pages/TicketsPage";
import { SHELL_TEXT } from "./shellContent";
import { startNodeDelayTest } from "./nodeDelayStore";
import { createSessionPageDataCache } from "./pageDataCache";
import { nativeShellServices, type ShellServices } from "./shellServices";
import { parseCommandError } from "./ipc";
import {
  SafeErrorBoundary,
  StatusScreen,
  ToastRegion,
  type ToastMessage,
} from "./ui/AsyncState";
import { UI_TEXT } from "./uiContent";
import {
  readThemePreference,
  storeThemePreference,
  systemTheme,
  type ThemePreference,
} from "./theme";

interface NavigationItem {
  label: string;
  path: string;
  icon: LucideIcon;
}

const NAVIGATION: readonly NavigationItem[] = [
  { label: SHELL_TEXT.connection, path: "/app", icon: Home },
  { label: SHELL_TEXT.subscription, path: "/subscription", icon: Layers },
  { label: SHELL_TEXT.nodes, path: "/nodes", icon: Server },
  { label: SHELL_TEXT.account, path: "/account", icon: User },
  { label: SHELL_TEXT.settings, path: "/settings", icon: Settings },
];

const PAGE_TITLES: Record<string, string> = {
  ...Object.fromEntries(NAVIGATION.map(({ path, label }) => [path, label])),
  "/orders": "我的订单",
  "/invitation": "我的邀请",
  "/tickets": "我的工单",
  "/help": "问题解答",
  "/legal": "法律与隐私",
};

type BootstrapState =
  | { status: "loading" }
  | { status: "error" }
  | { status: "ready"; value: BusinessInitializationResponse };

type NoticesState =
  | { status: "loading" }
  | { status: "error" }
  | { status: "ready"; notices: Notice[] };

const NOTICE_CONTENTION_RETRY_DELAYS_MS = [200, 400, 800] as const;

function isNoticeRequestContention(error: unknown): boolean {
  try {
    return parseCommandError(error).code === "cancelled";
  } catch {
    return false;
  }
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

function ThemeButton({
  resolvedTheme,
  onToggle,
}: {
  resolvedTheme: "light" | "dark";
  onToggle: () => void;
}) {
  const label =
    resolvedTheme === "dark"
      ? SHELL_TEXT.switchToLight
      : SHELL_TEXT.switchToDark;
  return (
    <button
      type="button"
      className="theme-button"
      aria-label={label}
      title={label}
      onClick={onToggle}
    >
      {resolvedTheme === "dark" ? (
        <Sun aria-hidden="true" />
      ) : (
        <Moon aria-hidden="true" />
      )}
      <span>
        {resolvedTheme === "dark"
          ? SHELL_TEXT.darkTheme
          : SHELL_TEXT.lightTheme}
      </span>
    </button>
  );
}

function PublicFrame({
  children,
  resolvedTheme,
  onToggleTheme,
}: {
  children: ReactNode;
  resolvedTheme: "light" | "dark";
  onToggleTheme: () => void;
}) {
  return (
    <div className="public-workspace">
      <aside className="public-brand-panel">
        <div className="brand-panel-blob brand-panel-blob-warm" />
        <div className="brand-panel-blob brand-panel-blob-deep" />
        <div className="brand-panel-ring" aria-hidden="true" />
        <div className="brand-panel-content">
          <img
            src={orangeIcon}
            alt=""
            aria-hidden="true"
            className="brand-panel-logo"
            draggable={false}
          />
          <h1>{SHELL_TEXT.brandTagline}</h1>
          <p>{SHELL_TEXT.brandTaglineDetail}</p>
          <ul>
            <li>
              <Zap aria-hidden="true" />
              {SHELL_TEXT.brandBulletNodes}
            </li>
            <li>
              <ShieldCheck aria-hidden="true" />
              {SHELL_TEXT.brandBulletSecure}
            </li>
            <li>
              <MonitorSmartphone aria-hidden="true" />
              {SHELL_TEXT.brandBulletDevices}
            </li>
          </ul>
        </div>
      </aside>
      <div className="public-main">
        <header className="public-topbar">
          <div className="public-topbar-brand">
            <Brand compact />
          </div>
          <ThemeButton resolvedTheme={resolvedTheme} onToggle={onToggleTheme} />
          <SupportChatButton />
        </header>
        {children}
      </div>
    </div>
  );
}

function Navigation({ mobile = false }: { mobile?: boolean }) {
  return (
    <nav
      className={mobile ? "mobile-navigation" : "sidebar-navigation"}
      aria-label={SHELL_TEXT.navigation}
    >
      <ul>
        {NAVIGATION.map(({ label, path, icon: Icon }) => (
          <li key={path}>
            <NavLink className="navigation-item" to={path}>
              <Icon aria-hidden="true" />
              <span>{label}</span>
            </NavLink>
          </li>
        ))}
      </ul>
    </nav>
  );
}

function AuthenticatedShell({
  config,
  user,
  services,
  theme,
  onThemeChange,
  resolvedTheme,
  onToggleTheme,
  onLoggedOut,
  onUserUpdated,
}: {
  config: ConfigResponse;
  user: UserProfile;
  services: ShellServices;
  theme: ThemePreference;
  onThemeChange: (theme: ThemePreference) => void;
  resolvedTheme: "light" | "dark";
  onToggleTheme: () => void;
  onLoggedOut: (session: AuthSessionResponse) => void;
  onUserUpdated: (user: UserProfile) => void;
}) {
  const location = useLocation();
  const [pageDataCache] = useState(() =>
    createSessionPageDataCache(user.userId),
  );
  const [noticeOpen, setNoticeOpen] = useState(false);
  const [noticesState, setNoticesState] = useState<NoticesState>({
    status: "loading",
  });
  const noticeRequestId = useRef(0);
  const serviceNotice = config.notice?.trim() || null;
  const notices = noticesState.status === "ready" ? noticesState.notices : [];
  const hasNotice = serviceNotice !== null || notices.length > 0;
  const loadNotices = useCallback(() => {
    const requestId = noticeRequestId.current + 1;
    noticeRequestId.current = requestId;
    setNoticesState({ status: "loading" });

    const load = async () => {
      for (let attempt = 0; ; attempt += 1) {
        try {
          const response = await services.fetchNotices();
          if (noticeRequestId.current === requestId) {
            setNoticesState({ status: "ready", notices: response.notices });
          }
          return;
        } catch (error) {
          if (noticeRequestId.current !== requestId) {
            return;
          }
          const retryDelay = NOTICE_CONTENTION_RETRY_DELAYS_MS[attempt];
          if (retryDelay === undefined || !isNoticeRequestContention(error)) {
            setNoticesState({ status: "error" });
            return;
          }
          await new Promise<void>((resolve) => {
            window.setTimeout(resolve, retryDelay);
          });
        }
        if (noticeRequestId.current !== requestId) {
          return;
        }
      }
    };

    void load();
  }, [services]);

  useEffect(() => {
    const task = window.setTimeout(loadNotices, 0);
    return () => {
      window.clearTimeout(task);
      noticeRequestId.current += 1;
    };
  }, [loadNotices]);
  const toggleNotices = () => {
    if (!noticeOpen && noticesState.status === "error") {
      loadNotices();
    }
    setNoticeOpen((open) => !open);
  };
  const pageTitle = location.pathname.startsWith("/orders/")
    ? "订单详情"
    : location.pathname.startsWith("/tickets/")
      ? "工单详情"
      : (PAGE_TITLES[location.pathname] ?? SHELL_TEXT.connection);

  return (
    <>
      <aside className="desktop-sidebar">
        <Brand />
        <Navigation />
        <div className="sidebar-footer">
          <span>{SHELL_TEXT.currentEnvironment}</span>
          <strong>{SHELL_TEXT.serviceReady}</strong>
        </div>
      </aside>

      <div className="workspace">
        <header className="topbar">
          <div className="mobile-brand">
            <Brand compact />
          </div>
          <div className="page-heading">
            <span>{SHELL_TEXT.workspace}</span>
            <h1>{pageTitle}</h1>
          </div>
          <div className="topbar-actions">
            <ThemeButton
              resolvedTheme={resolvedTheme}
              onToggle={onToggleTheme}
            />
            <button
              type="button"
              className="icon-button notification-button"
              aria-label={
                !hasNotice
                  ? SHELL_TEXT.notification
                  : SHELL_TEXT.notificationAvailable
              }
              title={SHELL_TEXT.notification}
              aria-expanded={noticeOpen}
              aria-controls="notification-popover"
              data-has-notice={hasNotice}
              onClick={toggleNotices}
            >
              <Bell aria-hidden="true" />
            </button>
            {noticeOpen && (
              <div
                id="notification-popover"
                className="notification-popover"
                role="dialog"
                aria-label={SHELL_TEXT.notification}
              >
                {serviceNotice === null &&
                notices.length === 0 &&
                noticesState.status === "ready" ? (
                  SHELL_TEXT.noNotifications
                ) : (
                  <ul className="notification-list">
                    {serviceNotice !== null && (
                      <li>
                        <strong>{SHELL_TEXT.serviceNotice}</strong>
                        <p>{serviceNotice}</p>
                      </li>
                    )}
                    {notices.map((notice, index) => (
                      <li key={`${notice.title}-${index}`}>
                        <strong>{notice.title}</strong>
                        <p>{notice.content}</p>
                      </li>
                    ))}
                  </ul>
                )}
                {noticesState.status === "loading" && (
                  <div className="notification-state" role="status">
                    <LoaderCircle className="spinning" aria-hidden="true" />
                    <span>{SHELL_TEXT.loadingNotifications}</span>
                  </div>
                )}
                {noticesState.status === "error" && (
                  <div className="notification-state" role="alert">
                    <span>{SHELL_TEXT.notificationsUnavailable}</span>
                    <button
                      type="button"
                      className="inline-action"
                      onClick={loadNotices}
                    >
                      <RefreshCw aria-hidden="true" />
                      {SHELL_TEXT.retry}
                    </button>
                  </div>
                )}
              </div>
            )}
            <SupportChatButton />
          </div>
        </header>

        <Routes>
          <Route path="/app" element={<ConnectionHome services={services} />} />
          <Route
            path="/subscription"
            element={
              <SubscriptionPage services={services} cache={pageDataCache} />
            }
          />
          <Route
            path="/nodes"
            element={<NodesPage services={services} cache={pageDataCache} />}
          />
          <Route path="/help" element={<HelpPage />} />
          <Route path="/legal" element={<LegalPage authenticated />} />
          <Route path="/orders" element={<OrdersPage services={services} />} />
          <Route
            path="/invitation"
            element={<InvitationPage services={services} />}
          />
          <Route
            path="/tickets"
            element={<TicketsPage services={services} />}
          />
          <Route
            path="/tickets/:ticketId"
            element={<TicketDetailPage services={services} />}
          />
          <Route
            path="/orders/:orderId"
            element={<OrderDetailPage services={services} />}
          />
          <Route
            path="/account"
            element={
              <AccountPage
                user={user}
                services={services}
                onUserUpdated={onUserUpdated}
                onLoggedOut={onLoggedOut}
              />
            }
          />
          <Route
            path="/settings"
            element={
              <SettingsPage
                services={services}
                theme={theme}
                onThemeChange={onThemeChange}
              />
            }
          />
          <Route path="*" element={<Navigate to="/app" replace />} />
        </Routes>
        <Navigation mobile />
      </div>
    </>
  );
}

function ReadyRouter({
  initialization,
  services,
  theme,
  onThemeChange,
  resolvedTheme,
  onToggleTheme,
  onRetryInitialization,
  onSessionChange,
  onToast,
}: {
  initialization: BusinessInitializationResponse;
  services: ShellServices;
  theme: ThemePreference;
  onThemeChange: (theme: ThemePreference) => void;
  resolvedTheme: "light" | "dark";
  onToggleTheme: () => void;
  onRetryInitialization: () => void;
  onSessionChange: (session: AuthSessionResponse) => void;
  onToast: (text: string, kind: ToastMessage["kind"]) => void;
}) {
  const { config, session } = initialization;
  const authenticatedUser =
    session.status === "authenticated" ? session.user : null;
  const authenticated = authenticatedUser !== null;

  // 登录态就绪后立即在后台异步测一次节点延迟，不阻塞界面。
  useEffect(() => {
    if (authenticated) {
      startNodeDelayTest(services);
    }
  }, [authenticated, services]);
  const publicAuthPage = (mode: "login" | "register") =>
    authenticated ? (
      <Navigate to="/app" replace />
    ) : (
      <PublicFrame resolvedTheme={resolvedTheme} onToggleTheme={onToggleTheme}>
        <AuthPage
          mode={mode}
          config={config}
          unverified={session.status === "unverified"}
          services={services}
          onRetryInitialization={onRetryInitialization}
          onAuthenticated={(user, message) => {
            onSessionChange({
              schemaVersion: 1,
              status: "authenticated",
              user,
            });
            onToast(message, "success");
          }}
        />
      </PublicFrame>
    );
  const publicForgotPasswordPage = authenticated ? (
    <Navigate to="/app" replace />
  ) : (
    <PublicFrame resolvedTheme={resolvedTheme} onToggleTheme={onToggleTheme}>
      <ForgotPasswordPage
        config={config}
        services={services}
        onCompleted={() => onToast(SHELL_TEXT.passwordResetSuccess, "success")}
      />
    </PublicFrame>
  );

  if (authenticatedUser !== null) {
    return (
      <Routes>
        <Route path="/login" element={<Navigate to="/app" replace />} />
        <Route path="/register" element={<Navigate to="/app" replace />} />
        <Route
          path="/forgot-password"
          element={<Navigate to="/app" replace />}
        />
        <Route
          path="*"
          element={
            <AuthenticatedShell
              config={config}
              user={authenticatedUser}
              services={services}
              theme={theme}
              onThemeChange={onThemeChange}
              resolvedTheme={resolvedTheme}
              onToggleTheme={onToggleTheme}
              onLoggedOut={(nextSession) => {
                onSessionChange(nextSession);
                onToast(SHELL_TEXT.logoutSuccess, "success");
              }}
              onUserUpdated={(user) => {
                onSessionChange({
                  schemaVersion: 1,
                  status: "authenticated",
                  user,
                });
              }}
            />
          }
        />
      </Routes>
    );
  }

  return (
    <Routes>
      <Route path="/login" element={publicAuthPage("login")} />
      <Route path="/register" element={publicAuthPage("register")} />
      <Route path="/forgot-password" element={publicForgotPasswordPage} />
      <Route path="/legal" element={<LegalPage authenticated={false} />} />
      <Route path="*" element={<Navigate to="/login" replace />} />
    </Routes>
  );
}

function Shell({
  services,
  theme,
  setTheme,
}: {
  services: ShellServices;
  theme: ThemePreference;
  setTheme: (theme: ThemePreference) => void;
}) {
  const location = useLocation();
  const [attempt, setAttempt] = useState(0);
  const [bootstrap, setBootstrap] = useState<BootstrapState>({
    status: "loading",
  });
  const [toast, setToast] = useState<ToastMessage | null>(null);
  const toastId = useRef(0);
  const [resolvedSystemTheme, setResolvedSystemTheme] = useState(systemTheme);
  const resolvedTheme = theme === "system" ? resolvedSystemTheme : theme;
  const nextTheme = resolvedTheme === "dark" ? "light" : "dark";
  const legalPageRequested = location.pathname === "/legal";
  const showToast = useCallback((text: string, kind: ToastMessage["kind"]) => {
    toastId.current += 1;
    setToast({ id: toastId.current, text, kind });
  }, []);

  useEffect(() => {
    const preference = window.matchMedia("(prefers-color-scheme: dark)");
    const updateResolvedTheme = (event: MediaQueryListEvent) => {
      setResolvedSystemTheme(event.matches ? "dark" : "light");
    };
    preference.addEventListener("change", updateResolvedTheme);
    return () => preference.removeEventListener("change", updateResolvedTheme);
  }, []);

  useEffect(() => {
    let active = true;
    void services.initializeBusiness().then(
      (value) => {
        if (active) {
          setBootstrap({ status: "ready", value });
        }
      },
      () => {
        if (active) {
          setBootstrap({ status: "error" });
        }
      },
    );
    return () => {
      active = false;
    };
  }, [attempt, services]);

  useEffect(() => {
    let active = true;
    let unlisten: (() => void) | undefined;
    void services
      .listenForTrayRuntimeError((kind) => {
        if (active) {
          showToast(
            kind === "exit"
              ? SHELL_TEXT.trayExitFailed
              : SHELL_TEXT.trayActionFailed,
            "error",
          );
        }
      })
      .then((dispose) => {
        if (active) {
          unlisten = dispose;
        } else {
          dispose();
        }
      })
      .catch(() => undefined);
    return () => {
      active = false;
      unlisten?.();
    };
  }, [services, showToast]);

  const retryInitialization = () => {
    setBootstrap({ status: "loading" });
    setAttempt((current) => current + 1);
  };
  const updateSession = (session: AuthSessionResponse) => {
    setBootstrap((current) =>
      current.status === "ready"
        ? { status: "ready", value: { ...current.value, session } }
        : current,
    );
  };

  return (
    <div
      className={`orange-app ${
        bootstrap.status === "ready" &&
        bootstrap.value.session.status === "authenticated"
          ? "app-authenticated"
          : "app-public"
      }`}
      data-theme={theme}
    >
      {legalPageRequested && bootstrap.status !== "ready" && (
        <LegalPage authenticated={false} />
      )}
      {!legalPageRequested && bootstrap.status === "loading" && (
        <PublicFrame
          resolvedTheme={resolvedTheme}
          onToggleTheme={() => setTheme(nextTheme)}
        >
          <StatusScreen
            kind="loading"
            title={SHELL_TEXT.startupTitle}
            detail={SHELL_TEXT.startupDetail}
          />
        </PublicFrame>
      )}
      {!legalPageRequested && bootstrap.status === "error" && (
        <PublicFrame
          resolvedTheme={resolvedTheme}
          onToggleTheme={() => setTheme(nextTheme)}
        >
          <StatusScreen
            kind="error"
            title={SHELL_TEXT.startupErrorTitle}
            detail={SHELL_TEXT.startupErrorDetail}
            actionLabel={SHELL_TEXT.retry}
            onAction={retryInitialization}
          />
        </PublicFrame>
      )}
      {bootstrap.status === "ready" && (
        <ReadyRouter
          initialization={bootstrap.value}
          services={services}
          theme={theme}
          onThemeChange={setTheme}
          resolvedTheme={resolvedTheme}
          onToggleTheme={() => setTheme(nextTheme)}
          onRetryInitialization={retryInitialization}
          onSessionChange={updateSession}
          onToast={showToast}
        />
      )}
      <ToastRegion message={toast} onDismiss={() => setToast(null)} />
    </div>
  );
}

export default function App() {
  const [theme, setThemeState] = useState<ThemePreference>(readThemePreference);
  const setTheme = useCallback((nextTheme: ThemePreference) => {
    storeThemePreference(nextTheme);
    setThemeState(nextTheme);
  }, []);

  return (
    <SafeErrorBoundary>
      <HashRouter>
        <Shell
          services={nativeShellServices}
          theme={theme}
          setTheme={setTheme}
        />
      </HashRouter>
    </SafeErrorBoundary>
  );
}

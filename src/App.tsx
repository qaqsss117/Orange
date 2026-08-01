import {
  type ReactNode,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import type { LucideIcon } from "lucide-react";
import {
  Bell,
  Home,
  Layers,
  Moon,
  Server,
  Settings,
  Sun,
  User,
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
  UserProfile,
} from "./businessApi";
import { ConnectionHome } from "./pages/ConnectionHome";
import { AccountPage } from "./pages/AccountPage";
import { AuthPage } from "./pages/AuthPage";
import { InvitationPage } from "./pages/InvitationPage";
import { NodesPage } from "./pages/NodesPage";
import { OrdersPage } from "./pages/OrdersPage";
import { OrderDetailPage } from "./pages/OrderDetailPage";
import { SettingsPage } from "./pages/SettingsPage";
import { SubscriptionPage } from "./pages/SubscriptionPage";
import { TicketsPage } from "./pages/TicketsPage";
import { SHELL_TEXT } from "./shellContent";
import {
  createPreviewShellServices,
  nativeShellServices,
  readShellPreview,
  type ShellServices,
} from "./shellServices";
import {
  SafeErrorBoundary,
  StatusScreen,
  ToastRegion,
  type ToastMessage,
} from "./ui/AsyncState";
import { UI_TEXT } from "./uiContent";
import { readUiPreview, systemTheme, type PreviewTheme } from "./uiPreview";

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
};

type BootstrapState =
  | { status: "loading" }
  | { status: "error" }
  | { status: "ready"; value: BusinessInitializationResponse };

export interface AppProps {
  services?: ShellServices;
  developmentEnabled?: boolean;
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
      <header className="public-topbar">
        <Brand compact />
        <ThemeButton resolvedTheme={resolvedTheme} onToggle={onToggleTheme} />
      </header>
      {children}
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
  user,
  services,
  resolvedTheme,
  onToggleTheme,
  onLoggedOut,
  onUserUpdated,
}: {
  user: UserProfile;
  services: ShellServices;
  resolvedTheme: "light" | "dark";
  onToggleTheme: () => void;
  onLoggedOut: (session: AuthSessionResponse) => void;
  onUserUpdated: (user: UserProfile) => void;
}) {
  const location = useLocation();
  const [noticeOpen, setNoticeOpen] = useState(false);
  const pageTitle = location.pathname.startsWith("/orders/")
    ? "订单详情"
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
              aria-label={SHELL_TEXT.notification}
              title={SHELL_TEXT.notification}
              aria-expanded={noticeOpen}
              onClick={() => setNoticeOpen((open) => !open)}
            >
              <Bell aria-hidden="true" />
            </button>
            {noticeOpen && (
              <div className="notification-popover" role="status">
                {SHELL_TEXT.noNotifications}
              </div>
            )}
          </div>
        </header>

        <Routes>
          <Route path="/app" element={<ConnectionHome services={services} />} />
          <Route
            path="/subscription"
            element={<SubscriptionPage services={services} />}
          />
          <Route path="/nodes" element={<NodesPage services={services} />} />
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
            element={<SettingsPage services={services} />}
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
  resolvedTheme,
  onToggleTheme,
  onRetryInitialization,
  onSessionChange,
  onToast,
}: {
  initialization: BusinessInitializationResponse;
  services: ShellServices;
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

  if (authenticatedUser !== null) {
    return (
      <Routes>
        <Route path="/login" element={<Navigate to="/app" replace />} />
        <Route path="/register" element={<Navigate to="/app" replace />} />
        <Route
          path="*"
          element={
            <AuthenticatedShell
              user={authenticatedUser}
              services={services}
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
      <Route path="*" element={<Navigate to="/login" replace />} />
    </Routes>
  );
}

function Shell({
  services,
  theme,
  setTheme,
  preview,
}: {
  services: ShellServices;
  theme: PreviewTheme;
  setTheme: (theme: PreviewTheme) => void;
  preview: ReturnType<typeof readUiPreview>;
}) {
  const [attempt, setAttempt] = useState(0);
  const [bootstrap, setBootstrap] = useState<BootstrapState>({
    status: "loading",
  });
  const [toast, setToast] = useState<ToastMessage | null>(null);
  const toastId = useRef(0);
  const resolvedTheme = theme === "system" ? systemTheme() : theme;
  const nextTheme = resolvedTheme === "dark" ? "light" : "dark";
  const showToast = useCallback((text: string, kind: ToastMessage["kind"]) => {
    toastId.current += 1;
    setToast({ id: toastId.current, text, kind });
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
      data-font-scale={preview.fontScale}
      data-motion={preview.motion}
    >
      {bootstrap.status === "loading" && (
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
      {bootstrap.status === "error" && (
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

export default function App({
  services,
  developmentEnabled = import.meta.env.DEV,
}: AppProps) {
  const uiPreview = readUiPreview(window.location.search);
  const [theme, setTheme] = useState<PreviewTheme>(uiPreview.theme);
  const shellPreview = readShellPreview(
    window.location.search,
    developmentEnabled,
  );
  const resolvedServices = useMemo(
    () =>
      services ??
      (shellPreview === null
        ? nativeShellServices
        : createPreviewShellServices(shellPreview)),
    [services, shellPreview],
  );

  return (
    <SafeErrorBoundary>
      <HashRouter>
        <Shell
          services={resolvedServices}
          theme={theme}
          setTheme={setTheme}
          preview={uiPreview}
        />
      </HashRouter>
    </SafeErrorBoundary>
  );
}

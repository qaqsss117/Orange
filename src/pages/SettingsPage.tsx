import {
  AlertCircle,
  ExternalLink,
  Globe2,
  Info,
  Monitor,
  Moon,
  Network,
  Power,
  RefreshCw,
  Route,
  ShieldCheck,
  Sun,
} from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import type {
  ConnectionMode,
  LegalDocument,
  NetworkTool,
  RoutingMode,
  RuntimeInfoResponse,
} from "../ipc";
import { toPublicUiError, type ShellServices } from "../shellServices";
import type { ThemePreference } from "../theme";

const MODES: ReadonlyArray<{
  id: ConnectionMode;
  label: string;
  detail: string;
  icon: typeof Monitor;
}> = [
  {
    id: "system_proxy",
    label: "系统代理",
    detail: "仅代理遵循当前用户 Windows 代理设置的应用。",
    icon: Monitor,
  },
  {
    id: "tun",
    label: "TUN",
    detail: "通过虚拟网卡接管更完整的系统网络流量。",
    icon: Network,
  },
];

const ROUTING_MODES: ReadonlyArray<{
  id: RoutingMode;
  label: string;
  detail: string;
  icon: typeof Monitor;
}> = [
  {
    id: "smart",
    label: "智能路由",
    detail: "国内资源直连，其他流量通过当前节点。",
    icon: Route,
  },
  {
    id: "global",
    label: "全局代理",
    detail: "所有流量都通过当前节点。",
    icon: Globe2,
  },
  {
    id: "direct",
    label: "全部直连",
    detail: "所有流量绕过代理。",
    icon: Network,
  },
];

const THEMES: ReadonlyArray<{
  id: ThemePreference;
  label: string;
  detail: string;
  icon: typeof Monitor;
}> = [
  {
    id: "system",
    label: "跟随系统",
    detail: "使用 Windows 外观设置",
    icon: Monitor,
  },
  {
    id: "light",
    label: "浅色",
    detail: "始终使用浅色外观",
    icon: Sun,
  },
  {
    id: "dark",
    label: "深色",
    detail: "始终使用深色外观",
    icon: Moon,
  },
];

const NETWORK_TOOL_OPTIONS: ReadonlyArray<{
  id: NetworkTool;
  label: string;
  detail: string;
}> = [
  {
    id: "ip_lookup",
    label: "IP 查询",
    detail: "查看当前公网 IP 和网络位置",
  },
  {
    id: "speed_test",
    label: "网速测试",
    detail: "测量当前网络下载速度",
  },
];

const LEGAL_DOCUMENT_OPTIONS: ReadonlyArray<{
  id: LegalDocument;
  label: string;
}> = [
  { id: "terms_of_service", label: "用户协议" },
  { id: "privacy_policy", label: "隐私政策" },
];

export function SettingsPage({
  services,
  theme,
  onThemeChange,
}: {
  services: ShellServices;
  theme: ThemePreference;
  onThemeChange: (theme: ThemePreference) => void;
}) {
  const [mode, setMode] = useState<ConnectionMode | null>(null);
  const [pending, setPending] = useState<ConnectionMode | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [routingMode, setRoutingMode] = useState<RoutingMode | null>(null);
  const [routingPending, setRoutingPending] = useState<RoutingMode | null>(
    null,
  );
  const [routingError, setRoutingError] = useState<string | null>(null);
  const [launchOnStartup, setLaunchOnStartup] = useState<boolean | null>(null);
  const [launchPending, setLaunchPending] = useState(false);
  const [launchError, setLaunchError] = useState<string | null>(null);
  const [runtimeInfo, setRuntimeInfo] = useState<RuntimeInfoResponse | null>(
    null,
  );
  const [runtimeError, setRuntimeError] = useState<string | null>(null);
  const [servicePortalPending, setServicePortalPending] = useState(false);
  const [servicePortalError, setServicePortalError] = useState<string | null>(
    null,
  );
  const [networkToolPending, setNetworkToolPending] =
    useState<NetworkTool | null>(null);
  const [networkToolError, setNetworkToolError] = useState<{
    tool: NetworkTool;
    message: string;
  } | null>(null);
  const [legalDocumentPending, setLegalDocumentPending] =
    useState<LegalDocument | null>(null);
  const [legalDocumentError, setLegalDocumentError] = useState<string | null>(
    null,
  );

  const load = useCallback(async () => {
    setError(null);
    try {
      setMode((await services.getConnectionMode()).mode);
    } catch (reason) {
      setError(toPublicUiError(reason).message);
    }
  }, [services]);

  const loadRuntimeInfo = useCallback(async () => {
    setRuntimeError(null);
    try {
      setRuntimeInfo(await services.getRuntimeInfo());
    } catch (reason) {
      setRuntimeError(toPublicUiError(reason).message);
    }
  }, [services]);

  const loadRoutingMode = useCallback(async () => {
    setRoutingError(null);
    try {
      setRoutingMode((await services.getRoutingMode()).mode);
    } catch (reason) {
      setRoutingError(toPublicUiError(reason).message);
    }
  }, [services]);

  const loadLaunchOnStartup = useCallback(async () => {
    setLaunchError(null);
    try {
      setLaunchOnStartup((await services.getLaunchOnStartup()).enabled);
    } catch (reason) {
      setLaunchError(toPublicUiError(reason).message);
    }
  }, [services]);

  useEffect(() => {
    let active = true;
    void services.getConnectionMode().then(
      (value) => {
        if (active) setMode(value.mode);
      },
      (reason) => {
        if (active) setError(toPublicUiError(reason).message);
      },
    );
    void services.getRuntimeInfo().then(
      (value) => {
        if (active) setRuntimeInfo(value);
      },
      (reason) => {
        if (active) setRuntimeError(toPublicUiError(reason).message);
      },
    );
    void services.getRoutingMode().then(
      (value) => {
        if (active) setRoutingMode(value.mode);
      },
      (reason) => {
        if (active) setRoutingError(toPublicUiError(reason).message);
      },
    );
    void services.getLaunchOnStartup().then(
      (value) => {
        if (active) setLaunchOnStartup(value.enabled);
      },
      (reason) => {
        if (active) setLaunchError(toPublicUiError(reason).message);
      },
    );
    return () => {
      active = false;
    };
  }, [services]);

  const selectMode = async (target: ConnectionMode) => {
    if (pending !== null || mode === target) return;
    setPending(target);
    setError(null);
    try {
      setMode((await services.setConnectionMode(target)).mode);
    } catch (reason) {
      setError(toPublicUiError(reason).message);
    } finally {
      setPending(null);
    }
  };

  const toggleLaunchOnStartup = async () => {
    if (launchOnStartup === null || launchPending) return;
    const target = !launchOnStartup;
    setLaunchPending(true);
    setLaunchError(null);
    try {
      setLaunchOnStartup((await services.setLaunchOnStartup(target)).enabled);
    } catch (reason) {
      setLaunchError(toPublicUiError(reason).message);
    } finally {
      setLaunchPending(false);
    }
  };

  const selectRoutingMode = async (target: RoutingMode) => {
    if (routingPending !== null || routingMode === target) return;
    setRoutingPending(target);
    setRoutingError(null);
    try {
      setRoutingMode((await services.setRoutingMode(target)).mode);
    } catch (reason) {
      setRoutingError(toPublicUiError(reason).message);
    } finally {
      setRoutingPending(null);
    }
  };

  const openServicePortal = async () => {
    if (servicePortalPending) return;
    setServicePortalPending(true);
    setServicePortalError(null);
    try {
      await services.openServicePortal();
    } catch (reason) {
      setServicePortalError(toPublicUiError(reason).message);
    } finally {
      setServicePortalPending(false);
    }
  };

  const openSupportChat = async () => {
    if (servicePortalPending) return;
    setServicePortalPending(true);
    setServicePortalError(null);
    try {
      await services.openSupportChat();
    } catch (reason) {
      setServicePortalError(toPublicUiError(reason).message);
    } finally {
      setServicePortalPending(false);
    }
  };

  const openTelegramBot = async () => {
    if (servicePortalPending) return;
    setServicePortalPending(true);
    setServicePortalError(null);
    try {
      await services.openTelegramBot();
    } catch (reason) {
      setServicePortalError(toPublicUiError(reason).message);
    } finally {
      setServicePortalPending(false);
    }
  };

  const openNetworkTool = async (tool: NetworkTool) => {
    if (networkToolPending !== null) return;
    setNetworkToolPending(tool);
    setNetworkToolError(null);
    try {
      await services.openNetworkTool(tool);
    } catch (reason) {
      setNetworkToolError({
        tool,
        message: toPublicUiError(reason).message,
      });
    } finally {
      setNetworkToolPending(null);
    }
  };

  const openLegalDocument = async (document: LegalDocument) => {
    if (legalDocumentPending !== null) return;
    setLegalDocumentPending(document);
    setLegalDocumentError(null);
    try {
      await services.openLegalDocument(document);
    } catch (reason) {
      setLegalDocumentError(toPublicUiError(reason).message);
    } finally {
      setLegalDocumentPending(null);
    }
  };

  return (
    <main className="management-page settings-page">
      <div className="management-heading">
        <div>
          <span>本机配置</span>
          <h2>设置</h2>
          <p>选择 Orange 在 Windows 上接管流量的方式。</p>
        </div>
      </div>

      <section className="settings-section" aria-labelledby="theme-title">
        <div className="section-heading">
          <Sun aria-hidden="true" />
          <div>
            <h3 id="theme-title">外观</h3>
          </div>
        </div>

        <div
          className="mode-segment theme-mode-segment"
          role="radiogroup"
          aria-label="外观模式"
        >
          {THEMES.map((option) => {
            const Icon = option.icon;
            const selected = option.id === theme;
            return (
              <button
                type="button"
                role="radio"
                aria-checked={selected}
                className="mode-option"
                data-selected={selected}
                key={option.id}
                onClick={() => onThemeChange(option.id)}
              >
                <Icon aria-hidden="true" />
                <span>
                  <strong>{option.label}</strong>
                  <small>{option.detail}</small>
                </span>
              </button>
            );
          })}
        </div>
      </section>

      <section className="settings-section" aria-labelledby="startup-title">
        <div className="section-heading">
          <Power aria-hidden="true" />
          <div>
            <h3 id="startup-title">启动</h3>
          </div>
        </div>

        <div className="settings-toggle-row">
          <div>
            <strong>开机启动</strong>
            <small>
              {launchOnStartup === null
                ? "正在读取"
                : launchOnStartup
                  ? "已开启"
                  : "已关闭"}
            </small>
          </div>
          <button
            type="button"
            role="switch"
            aria-label="开机启动"
            aria-checked={launchOnStartup ?? false}
            aria-busy={launchPending}
            className="setting-switch"
            data-enabled={launchOnStartup === true}
            disabled={launchOnStartup === null || launchPending}
            onClick={() => void toggleLaunchOnStartup()}
          >
            <span aria-hidden="true" />
          </button>
        </div>

        {launchError !== null && (
          <div className="inline-notice inline-notice-error" role="alert">
            <AlertCircle aria-hidden="true" />
            <span>{launchError}</span>
            {launchOnStartup === null && (
              <button
                type="button"
                className="inline-action"
                onClick={() => void loadLaunchOnStartup()}
              >
                重试
              </button>
            )}
          </div>
        )}
      </section>

      <section
        className="settings-section"
        aria-labelledby="connection-mode-title"
      >
        <div className="section-heading">
          <ShieldCheck aria-hidden="true" />
          <div>
            <h3 id="connection-mode-title">连接模式</h3>
            <p>在线切换会先安全断开，再按新模式重新连接。</p>
          </div>
        </div>

        {mode === null && error === null ? (
          <div className="page-state compact" role="status">
            <RefreshCw className="spinning" aria-hidden="true" />
            <span>正在读取设置</span>
          </div>
        ) : (
          <div className="mode-segment" role="radiogroup" aria-label="连接模式">
            {MODES.map((option) => {
              const Icon = option.icon;
              const selected = option.id === mode;
              return (
                <button
                  type="button"
                  role="radio"
                  aria-checked={selected}
                  className="mode-option"
                  data-selected={selected}
                  disabled={pending !== null}
                  key={option.id}
                  onClick={() => void selectMode(option.id)}
                >
                  <Icon aria-hidden="true" />
                  <span>
                    <strong>{option.label}</strong>
                    <small>
                      {pending === option.id ? "正在切换" : option.detail}
                    </small>
                  </span>
                </button>
              );
            })}
          </div>
        )}

        {error !== null && (
          <div className="inline-notice inline-notice-error" role="alert">
            <AlertCircle aria-hidden="true" />
            <span>{error}</span>
            {mode === null && (
              <button
                type="button"
                className="inline-action"
                onClick={() => void load()}
              >
                重试
              </button>
            )}
          </div>
        )}
      </section>

      <section
        className="settings-section"
        aria-labelledby="routing-mode-title"
      >
        <div className="section-heading">
          <Route aria-hidden="true" />
          <div>
            <h3 id="routing-mode-title">路由模式</h3>
            <p>切换后立即按新策略重新建立当前连接。</p>
          </div>
        </div>

        {routingMode === null && routingError === null ? (
          <div className="page-state compact" role="status">
            <RefreshCw className="spinning" aria-hidden="true" />
            <span>正在读取路由模式</span>
          </div>
        ) : (
          <div
            className="mode-segment routing-mode-segment"
            role="radiogroup"
            aria-label="路由模式"
          >
            {ROUTING_MODES.map((option) => {
              const Icon = option.icon;
              const selected = option.id === routingMode;
              return (
                <button
                  type="button"
                  role="radio"
                  aria-checked={selected}
                  className="mode-option"
                  data-selected={selected}
                  disabled={routingPending !== null}
                  key={option.id}
                  onClick={() => void selectRoutingMode(option.id)}
                >
                  <Icon aria-hidden="true" />
                  <span>
                    <strong>{option.label}</strong>
                    <small>
                      {routingPending === option.id
                        ? "正在切换"
                        : option.detail}
                    </small>
                  </span>
                </button>
              );
            })}
          </div>
        )}

        {routingError !== null && (
          <div className="inline-notice inline-notice-error" role="alert">
            <AlertCircle aria-hidden="true" />
            <span>{routingError}</span>
            {routingMode === null && (
              <button
                type="button"
                className="inline-action"
                onClick={() => void loadRoutingMode()}
              >
                重试
              </button>
            )}
          </div>
        )}
      </section>

      <section
        className="settings-section"
        aria-labelledby="network-tools-title"
      >
        <div className="section-heading">
          <Network aria-hidden="true" />
          <div>
            <h3 id="network-tools-title">网络工具</h3>
          </div>
        </div>

        <div className="settings-action-list">
          {NETWORK_TOOL_OPTIONS.map((option) => (
            <div className="settings-action-row" key={option.id}>
              <div>
                <strong>{option.label}</strong>
                <small>{option.detail}</small>
              </div>
              <button
                type="button"
                className="secondary-action"
                disabled={networkToolPending !== null}
                onClick={() => void openNetworkTool(option.id)}
              >
                <ExternalLink aria-hidden="true" />
                {networkToolPending === option.id ? "正在打开" : "打开"}
              </button>
            </div>
          ))}
        </div>

        {networkToolError !== null && (
          <div className="inline-notice inline-notice-error" role="alert">
            <AlertCircle aria-hidden="true" />
            <span>{networkToolError.message}</span>
            <button
              type="button"
              className="inline-action"
              disabled={networkToolPending !== null}
              onClick={() => void openNetworkTool(networkToolError.tool)}
            >
              重试
            </button>
          </div>
        )}
      </section>

      <section className="settings-section" aria-labelledby="support-title">
        <div className="section-heading">
          <ExternalLink aria-hidden="true" />
          <div>
            <h3 id="support-title">支持</h3>
          </div>
        </div>

        <div className="settings-action-row">
          <div>
            <strong>在线客服</strong>
            <small>应用内打开客服聊天窗口</small>
          </div>
          <button
            type="button"
            className="secondary-action"
            disabled={servicePortalPending}
            onClick={() => void openSupportChat()}
          >
            <ExternalLink aria-hidden="true" />
            {servicePortalPending ? "正在打开" : "打开"}
          </button>
        </div>

        <div className="settings-action-row">
          <div>
            <strong>服务中心</strong>
            <small>在系统浏览器中打开</small>
          </div>
          <button
            type="button"
            className="secondary-action"
            disabled={servicePortalPending}
            onClick={() => void openServicePortal()}
          >
            <ExternalLink aria-hidden="true" />
            {servicePortalPending ? "正在打开" : "打开"}
          </button>
        </div>

        <div className="settings-action-row">
          <div>
            <strong>Telegram 机器人</strong>
            <small>绑定账户并接收通知</small>
          </div>
          <button
            type="button"
            className="secondary-action"
            disabled={servicePortalPending}
            onClick={() => void openTelegramBot()}
          >
            <ExternalLink aria-hidden="true" />
            {servicePortalPending ? "正在打开" : "打开"}
          </button>
        </div>

        {servicePortalError !== null && (
          <div className="inline-notice inline-notice-error" role="alert">
            <AlertCircle aria-hidden="true" />
            <span>{servicePortalError}</span>
            <button
              type="button"
              className="inline-action"
              disabled={servicePortalPending}
              onClick={() => void openServicePortal()}
            >
              重试
            </button>
          </div>
        )}
      </section>

      <section className="settings-section" aria-labelledby="about-title">
        <div className="section-heading">
          <Info aria-hidden="true" />
          <div>
            <h3 id="about-title">关于 Orange</h3>
          </div>
        </div>

        {runtimeInfo === null && runtimeError === null && (
          <div className="page-state compact" role="status">
            <RefreshCw className="spinning" aria-hidden="true" />
            <span>正在读取版本</span>
          </div>
        )}

        {runtimeInfo !== null && (
          <dl className="settings-info-list">
            <div>
              <dt>产品</dt>
              <dd>{runtimeInfo.productName}</dd>
            </div>
            <div>
              <dt>当前版本</dt>
              <dd>{runtimeInfo.productVersion}</dd>
            </div>
          </dl>
        )}

        <div className="settings-action-list">
          <div className="settings-action-row">
            <div>
              <strong>检查更新</strong>
              <small>打开官网下载页获取最新版本</small>
            </div>
            <button
              type="button"
              className="secondary-action"
              disabled={servicePortalPending}
              onClick={() => void openServicePortal()}
            >
              <ExternalLink aria-hidden="true" />
              {servicePortalPending ? "正在打开" : "打开"}
            </button>
          </div>
          {LEGAL_DOCUMENT_OPTIONS.map((option) => (
            <div className="settings-action-row" key={option.id}>
              <div>
                <strong>{option.label}</strong>
                <small>在系统浏览器中查看</small>
              </div>
              <button
                type="button"
                className="secondary-action"
                disabled={legalDocumentPending !== null}
                onClick={() => void openLegalDocument(option.id)}
              >
                <ExternalLink aria-hidden="true" />
                {legalDocumentPending === option.id ? "正在打开" : "打开"}
              </button>
            </div>
          ))}
        </div>

        {legalDocumentError !== null && (
          <div className="inline-notice inline-notice-error" role="alert">
            <AlertCircle aria-hidden="true" />
            <span>{legalDocumentError}</span>
          </div>
        )}

        {runtimeError !== null && (
          <div className="inline-notice inline-notice-error" role="alert">
            <AlertCircle aria-hidden="true" />
            <span>{runtimeError}</span>
            <button
              type="button"
              className="inline-action"
              onClick={() => void loadRuntimeInfo()}
            >
              重试
            </button>
          </div>
        )}
      </section>
    </main>
  );
}

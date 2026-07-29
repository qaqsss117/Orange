import {
  AlertCircle,
  Monitor,
  Network,
  RefreshCw,
  ShieldCheck,
} from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import type { ConnectionMode } from "../ipc";
import { toPublicUiError, type ShellServices } from "../shellServices";

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

export function SettingsPage({ services }: { services: ShellServices }) {
  const [mode, setMode] = useState<ConnectionMode | null>(null);
  const [pending, setPending] = useState<ConnectionMode | null>(null);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    setError(null);
    try {
      setMode((await services.getConnectionMode()).mode);
    } catch (reason) {
      setError(toPublicUiError(reason).message);
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

  return (
    <main className="management-page settings-page">
      <div className="management-heading">
        <div>
          <span>本机配置</span>
          <h2>设置</h2>
          <p>选择 Orange 在 Windows 上接管流量的方式。</p>
        </div>
      </div>

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
    </main>
  );
}

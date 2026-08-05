import { AlertCircle, Check, Gauge, RefreshCw, Server } from "lucide-react";
import { useCallback, useEffect, useMemo, useState } from "react";
import type { NodeCatalogResponse, PublicNodeDelay } from "../ipc";
import { toPublicUiError, type ShellServices } from "../shellServices";

const PROTOCOL_LABELS = {
  shadowsocks: "Shadowsocks",
  trojan: "Trojan",
  hysteria2: "Hysteria 2",
  vless: "VLESS",
} as const;

function delayLabel(result: PublicNodeDelay | undefined): string {
  if (result === undefined) return "未测试";
  if (result.status === "available") return `${result.delayMs} ms`;
  return result.status === "timed_out" ? "超时" : "不可用";
}

export function NodesPage({ services }: { services: ShellServices }) {
  const [catalog, setCatalog] = useState<NodeCatalogResponse | null>(null);
  const [loading, setLoading] = useState(true);
  const [testing, setTesting] = useState(false);
  const [selecting, setSelecting] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [delays, setDelays] = useState<Record<string, PublicNodeDelay>>({});

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      setCatalog(await services.getNodeCatalog());
    } catch (reason) {
      setError(toPublicUiError(reason).message);
    } finally {
      setLoading(false);
    }
  }, [services]);

  useEffect(() => {
    let active = true;
    void services.getNodeCatalog().then(
      (value) => {
        if (active) {
          setCatalog(value);
          setLoading(false);
        }
      },
      (reason) => {
        if (active) {
          setError(toPublicUiError(reason).message);
          setLoading(false);
        }
      },
    );
    return () => {
      active = false;
    };
  }, [services]);

  const nodeCount = useMemo(
    () =>
      catalog?.groups.reduce((count, group) => count + group.nodes.length, 0) ??
      0,
    [catalog],
  );

  const testDelays = async () => {
    if (testing || nodeCount === 0) return;
    setTesting(true);
    setError(null);
    try {
      const response = await services.testNodeDelays();
      setDelays(
        Object.fromEntries(
          response.results.map((result) => [
            `${result.selectorId}:${result.nodeId}`,
            result.result,
          ]),
        ),
      );
    } catch (reason) {
      setError(toPublicUiError(reason).message);
    } finally {
      setTesting(false);
    }
  };

  const select = async (selectorId: string, nodeId: string) => {
    const key = `${selectorId}:${nodeId}`;
    if (selecting !== null) return;
    setSelecting(key);
    setError(null);
    setNotice(null);
    try {
      const response = await services.selectNode(selectorId, nodeId);
      if (response.pending) {
        setNotice("已保存节点选择,将在连接后生效。");
      }
      setCatalog((current) =>
        current === null
          ? current
          : {
              ...current,
              groups: current.groups.map((group) =>
                group.id === selectorId
                  ? { ...group, selectedNodeId: nodeId }
                  : group,
              ),
            },
      );
    } catch (reason) {
      setError(toPublicUiError(reason).message);
    } finally {
      setSelecting(null);
    }
  };

  return (
    <main className="management-page nodes-page">
      <div className="management-heading">
        <div>
          <span>线路目录</span>
          <h2>节点</h2>
          <p>
            {nodeCount === 0
              ? "选择订阅中的可用线路。"
              : `共 ${nodeCount} 个可用节点。`}
          </p>
        </div>
        <button
          type="button"
          className="secondary-action"
          disabled={testing || nodeCount === 0}
          onClick={() => void testDelays()}
        >
          <Gauge className={testing ? "spinning" : ""} aria-hidden="true" />
          {testing ? "正在测试" : "测试延迟"}
        </button>
      </div>

      {error !== null && (
        <div className="inline-notice inline-notice-error" role="alert">
          <AlertCircle aria-hidden="true" />
          <span>{error}</span>
        </div>
      )}

      {notice !== null && (
        <div className="inline-notice" role="status">
          <Check aria-hidden="true" />
          <span>{notice}</span>
        </div>
      )}

      {loading ? (
        <div className="page-state" role="status">
          <RefreshCw className="spinning" aria-hidden="true" />
          <span>正在读取节点目录</span>
        </div>
      ) : catalog === null || catalog.groups.length === 0 ? (
        <div className="page-state" role="status">
          <Server aria-hidden="true" />
          <strong>暂无节点</strong>
          <span>请先刷新有效订阅。</span>
          <button
            type="button"
            className="inline-action"
            onClick={() => void load()}
          >
            重试
          </button>
        </div>
      ) : (
        <div className="node-groups">
          {catalog.groups.map((group) => (
            <section
              className="node-group"
              key={group.id}
              aria-labelledby={`group-${group.id}`}
            >
              <div className="node-group-heading">
                <div>
                  <span>策略组</span>
                  <h3 id={`group-${group.id}`}>{group.id}</h3>
                </div>
                <span>{group.nodes.length} 个节点</span>
              </div>
              <div className="node-list">
                {group.nodes.map((node) => {
                  const key = `${group.id}:${node.id}`;
                  const selected = group.selectedNodeId === node.id;
                  const delay = delays[key];
                  return (
                    <button
                      type="button"
                      className="node-row"
                      data-selected={selected}
                      aria-pressed={selected}
                      disabled={selecting !== null}
                      key={node.id}
                      onClick={() => void select(group.id, node.id)}
                    >
                      <span className="node-selection-mark">
                        {selected && <Check aria-hidden="true" />}
                      </span>
                      <span className="node-copy">
                        <strong>{node.name}</strong>
                        <span>{PROTOCOL_LABELS[node.protocol]}</span>
                      </span>
                      <span
                        className={`node-delay delay-${delay?.status ?? "untested"}`}
                      >
                        {selecting === key ? "正在选择" : delayLabel(delay)}
                      </span>
                    </button>
                  );
                })}
              </div>
            </section>
          ))}
        </div>
      )}
    </main>
  );
}

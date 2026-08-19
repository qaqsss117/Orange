import {
  AlertCircle,
  Check,
  Gauge,
  MousePointer2,
  RefreshCw,
  Server,
  Sparkles,
} from "lucide-react";
import {
  useCallback,
  useEffect,
  useMemo,
  useState,
  useSyncExternalStore,
} from "react";
import type {
  NodeCatalogResponse,
  NodeLoadState,
  NodeSelectionMode,
  PublicNodeDelay,
} from "../ipc";
import {
  getNodeDelayState,
  startNodeDelayTest,
  subscribeNodeDelays,
} from "../nodeDelayStore";
import {
  loadCachedPageResource,
  setCachedPageResource,
  type SessionPageDataCache,
} from "../pageDataCache";
import { toPublicUiError, type ShellServices } from "../shellServices";
import { parseNodeName } from "../ui/nodeRegion";
import { NodeRegionIcon } from "../ui/NodeRegionIcon";

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

function loadStateLabel(state: NodeLoadState): string {
  switch (state) {
    case "idle":
      return "空闲";
    case "normal":
      return "正常";
    case "busy":
    case "overloaded":
      return "繁忙";
    case "unknown":
      return "未知";
  }
}

export function NodesPage({
  services,
  cache,
}: {
  services: ShellServices;
  cache: SessionPageDataCache;
}) {
  const [catalog, setCatalog] = useState<NodeCatalogResponse | null>(
    cache.nodeCatalog.value,
  );
  const [loading, setLoading] = useState(cache.nodeCatalog.value === null);
  const [refreshing, setRefreshing] = useState(false);
  const [selecting, setSelecting] = useState<string | null>(null);
  const [modePending, setModePending] = useState<NodeSelectionMode | null>(
    null,
  );
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const delayState = useSyncExternalStore(
    subscribeNodeDelays,
    getNodeDelayState,
  );
  const { delays, testing, error: delayError } = delayState;

  // 拉取最新订阅后强制重读节点目录，让新增/下线的线路立即出现在列表里。
  // 走缓存层并带 force，避免刷新后其他页面继续拿到旧的节点快照。
  const refresh = useCallback(async () => {
    if (refreshing) return;
    setRefreshing(true);
    setError(null);
    setNotice(null);
    try {
      await services.refreshSubscription();
      setCatalog(
        await loadCachedPageResource(
          cache.nodeCatalog,
          () => services.getNodeCatalog(),
          { force: true },
        ),
      );
      setNotice("订阅已刷新。");
    } catch (reason) {
      setError(toPublicUiError(reason).message);
    } finally {
      setRefreshing(false);
    }
  }, [cache, refreshing, services]);

  useEffect(() => {
    let active = true;
    const refreshCachedValue = cache.nodeCatalog.value !== null;
    void loadCachedPageResource(
      cache.nodeCatalog,
      () => services.getNodeCatalog(),
      { force: refreshCachedValue },
    ).then(
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
  }, [cache, services]);

  const nodeCount = useMemo(
    () =>
      catalog?.groups.reduce((count, group) => count + group.nodes.length, 0) ??
      0,
    [catalog],
  );

  // 进入节点页时若从未测试过，后台异步补一次，不阻塞页面。
  useEffect(() => {
    if (nodeCount > 0) {
      startNodeDelayTest(services);
    }
  }, [services, nodeCount]);

  const testDelays = () => {
    if (nodeCount === 0) return;
    setError(null);
    startNodeDelayTest(services, { force: true });
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
      setCatalog((current) => {
        const next: NodeCatalogResponse | null =
          current === null
            ? current
            : {
                ...current,
                selectionMode: "manual",
                groups: current.groups.map((group) =>
                  group.id === selectorId
                    ? { ...group, selectedNodeId: nodeId }
                    : group,
                ),
              };
        if (next !== null) setCachedPageResource(cache.nodeCatalog, next);
        return next;
      });
    } catch (reason) {
      setError(toPublicUiError(reason).message);
    } finally {
      setSelecting(null);
    }
  };

  const selectMode = async (mode: NodeSelectionMode) => {
    if (
      catalog === null ||
      modePending !== null ||
      catalog.selectionMode === mode
    ) {
      return;
    }
    setModePending(mode);
    setError(null);
    setNotice(null);
    try {
      const response = await services.setNodeSelectionMode(mode);
      setCatalog((current) => {
        const next: NodeCatalogResponse | null =
          current === null
            ? current
            : { ...current, selectionMode: response.mode };
        if (next !== null) setCachedPageResource(cache.nodeCatalog, next);
        return next;
      });
    } catch (reason) {
      setError(toPublicUiError(reason).message);
    } finally {
      setModePending(null);
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
        <div className="nodes-heading-actions">
          <div
            className="node-mode-segment"
            role="group"
            aria-label="节点选择模式"
          >
            <button
              type="button"
              className="node-mode-option"
              data-selected={catalog?.selectionMode === "auto"}
              aria-pressed={catalog?.selectionMode === "auto"}
              disabled={catalog === null || modePending !== null}
              onClick={() => void selectMode("auto")}
            >
              <Sparkles aria-hidden="true" />
              <span>{modePending === "auto" ? "切换中" : "自动"}</span>
            </button>
            <button
              type="button"
              className="node-mode-option"
              data-selected={catalog?.selectionMode === "manual"}
              aria-pressed={catalog?.selectionMode === "manual"}
              disabled={catalog === null || modePending !== null}
              onClick={() => void selectMode("manual")}
            >
              <MousePointer2 aria-hidden="true" />
              <span>{modePending === "manual" ? "切换中" : "手动"}</span>
            </button>
          </div>
          <button
            type="button"
            className="secondary-action"
            disabled={refreshing}
            onClick={() => void refresh()}
          >
            <RefreshCw
              className={refreshing ? "spinning" : ""}
              aria-hidden="true"
            />
            {refreshing ? "正在刷新" : "刷新订阅"}
          </button>
          <button
            type="button"
            className="secondary-action"
            disabled={testing || nodeCount === 0}
            onClick={() => testDelays()}
          >
            <Gauge className={testing ? "spinning" : ""} aria-hidden="true" />
            {testing ? "正在测试" : "测试延迟"}
          </button>
        </div>
      </div>

      {(error ?? delayError) !== null && (
        <div className="inline-notice inline-notice-error" role="alert">
          <AlertCircle aria-hidden="true" />
          <span>{error ?? delayError}</span>
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
            disabled={refreshing}
            onClick={() => void refresh()}
          >
            {refreshing ? "正在刷新" : "刷新订阅"}
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
                  const { tag, displayName } = parseNodeName(node.name);
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
                        <strong>
                          <NodeRegionIcon tag={tag} />
                          {displayName}
                        </strong>
                        <span>
                          {PROTOCOL_LABELS[node.protocol]} ·{" "}
                          {loadStateLabel(node.loadState)}
                        </span>
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

import type { PublicNodeDelay } from "./ipc";
import { toPublicUiError, type ShellServices } from "./shellServices";

/**
 * 全局节点延迟测试结果缓存。
 *
 * 结果保存在模块级状态中，与页面组件生命周期解耦：
 * 切换路由后结果仍然保留，后台测试在页面卸载后继续运行，
 * 完成时通过订阅通知所有正在监听的页面。
 */
export interface NodeDelayState {
  /** key 为 `${selectorId}:${nodeId}` */
  delays: Record<string, PublicNodeDelay>;
  testing: boolean;
  /** 最近一次测试完成时间（epoch ms），null 表示从未测试 */
  testedAt: number | null;
  error: string | null;
}

const initialState: NodeDelayState = {
  delays: {},
  testing: false,
  testedAt: null,
  error: null,
};

let state: NodeDelayState = initialState;
let inFlight: Promise<void> | null = null;
const listeners = new Set<() => void>();

function setState(patch: Partial<NodeDelayState>): void {
  state = { ...state, ...patch };
  for (const listener of listeners) {
    listener();
  }
}

export function getNodeDelayState(): NodeDelayState {
  return state;
}

export function subscribeNodeDelays(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

/**
 * 异步启动延迟测试，立即返回，不阻塞调用方。
 *
 * - 已有一次测试在跑时直接复用，不重复发起；
 * - 默认情况下已有结果则跳过（force 用于手动重测）；
 * - 完成后无论页面是否还挂着，结果都会写入缓存。
 */
export function startNodeDelayTest(
  services: ShellServices,
  options?: { force?: boolean },
): void {
  if (inFlight !== null) return;
  if (options?.force !== true && state.testedAt !== null) return;

  setState({ testing: true, error: null });
  inFlight = services
    .testNodeDelays()
    .then((response) => {
      const delays = Object.fromEntries(
        response.results.map((result) => [
          `${result.selectorId}:${result.nodeId}`,
          result.result,
        ]),
      );
      setState({ delays, testedAt: Date.now(), testing: false, error: null });
    })
    .catch((reason: unknown) => {
      setState({ testing: false, error: toPublicUiError(reason).message });
    })
    .finally(() => {
      inFlight = null;
    });
}

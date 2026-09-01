import { useSyncExternalStore } from 'react';
import NetInfo from '@react-native-community/netinfo';

type NetworkHintPhase = 'hidden' | 'switching' | 'offline_confirmed';

// WiFi ↔ 4G 切换时的保护窗口，窗口内不触发离线判断
const NETWORK_SWITCH_GUARD_MS = 3000;

export interface NetworkStatusSnapshot {
  isOffline: boolean;
  isResolved: boolean;
  isConnected: boolean | null;
  networkType: string;
  hintPhase: NetworkHintPhase;
  showOfflineHint: boolean;
  shouldBlockSend: boolean;
}

type RawNetworkSnapshot = {
  isConnected: boolean | null;
  networkType: string;
};

const networkStatusStore = {
  snapshot: {
    isOffline: false,
    isResolved: false,
    isConnected: null,
    networkType: 'unknown',
    hintPhase: 'hidden' as NetworkHintPhase,
    showOfflineHint: false,
    shouldBlockSend: false,
  } as NetworkStatusSnapshot,
  rawSnapshot: {
    isConnected: null,
    networkType: 'unknown',
  } as RawNetworkSnapshot,
  previousConnectedType: null as string | null,
  switchGuardUntil: 0,
  timer: null as ReturnType<typeof setTimeout> | null,
  unsubscribe: null as (() => void) | null,
  listeners: new Set<() => void>(),
};

function isConnectedNetworkType(type: string) {
  return type === 'wifi' || type === 'cellular';
}

/**
 * 把 NetInfo 原始返回收敛成模块内部统一使用的原始快照。
 */
function toRawNetworkSnapshot(input: {
  isConnected: boolean | null;
  type?: string;
}): RawNetworkSnapshot {
  return {
    isConnected: input.isConnected,
    networkType: input.type ?? 'unknown',
  };
}

/**
 * 通知所有订阅网络状态的组件重新读取最新快照。
 */
function emitNetworkStatusChange() {
  networkStatusStore.listeners.forEach((listener) => listener());
}

/**
 * 清理上一次等待中的状态机定时器，避免旧定时器继续推进过期状态。
 */
function clearNetworkStatusTimer() {
  if (!networkStatusStore.timer) {
    return;
  }
  clearTimeout(networkStatusStore.timer);
  networkStatusStore.timer = null;
}

/**
 * 把新的业务快照写回模块状态，只有真的变化时才触发订阅者更新。
 */
function commitNetworkStatusSnapshot(nextSnapshot: NetworkStatusSnapshot) {
  const cur = networkStatusStore.snapshot;
  if (
    cur.isOffline === nextSnapshot.isOffline &&
    cur.isResolved === nextSnapshot.isResolved &&
    cur.isConnected === nextSnapshot.isConnected &&
    cur.networkType === nextSnapshot.networkType &&
    cur.hintPhase === nextSnapshot.hintPhase &&
    cur.showOfflineHint === nextSnapshot.showOfflineHint &&
    cur.shouldBlockSend === nextSnapshot.shouldBlockSend
  ) {
    return;
  }
  networkStatusStore.snapshot = nextSnapshot;
  emitNetworkStatusChange();
}

/**
 * 断网判断只依赖 isConnected === false。
 * isInternetReachable 在 iOS 真机调试时不可靠（USB/Metro 会导致假阳性），不作为判断依据。
 */
function recomputeNetworkStatus(now = Date.now()) {
  clearNetworkStatusTimer();

  const { isConnected, networkType } = networkStatusStore.rawSnapshot;
  const isResolved = typeof isConnected === 'boolean';

  // 数据尚未就绪，保持初始态
  if (!isResolved) {
    return;
  }

  const isConnectedNow = isConnected === true;
  const isConnectedType = isConnectedNetworkType(networkType);
  const previousConnectedType = networkStatusStore.previousConnectedType;
  const isSwitchingBetweenConnectedNetworks =
    isConnectedNow &&
    isConnectedType &&
    previousConnectedType != null &&
    previousConnectedType !== networkType &&
    isConnectedNetworkType(previousConnectedType);

  if (isConnectedNow && isConnectedType) {
    networkStatusStore.previousConnectedType = networkType;
  }

  // 网络类型切换（WiFi ↔ 4G），保护窗口内不判断离线
  if (isSwitchingBetweenConnectedNetworks) {
    networkStatusStore.switchGuardUntil = now + NETWORK_SWITCH_GUARD_MS;
    networkStatusStore.timer = setTimeout(
      () => recomputeNetworkStatus(),
      NETWORK_SWITCH_GUARD_MS,
    );
    commitNetworkStatusSnapshot({
      isOffline: false,
      isResolved,
      isConnected,
      networkType,
      hintPhase: 'switching',
      showOfflineHint: false,
      shouldBlockSend: false,
    });
    return;
  }

  // 仍在切换保护窗口内
  if (now < networkStatusStore.switchGuardUntil) {
    networkStatusStore.timer = setTimeout(
      () => recomputeNetworkStatus(),
      networkStatusStore.switchGuardUntil - now,
    );
    commitNetworkStatusSnapshot({
      isOffline: false,
      isResolved,
      isConnected,
      networkType,
      hintPhase: 'switching',
      showOfflineHint: false,
      shouldBlockSend: false,
    });
    return;
  }

  // isConnected === false 是唯一可靠的断网信号
  if (isConnected === false) {
    commitNetworkStatusSnapshot({
      isOffline: true,
      isResolved,
      isConnected,
      networkType,
      hintPhase: 'offline_confirmed',
      showOfflineHint: true,
      shouldBlockSend: true,
    });
    return;
  }

  commitNetworkStatusSnapshot({
    isOffline: false,
    isResolved,
    isConnected,
    networkType,
    hintPhase: 'hidden',
    showOfflineHint: false,
    shouldBlockSend: false,
  });
}

/**
 * 把最新原始网络事件写入模块，再统一交给状态机判断。
 */
function applyRawNetworkSnapshot(rawSnapshot: RawNetworkSnapshot) {
  networkStatusStore.rawSnapshot = rawSnapshot;
  recomputeNetworkStatus();
}

/**
 * 确保整个应用只创建一次 NetInfo 订阅，让 Header 和发送前校验共用同一份业务态。
 */
function ensureNetworkStatusSubscription() {
  if (networkStatusStore.unsubscribe) {
    return;
  }

  networkStatusStore.unsubscribe = NetInfo.addEventListener((state) => {
    applyRawNetworkSnapshot(toRawNetworkSnapshot(state));
  });

  void NetInfo.fetch()
    .then((state) => {
      applyRawNetworkSnapshot(toRawNetworkSnapshot(state));
    })
    .catch(() => {
      // 初始化失败时静默处理，状态机保持默认值（不阻断发送）
    });
}

/**
 * 给 React 组件提供标准订阅入口。
 */
function subscribeNetworkStatus(onStoreChange: () => void) {
  ensureNetworkStatusSubscription();
  networkStatusStore.listeners.add(onStoreChange);
  return () => {
    networkStatusStore.listeners.delete(onStoreChange);
  };
}

/**
 * 读取当前已经稳定化后的业务网络状态，同步无副作用。
 */
export function getNetworkStatusSnapshot(): NetworkStatusSnapshot {
  ensureNetworkStatusSubscription();
  return networkStatusStore.snapshot;
}

/**
 * 给 UI 层提供统一网络状态，顶部提示直接读取稳定业务态，不再自己再造状态机。
 */
export function useNetworkStatus(): NetworkStatusSnapshot {
  return useSyncExternalStore(
    subscribeNetworkStatus,
    getNetworkStatusSnapshot,
    getNetworkStatusSnapshot,
  );
}

/**
 * 发送前主动强刷一次底层网络状态，再返回同一套稳定业务态。
 * 注意：iOS 上 NetInfo.refresh() 是 no-op，不会真正重新探测，直接读快照即可。
 */
export function getNetworkStatusForSend(): NetworkStatusSnapshot {
  ensureNetworkStatusSubscription();
  return networkStatusStore.snapshot;
}

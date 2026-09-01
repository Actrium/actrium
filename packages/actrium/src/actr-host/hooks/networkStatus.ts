import { useSyncExternalStore } from 'react';
import NetInfo from '@react-native-community/netinfo';

type NetworkHintPhase = 'hidden' | 'switching' | 'offline_confirmed';

// WiFi ↔ 4G Toggle protection window, not trigger offline judgement inside the window
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
 * - Put it on. NetInfo Returns the original snapshot that is used in a single form within the harvest-concentrating module.
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
 * Notify all components that subscribe to network status to read the latest snapshot.
 */
function emitNetworkStatusChange() {
  networkStatusStore.listeners.forEach((listener) => listener());
}

/**
 * Clears the last waiting status timer to avoid the old timer continuing to advance into an expired state.
 */
function clearNetworkStatusTimer() {
  if (!networkStatusStore.timer) {
    return;
  }
  clearTimeout(networkStatusStore.timer);
  networkStatusStore.timer = null;
}

/**
 * Back to the module status with a new business snapshot, only when a real change occurs will trigger the subscriber ' s update.
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
 * The offline judgment depends on it. isConnected === false.
 * isInternetReachable Yes. iOS Unreliability when debugging a real machine (in the case of a real machine)USB/Metro It can lead to false positiveness) and does not provide a basis for judgement.
 */
function recomputeNetworkStatus(now = Date.now()) {
  clearNetworkStatusTimer();

  const { isConnected, networkType } = networkStatusStore.rawSnapshot;
  const isResolved = typeof isConnected === 'boolean';

  // Data not ready to start.
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

  // Network type switching (WiFi ↔ 4G) protect windows from offline
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

  // Still in the Switch Save Window
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

  // isConnected === false The only reliable offline signal.
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
 * Write the latest original network events into the module and then hand them over to the status machine for judgement.
 */
function applyRawNetworkSnapshot(rawSnapshot: RawNetworkSnapshot) {
  networkStatusStore.rawSnapshot = rawSnapshot;
  recomputeNetworkStatus();
}

/**
 * Ensure that the entire application is created only once NetInfo Subscriptions, let Header The same business as the pre-transmitrator check.
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
      // Quiet processing when initialization fails, status machine maintains default value (no interruption of sending)
    });
}

/**
 * Here. React Component provides a standard subscription portal.
 */
function subscribeNetworkStatus(onStoreChange: () => void) {
  ensureNetworkStatusSubscription();
  networkStatusStore.listeners.add(onStoreChange);
  return () => {
    networkStatusStore.listeners.delete(onStoreChange);
  };
}

/**
 * Read the current stable business network state, and no side effects.
 */
export function getNetworkStatusSnapshot(): NetworkStatusSnapshot {
  ensureNetworkStatusSubscription();
  return networkStatusStore.snapshot;
}

/**
 * Here. UI The layer provides a unified network state, with a top tip to read the stable business and no longer build the self-reform machine.
 */
export function useNetworkStatus(): NetworkStatusSnapshot {
  return useSyncExternalStore(
    subscribeNetworkStatus,
    getNetworkStatusSnapshot,
    getNetworkStatusSnapshot,
  );
}

/**
 * Actively brush the bottom network state before sending and return to the same stable business.
 * Note:iOS Go, go, go! NetInfo.refresh() Yes. no-opIt's not really redetectable. Just read the snapshot.
 */
export function getNetworkStatusForSend(): NetworkStatusSnapshot {
  ensureNetworkStatusSubscription();
  return networkStatusStore.snapshot;
}

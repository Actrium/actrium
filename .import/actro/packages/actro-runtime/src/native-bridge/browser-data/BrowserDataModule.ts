import { NativeModules } from 'react-native';
import type { BookmarkData, HistoryItemData } from '../../actr-host/types';

type BrowserDataNativeModule = {
  getRecentHistory?: (limit: number) => Promise<HistoryItemData[]>;
  deleteHistoryItem?: (address: string) => Promise<boolean>;
  clearHistory?: () => Promise<boolean>;
  getBookmarks?: (limit: number) => Promise<BookmarkData[]>;
  deleteBookmark?: (address: string) => Promise<boolean>;
  clearBookmarks?: () => Promise<boolean>;
  saveSyncSession?: (
    environment: string,
    token: string,
    baseUrl: string,
  ) => Promise<boolean>;
  clearSyncSession?: (environment: string) => Promise<boolean>;
  syncNow?: () => Promise<string>;
  getSyncStatus?: () => Promise<string>;
};

const BrowserDataModule: BrowserDataNativeModule | undefined = (
  NativeModules as { BrowserDataModule?: BrowserDataNativeModule }
).BrowserDataModule;

export type BrowserSyncStatusName =
  | 'idle'
  | 'pending'
  | 'retryScheduled'
  | 'waitingForCredentials'
  | 'requiresUserAction';

export type BrowserSyncStatus = {
  version: number;
  storageSchemaVersion: number;
  status: BrowserSyncStatusName | string;
  schedulerAction: string;
  retryAttempt: number;
  nextRunDelaySeconds?: number | null;
  nextRunAtUnixSeconds?: number | null;
  waitingForCredentials: boolean;
  requiresUserAction: boolean;
  pendingRunScheduled: boolean;
  lastClassification?: string | null;
  lastResult?: unknown | null;
};

export const hasBrowserDataModule = (): boolean => Boolean(BrowserDataModule);

export const hasBrowserSyncModule = (): boolean =>
  Boolean(BrowserDataModule?.getSyncStatus)
  && Boolean(BrowserDataModule?.syncNow);

export const loadRecentBrowserHistory = async (
  limit = 500,
): Promise<HistoryItemData[]> => {
  try {
    return (await BrowserDataModule?.getRecentHistory?.(limit)) ?? [];
  } catch (error) {
    console.error('[BrowserDataModule] Failed to load browser history', error);
    return [];
  }
};

export const deleteBrowserHistoryItem = async (
  address?: string,
): Promise<boolean> => {
  if (!address) return false;
  try {
    return (await BrowserDataModule?.deleteHistoryItem?.(address)) ?? false;
  } catch (error) {
    console.error('[BrowserDataModule] Failed to delete browser history item', error);
    return false;
  }
};

export const clearBrowserHistory = async (): Promise<boolean> => {
  try {
    return (await BrowserDataModule?.clearHistory?.()) ?? false;
  } catch (error) {
    console.error('[BrowserDataModule] Failed to clear browser history', error);
    return false;
  }
};

export const loadBrowserBookmarks = async (
  limit = 500,
): Promise<BookmarkData[]> => {
  try {
    return (await BrowserDataModule?.getBookmarks?.(limit)) ?? [];
  } catch (error) {
    console.error('[BrowserDataModule] Failed to load browser bookmarks', error);
    return [];
  }
};

export const deleteBrowserBookmark = async (
  address?: string,
): Promise<boolean> => {
  if (!address) return false;
  try {
    return (await BrowserDataModule?.deleteBookmark?.(address)) ?? false;
  } catch (error) {
    console.error('[BrowserDataModule] Failed to delete browser bookmark', error);
    return false;
  }
};

export const clearBrowserBookmarks = async (): Promise<boolean> => {
  try {
    return (await BrowserDataModule?.clearBookmarks?.()) ?? false;
  } catch (error) {
    console.error('[BrowserDataModule] Failed to clear browser bookmarks', error);
    return false;
  }
};

export const saveBrowserSyncSession = async (
  environment: string,
  token: string,
  baseUrl: string,
): Promise<boolean> => {
  try {
    return (await BrowserDataModule?.saveSyncSession?.(environment, token, baseUrl)) ?? false;
  } catch (error) {
    console.error('[BrowserDataModule] Failed to save browser sync session', error);
    return false;
  }
};

export const clearBrowserSyncSession = async (
  environment: string,
): Promise<boolean> => {
  try {
    return (await BrowserDataModule?.clearSyncSession?.(environment)) ?? false;
  } catch (error) {
    console.error('[BrowserDataModule] Failed to clear browser sync session', error);
    return false;
  }
};

export const syncBrowserDataNow = async (): Promise<string | null> => {
  try {
    return (await BrowserDataModule?.syncNow?.()) ?? null;
  } catch (error) {
    console.error('[BrowserDataModule] Failed to run browser sync', error);
    return null;
  }
};

export const getBrowserSyncStatus = async (): Promise<BrowserSyncStatus | null> => {
  try {
    const rawStatus = await BrowserDataModule?.getSyncStatus?.();
    return parseBrowserSyncStatus(rawStatus);
  } catch (error) {
    console.error('[BrowserDataModule] Failed to get browser sync status', error);
    return null;
  }
};

const parseBrowserSyncStatus = (
  rawStatus?: string | null,
): BrowserSyncStatus | null => {
  if (!rawStatus || rawStatus === 'null') return null;

  try {
    const parsed = JSON.parse(rawStatus) as Partial<BrowserSyncStatus> | null;
    if (!parsed || typeof parsed !== 'object') return null;
    if (typeof parsed.status !== 'string') return null;
    if (typeof parsed.schedulerAction !== 'string') return null;
    if (typeof parsed.retryAttempt !== 'number') return null;

    return {
      version: typeof parsed.version === 'number' ? parsed.version : 1,
      storageSchemaVersion:
        typeof parsed.storageSchemaVersion === 'number'
          ? parsed.storageSchemaVersion
          : 0,
      status: parsed.status,
      schedulerAction: parsed.schedulerAction,
      retryAttempt: parsed.retryAttempt,
      nextRunDelaySeconds: parsed.nextRunDelaySeconds ?? null,
      nextRunAtUnixSeconds: parsed.nextRunAtUnixSeconds ?? null,
      waitingForCredentials: Boolean(parsed.waitingForCredentials),
      requiresUserAction: Boolean(parsed.requiresUserAction),
      pendingRunScheduled: Boolean(parsed.pendingRunScheduled),
      lastClassification: parsed.lastClassification ?? null,
      lastResult: parsed.lastResult ?? null,
    };
  } catch (error) {
    console.error('[BrowserDataModule] Failed to parse browser sync status', error);
    return null;
  }
};

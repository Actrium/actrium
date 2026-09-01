// System app types (built-in apps like chrome:// pages)
import type { SystemAppType as ActroCoreSystemAppType } from './coreContract';

export type SystemAppType = ActroCoreSystemAppType;

// History item passed from native
export type HistoryItemData = {
  id: string;
  title: string;
  appURL?: string;
  visitTime: number; // timestamp
};

// History group (closed tab group)
export type HistoryGroupData = {
  id: string;
  name: string;
  color: string;
  tabs: { title: string; appURL?: string }[];
  closedAt: number; // timestamp
};

// Bookmark item passed from native
export type BookmarkData = {
  id: string;
  title: string;
  appURL?: string;
  createdAt: number;
  lastUsed?: number;
};

export type TabConfig = {
  id: string;
  title: string;
  url?: string;
  htmlContent?: string;
  reloadKey?: number;
  contract?: {
    name: string;
    version: number;
  };
  permissions?: string[];
  // Legacy: 双 Engine 架构
  guestScript?: string;
  leftPanelScript?: string;
  rightPanelScript?: string;
  // v2.0: 单 Engine 架构
  unifiedScript?: string;
  backgroundColor: string;
  rightPanelDefaultVisible?: boolean;
  // System app type (built-in apps)
  systemAppType?: SystemAppType;
  // System app data
  historyItems?: HistoryItemData[];
  historyGroups?: HistoryGroupData[];
  bookmarks?: BookmarkData[];
  // 预设 App URL，有值时 LoadApp 自动加载，无需用户手动输入
  appUrl?: string;
  appId?: string;
};

export type ActrProps = {
  tabs: TabConfig[];
  currentTabId?: string;
  isLeftPanelVisible: boolean;
  isRightPanelVisible?: boolean;
  // Native 传递的容器尺寸（确保窗口缩小时 RN 能感知）
  containerWidth?: number;
  containerHeight?: number;
};

// TabRouter receives all tabs (both actr and system apps)
export type TabRouterProps = ActrProps & {
  // Single tab mode - each RootView renders one tab
  singleTabMode?: boolean;
  tab?: TabConfig; // Single tab when singleTabMode is true
  // Native 传递的容器尺寸（确保窗口缩小时 RN 能感知）
  containerWidth?: number;
  containerHeight?: number;

  // FIXME 这些应该放到 tab 里面？
  // 预设 App URL，有值时 LoadApp 自动加载，无需用户手动输入
  appUrl?: string;
  appId?: string;
  // Native-transparent payload for system app deep-links/actions
  systemAppPayload?: Record<string, unknown>;
};

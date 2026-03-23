import { create } from 'zustand';

interface AuthTask {
  clientId: string;
  appId: string;
  sourceTabId?: string;
}

export type AppAuthorizationStatus = 'unknown' | 'authorized' | 'not_authorized';

interface AuthTaskState {
  // Key 为 tabId
  tasks: Record<string, AuthTask>;
  // 以 appId 为 key，缓存授权状态；缺省视为 unknown
  alreadyAuthorizedAppIds: Record<string, AppAuthorizationStatus>;
  addAuthorizedAppId: (appId: string) => void;
  addUnauthorizedAppId: (appId: string) => void;
  getAppAuthorizationStatus: (appId: string) => AppAuthorizationStatus;
  clearAuthorizationCache: () => void;
  regTask: (
    tabId: string,
    clientId: string,
    appId: string,
    sourceTabId?: string,
  ) => void;
  updateTask: (tabId: string, data: Partial<AuthTask>) => void;
  getTask: (tabId: string) => AuthTask | undefined;
  clearTask: (tabId: string) => void;
}

/**
 * 用于管理授权任务的 Store
 * 当从某个插件发起登录/授权请求时，将上下文信息（clientId）存入此 Store
 * LoginApp 启动后根据当前的 tabId 提取对应的 clientId
 */
export const useAuthTaskStore = create<AuthTaskState>((set, get) => ({
  tasks: {},
  alreadyAuthorizedAppIds: {},
  addAuthorizedAppId: (appId) =>
    set((state) => {
      return {
        alreadyAuthorizedAppIds: {
          ...state.alreadyAuthorizedAppIds,
          [appId]: 'authorized',
        },
      };
    }),
  addUnauthorizedAppId: (appId) =>
    set((state) => {
      // 记录“已确认未授权”，避免每次发送都重复请求授权状态接口
      return {
        alreadyAuthorizedAppIds: {
          ...state.alreadyAuthorizedAppIds,
          [appId]: 'not_authorized',
        },
      };
    }),
  getAppAuthorizationStatus: (appId) =>
    get().alreadyAuthorizedAppIds[appId] || 'unknown',
  clearAuthorizationCache: () =>
    set(() => ({
      alreadyAuthorizedAppIds: {},
    })),
  regTask: (tabId, clientId, appId, sourceTabId) =>
    set((state) => ({
      tasks: { ...state.tasks, [tabId]: { clientId, appId, sourceTabId } },
    })),
  updateTask: (tabId, data) =>
    set((state) => ({
      tasks: {
        ...state.tasks,
        [tabId]: { ...state.tasks[tabId], ...data },
      },
    })),
  getTask: (tabId) => get().tasks[tabId],
  clearTask: (tabId) =>
    set((state) => {
      const newTasks = { ...state.tasks };
      delete newTasks[tabId];
      return { tasks: newTasks };
    }),
}));

import { create } from 'zustand';

interface AuthTask {
  clientId: string;
  appId: string;
}

interface AuthTaskState {
  // Key 为 tabId
  tasks: Record<string, AuthTask>;
  alreadyAuthorizedAppIds: Set<string>;
  addAuthorizedAppId: (appId: string) => void;
  regTask: (tabId: string, clientId: string, appId: string) => void;
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
  alreadyAuthorizedAppIds: new Set(),
  addAuthorizedAppId: (appId) =>
    set((state) => {
      const next = new Set(state.alreadyAuthorizedAppIds);
      next.add(appId);
      return { alreadyAuthorizedAppIds: next };
    }),
  regTask: (tabId, clientId, appId) =>
    set((state) => ({
      tasks: { ...state.tasks, [tabId]: { clientId, appId } },
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

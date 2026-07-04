import { Platform, Linking } from 'react-native';
import { create } from 'zustand';
import {
  checkNotifications,
  requestNotifications,
  openSettings,
  RESULTS,
} from 'react-native-permissions';

export interface NotificationSubSettings {
  task: boolean;
  accountSecurity: boolean;
  systemUpdate: boolean;
  operation: boolean;
}

export type NotificationSubKey = keyof NotificationSubSettings;

interface NotificationSettings {
  enabled: boolean;
  sub: NotificationSubSettings;
}

interface AppSettings {
  notification: NotificationSettings;
}

const DEFAULT_SUB_SETTINGS: NotificationSubSettings = {
  task: true,
  accountSecurity: true,
  systemUpdate: true,
  operation: true,
};

const DEFAULT_SETTINGS: AppSettings = {
  notification: {
    enabled: true,
    sub: DEFAULT_SUB_SETTINGS,
  },
};

interface AppSettingsState {
  settings: AppSettings;
  updateNotificationSubSetting: (key: NotificationSubKey, value: boolean) => void;
  setNotificationEnabled: (enabled: boolean) => void;
  /** 应用启动时检查权限，未请求过则主动弹窗申请 */
  syncNotificationPermission: () => Promise<void>;
  /** 只检查权限状态并同步开关，不弹权限申请弹窗 */
  checkNotificationPermission: () => Promise<void>;
  /** 尝试开启通知：有权限则开开关，无权限则跳转系统设置 */
  requestNotificationEnable: () => Promise<void>;
}

export const useSettingsStore = create<AppSettingsState>(
  (set, get) => ({
    settings: DEFAULT_SETTINGS,

    updateNotificationSubSetting: (key: NotificationSubKey, value: boolean) => {
      set((state) => ({
        settings: {
          ...state.settings,
          notification: {
            ...state.settings.notification,
            sub: {
              ...state.settings.notification.sub,
              [key]: value,
            },
          },
        },
      }));
    },

    setNotificationEnabled: (enabled: boolean) => {
      set((state) => ({
        settings: {
          ...state.settings,
          notification: {
            ...state.settings.notification,
            enabled,
          },
        },
      }));
    },

    syncNotificationPermission: async () => {
      try {
        const { status } = await checkNotifications();
        const hasPermission = status === RESULTS.GRANTED || status === RESULTS.LIMITED;

        if (hasPermission) {
          return;
        }

        // denied = 从未请求过权限，主动弹系统权限申请
        if (status === RESULTS.DENIED) {
          const result = await requestNotifications(['alert', 'badge', 'sound']);
          const granted = result.status === RESULTS.GRANTED || result.status === RESULTS.LIMITED;
          if (!granted) {
            get().setNotificationEnabled(false);
          }
          return;
        }

        // blocked = 用户曾拒绝过，不再弹窗，只同步状态
        get().setNotificationEnabled(false);
      } catch (error) {
        console.error('[useSettingsStore] Failed to sync notification permission:', error);
      }
    },

    checkNotificationPermission: async () => {
      try {
        const { status } = await checkNotifications();
        const hasPermission = status === RESULTS.GRANTED || status === RESULTS.LIMITED;
        if (!hasPermission) {
          get().setNotificationEnabled(false);
        }
      } catch (error) {
        console.error('[useSettingsStore] Failed to check notification permission:', error);
      }
    },

    requestNotificationEnable: async () => {
      try {
        const { status } = await checkNotifications();
        const hasPermission = status === RESULTS.GRANTED || status === RESULTS.LIMITED;
        if (hasPermission) {
          get().setNotificationEnabled(true);
        } else {
          // 用户已拒绝过，只能跳转系统设置手动开启
          openSettings().catch(() => {
            if (Platform.OS === 'ios') {
              Linking.openURL('app-settings:');
            }
          });
        }
      } catch (error) {
        console.error('[useSettingsStore] Failed to request notification enable:', error);
      }
    },
  }),
);

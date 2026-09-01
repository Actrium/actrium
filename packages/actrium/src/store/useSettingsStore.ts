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
  /** Apply check privileges on startup, apply active bullet windows without request */
  syncNotificationPermission: () => Promise<void>;
  /** Only check the permission status and sync the switch, no permission to apply for a bullet window */
  checkNotificationPermission: () => Promise<void>;
  /** Try to open notification: switch with permission, jump system settings without permission */
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

        // denied = Never asked for permission. Request for permission to active-ball system.
        if (status === RESULTS.DENIED) {
          const result = await requestNotifications(['alert', 'badge', 'sound']);
          const granted = result.status === RESULTS.GRANTED || result.status === RESULTS.LIMITED;
          if (!granted) {
            get().setNotificationEnabled(false);
          }
          return;
        }

        // blocked = User has refused, no more window-blasting, only synchronized status
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
          // User has refused, only jump system settings are manually activated
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

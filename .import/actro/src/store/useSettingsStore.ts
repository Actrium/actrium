import { create } from 'zustand';

export interface NotificationSettings {
  task: boolean;
  accountSecurity: boolean;
  systemUpdate: boolean;
  operation: boolean;
}

export type NotificationKey = keyof NotificationSettings;

interface AppSettings {
  notification: NotificationSettings;
}

const DEFAULT_NOTIFICATION_SETTINGS: NotificationSettings = {
  task: true,
  accountSecurity: true,
  systemUpdate: true,
  operation: true,
};

const DEFAULT_SETTINGS: AppSettings = {
  notification: DEFAULT_NOTIFICATION_SETTINGS,
};

interface AppSettingsState {
  settings: AppSettings;
  updateNotificationSetting: (key: NotificationKey, value: boolean) => void;
}

export const useSettingsStore = create<AppSettingsState>(
  (set) => ({
    settings: DEFAULT_SETTINGS,

    updateNotificationSetting: (key: NotificationKey, value: boolean) => {
      set((state) => ({
        settings: {
          ...state.settings,
          notification: {
            ...state.settings.notification,
            [key]: value,
          },
        },
      }));
    },
  }),
);

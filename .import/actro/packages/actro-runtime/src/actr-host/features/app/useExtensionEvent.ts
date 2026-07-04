import { HandlerRegistry } from 'askit';
import { useTranslation } from 'react-i18next';
import { Linking } from 'react-native';
import { useAppStore } from '../../../../src/actr-host/store/useAppStore';
import { useTabIdMapStore } from '../../../../src/actr-host/store/useTabIdMapStore';
import { useMessageStore } from '../../../../src/actr-host/store/useMessageStore';
import { resources } from '../../../../src/i18n';
import { isAndroid } from '../../../../src/utils/device';
import { EmailBridge } from '../../../../src/native-bridge';

export const useExtensionEvent = (
  tabId: string,
  { closePhonePanel }: { closePhonePanel: () => void },
): HandlerRegistry => {
  const { i18n } = useTranslation();

  return {
    /**
     * 获取应用信息
     * */
    GET_APP_INFO: {
      responseEvent: 'SEND_APP_INFO',
      handle: async (payload) => {
        const appId = useTabIdMapStore.getState().tabIdMap[tabId];
        const appinfo = useAppStore.getState().apps[appId];
        // 模拟获取应用信息的逻辑
        const data = {
          requestId: payload.requestId,
          appName: appinfo.app_name,
          logo: appinfo.info.logo,
          languageContents: appinfo.language_contents,
          favoriteCount: 1234,
          usedCount: 12345678,
          author: 'bot',
        };
        return data;
      },
    },
    /**
     * 清除聊天记录
     */
    CLEAR_CHAT_HISTORY: {
      responseEvent: 'CLEAR_CHAT_HISTORY_RESULT',
      handle: async (payload) => {
        console.log('Host 清除聊天记录', payload);
        const cleanMessage = useMessageStore.getState().cleanMessage;
        cleanMessage(tabId);
        return {
          success: true,
          requestId: payload.requestId,
        };
      },
    },
    /**
     * 关闭扩展显示
     */
    CLOSE_EXTENSION: {
      responseEvent: 'CLOSE_EXTENSION_RESULT',
      handle: async (payload) => {
        console.log('Host 关闭应用', payload);
        closePhonePanel();
        return {
          success: true,
          requestId: payload.requestId,
        };
      },
    },
    /**
     * 获取语言列表
     */
    GET_LANGUAGE_LIST: {
      responseEvent: 'LANGUAGE_LIST',
      handle: async (payload) => {
        console.log('Host 获取当前语言', payload);
        return {
          requestId: payload.requestId,
          current: i18n.language,
          languages: Object.keys(resources),
        };
      },
    },
    /**
     * 设置语言
     */
    SET_APP_LANGUAGE: {
      responseEvent: 'SET_APP_LANGUAGE_RESULT',
      handle: async (payload) => {
        console.log('设置语言');
        if (payload.language) {
          i18n.changeLanguage(payload.language);
        }
        return {
          requestId: payload.requestId,
          success: true,
        };
      },
    },
    SEND_EMAIL: {
      responseEvent: 'SEND_EMAIL_RESULT',
      handle: async (payload) => {
        if (payload.to && payload.subject) {
          if (isAndroid) {
            await EmailBridge.composeEmail({
              to: [payload.to],
              subject: payload.subject,
            });
          } else {
            await Linking.openURL(
              `mailto:${payload.to}?subject=${payload.subject}`,
            );
          }
        }
        return {
          requestId: payload.requestId,
          success: true,
        };
      },
    },
  };
};

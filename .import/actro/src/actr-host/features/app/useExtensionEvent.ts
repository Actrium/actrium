import { HandlerRegistry } from 'askit';
import { useAppStore } from '../../../../src/actr-host/store/useAppStore';
import { useTabIdMapStore } from '../../../../src/actr-host/store/useTabIdMapStore';

export const useExtensionEvent = (tabId: string): HandlerRegistry => {
  //   const appinfo = useAppStore((state) => state.apps[tabId]);

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
        return {
          success: true,
          requestId: payload.requestId,
        };
      },
    },
  };
};

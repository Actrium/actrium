import {
  type HandlerRegistry,
  HostHttpHandler,
} from 'askit/host-events';
import { useTranslation } from 'react-i18next';
import { Linking } from 'react-native';
import { useAppStore } from '../../../../src/runtime-host/store/useAppStore';
import { useTabIdMapStore } from '../../../../src/runtime-host/store/useTabIdMapStore';
import { useMessageStore } from '../../../../src/runtime-host/store/useMessageStore';
import { resources } from '../../../../src/i18n';
import { isAndroid } from '../../../../src/utils/device';
import { EmailBridge } from '../../../../src/native-bridge';

export const useExtensionEvent = (
  tabId: string,
  {
    closePhonePanel,
    permissions,
  }: { closePhonePanel: () => void; permissions: string[] },
): HandlerRegistry => {
  const { i18n } = useTranslation();

  return {
    ACTR_CALL: {
      responseEvent: 'ACTR_CALL_RESULT',
      handle: async (payload) => {
        const requestId = payload?.requestId;
        if (!permissions.includes('actor.call')) {
          return {
            requestId,
            success: false,
            error: 'The Actor package does not declare the actor.call capability.',
          };
        }

        if (
          typeof payload?.routeKey !== 'string' ||
          typeof payload?.payloadBase64 !== 'string'
        ) {
          return {
            requestId,
            success: false,
            error: 'ACTR_CALL requires routeKey and payloadBase64 strings.',
          };
        }

        const gatewayUrl =
          globalThis.__ACTRIUM_ACTR_GATEWAY_URL__ ??
          'http://127.0.0.1:8391/v1/call';
        const result = await HostHttpHandler.handleRequest<{
          success: boolean;
          payloadBase64?: string;
          error?: string;
        }>({
          requestId,
          url: gatewayUrl,
          method: 'POST',
          body: {
            routeKey: payload.routeKey,
            payloadBase64: payload.payloadBase64,
            timeoutMs:
              typeof payload.timeoutMs === 'number' ? payload.timeoutMs : 30_000,
          },
        });

        return {
          requestId,
          success: result.success && result.data?.success === true,
          payloadBase64: result.data?.payloadBase64,
          error: result.data?.error ?? result.message,
        };
      },
    },
    /**
     * Access to application information
     * */
    GET_APP_INFO: {
      responseEvent: 'SEND_APP_INFO',
      handle: async (payload) => {
        const appId = useTabIdMapStore.getState().tabIdMap[tabId];
        const appinfo = useAppStore.getState().apps[appId];
        // Simulates the logic of accessing applied information
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
     * Clear Chat Record
     */
    CLEAR_CHAT_HISTORY: {
      responseEvent: 'CLEAR_CHAT_HISTORY_RESULT',
      handle: async (payload) => {
        console.log('Host Clear Chat Record', payload);
        const cleanMessage = useMessageStore.getState().cleanMessage;
        cleanMessage(tabId);
        return {
          success: true,
          requestId: payload.requestId,
        };
      },
    },
    /**
     * Close Extension Display
     */
    CLOSE_EXTENSION: {
      responseEvent: 'CLOSE_EXTENSION_RESULT',
      handle: async (payload) => {
        console.log('Host Close Application', payload);
        closePhonePanel();
        return {
          success: true,
          requestId: payload.requestId,
        };
      },
    },
    /**
     * Get Language List
     */
    GET_LANGUAGE_LIST: {
      responseEvent: 'LANGUAGE_LIST',
      handle: async (payload) => {
        console.log('Host Get Current Language', payload);
        return {
          requestId: payload.requestId,
          current: i18n.language,
          languages: Object.keys(resources),
        };
      },
    },
    /**
     * Set Language
     */
    SET_APP_LANGUAGE: {
      responseEvent: 'SET_APP_LANGUAGE_RESULT',
      handle: async (payload) => {
        console.log('Set Language');
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

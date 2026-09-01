import { getAppInfo } from '../api/app';
import { authApi, CURRENT_ENV } from '../api/auth';
import { AppRepository } from '../repository/AppRepository';
import { AuthRepository } from '../repository/AuthRepository';
import { useAppStore } from '../actr-host/store/useAppStore';
import { useUserStore } from '../actr-host/store/useUserStore';
import { useAuthTaskStore } from '../actr-host/store/useAuthTaskStore';
import { AccountUserInfo } from '../api/types';
import { actrLoader } from '../actr-host/utils';
import { updateTabInfo } from '../native-bridge/tab/TabModule';
import {
  AUTH_ACCESS_CONTROL_RESPONSE_CONTRACT,
  authAccessControlStatus,
  authIsClientAppNotFoundResponse,
} from '../actr-host/coreContract';

export type TInitSessionResult =
  | {
      ok: true;
    }
  | {
      ok: false;
      reason: 'APP_NOT_FOUND' | 'NETWORK_ERROR' | 'UNKNOWN';
      message?: string;
    };

/**
 * AppSessionService
 * Logic of processing initialization of applied sessions, synchronisation of user information and verification of permissions
 */
export class AppSessionService {
  /**
   * Initialise Session
   * Include: Access App Information, Sync user information, verify authorization status
   *
   * @param appId Apply ID
   * @param url Apply Load Source Address
   */
  static async initSession(
    tabId: string,
    appId: string,
    url?: string,
  ): Promise<TInitSessionResult> {
    if (!appId) {
      return {
        ok: false,
        reason: 'APP_NOT_FOUND',
      };
    }

    try {
      console.log(
        `[AppSessionService] Initializing session for appId: ${appId}, url: ${url}`,
      );

      // 1. Get and save App Information
      const appRes = await getAppInfo(appId);

      // The back end will put“Application does not exist”The blogger says that the government is not a party to the law.
      // This kind of response, though, success=false, but semantics are not web errors and require first recognition alone.
      if (authIsClientAppNotFoundResponse(appRes)) {
        return {
          ok: false,
          reason: 'APP_NOT_FOUND',
          message: appRes?.message,
        };
      }

      // Remaining success=false The government has been unable to provide the necessary information.
      // Like the network anomaly,HTTP Not 2xxThe situation is not clear.
      if (!appRes?.success) {
        return {
          ok: false,
          reason: 'NETWORK_ERROR',
          message: appRes?.message,
        };
      }

      if (!appRes?.data) {
        return {
          ok: false,
          reason: 'APP_NOT_FOUND',
        };
      }

      // Will url Mount to appInfo objects to be stored in a uniform format Store and SQLite
      if (url) {
        appRes.data.url = url;
      }
      appRes.data.app_status = 'alive';

      // Update native Location tab Information
      void updateTabInfo({
        tabId: tabId,
        title: appRes.data.instance_name,
        icon: appRes.data.info?.logo || '',
        appUrl: appRes.data.url || '',
      })

      useAppStore.getState().setAppInfo(appId, appRes.data);
      await AppRepository.saveApp(appRes.data);

      // 🆕 2. Parsing guest_url (.actr Documentation)
      const guestUrl = appRes.data.actr_url;
      const guestHashCode = appRes.data.actr_hash_code;
      if (guestUrl?.includes('.actr') && guestHashCode) {
        try {
          console.log(
            `[AppSessionService] Loading and verifying .actr: ${guestUrl}`,
          );

          // Download,zip hash Validate, depress and manifest Read it all. ActrLoader The project is now being completed.
          // Avoid checking for the double-net request for the first download, and for the second download.
          const actrPackage = await actrLoader.load(guestUrl, appId, {
            expectedSha256: guestHashCode,
          });

          // Press appId Storage guest script
          useAppStore
            .getState()
            .setGuestScript(appId, actrPackage.unifiedScript);

          console.log(
            `[AppSessionService] Guest file loaded and verified for appId: ${appId}, script size: ${actrPackage.unifiedScript.length} bytes`,
          );
        } catch (error) {
          console.error(
            `[AppSessionService] Failed to load .actr for appId ${appId}:`,
            error,
          );
          // Load failure does not affect other functions
        }
      } else {
        console.log('[AppSessionService] actr_url is empty');
      }

      // 2. Access Token
      const { userInfo, sessionToken, setUserInfo, setSessionToken } =
        useUserStore.getState();
      let token = sessionToken;
      if (!token) {
        token = await AuthRepository.getToken(CURRENT_ENV);
      }

      if (token) {
        // 3. Synchronise User Information / Login Status
        let currentUser = userInfo;
        if (!currentUser) {
          try {
            const userRes = await authApi.getUserInfo(token);
            if (userRes.success && userRes.data) {
              currentUser = userRes.data as AccountUserInfo;
              setUserInfo(currentUser);
              setSessionToken(token);
            }
          } catch (e) {
            console.error('[AppSessionService] Failed to sync user info:', e);
          }
        }

        // 4. Verify Authorized Status
        if (currentUser?.user?.cn) {
          try {
            const authRes = await authApi.getIsAuthorized({
              appId,
              userId: currentUser.user.cn,
            });
            const authStatus = authAccessControlStatus(authRes.success, authRes.code);
            if (authStatus === AUTH_ACCESS_CONTROL_RESPONSE_CONTRACT.statuses.authorized) {
              useAuthTaskStore.getState().addAuthorizedAppId(appId);
            } else if (authStatus === AUTH_ACCESS_CONTROL_RESPONSE_CONTRACT.statuses.notAuthorized) {
              useAuthTaskStore.getState().addUnauthorizedAppId(appId);
            }
          } catch (e) {
            console.error(
              '[AppSessionService] Failed to check authorization status:',
              e,
            );
          }
        }
      }
      return { ok: true };
    } catch (error) {
      console.error('[AppSessionService] Initialization failed:', error);
      const message =
        error instanceof Error ? error.message : 'Unknown init session error';

      if (typeof message === 'string') {
        const lower = message.toLowerCase();
        if (
          lower.includes('network') ||
          lower.includes('timeout') ||
          lower.includes('fetch')
        ) {
          return {
            ok: false,
            reason: 'NETWORK_ERROR',
            message,
          };
        }
      }

      return {
        ok: false,
        reason: 'UNKNOWN',
        message,
      };
    }
  }
}

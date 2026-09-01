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
 * 处理应用会话的初始化、用户信息同步和权限校验等逻辑
 */
export class AppSessionService {
  /**
   * 初始化应用会话
   * 包含：获取 App 信息、同步用户信息、校验授权状态
   *
   * @param appId 应用 ID
   * @param url 应用加载来源地址
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

      // 1. 获取并保存 App 信息
      const appRes = await getAppInfo(appId);

      // 后端会把“应用不存在”放在业务错误码里返回，
      // 这类响应虽然 success=false，但语义不是网络错误，需要先单独识别。
      if (authIsClientAppNotFoundResponse(appRes)) {
        return {
          ok: false,
          reason: 'APP_NOT_FOUND',
          message: appRes?.message,
        };
      }

      // 剩余的 success=false 才按链路失败处理，
      // 例如网络异常、HTTP 非 2xx、权限链路未决等场景。
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

      // 将 url 挂载到 appInfo 对象中，以便统一存储到 Store 和 SQLite
      if (url) {
        appRes.data.url = url;
      }
      appRes.data.app_status = 'alive';

      // 更新 native 处 tab 信息
      void updateTabInfo({
        tabId: tabId,
        title: appRes.data.instance_name,
        icon: appRes.data.info?.logo || '',
        appUrl: appRes.data.url || '',
      })

      useAppStore.getState().setAppInfo(appId, appRes.data);
      await AppRepository.saveApp(appRes.data);

      // 🆕 2. 解析 guest_url (.actr 文件)
      const guestUrl = appRes.data.actr_url;
      const guestHashCode = appRes.data.actr_hash_code;
      if (guestUrl?.includes('.actr') && guestHashCode) {
        try {
          console.log(
            `[AppSessionService] Loading and verifying .actr: ${guestUrl}`,
          );

          // 下载、zip hash 校验、解压和 manifest 读取都在 ActrLoader 内完成，
          // 避免校验第一次下载、运行第二次下载的双网络请求。
          const actrPackage = await actrLoader.load(guestUrl, appId, {
            expectedSha256: guestHashCode,
          });

          // 按 appId 存储 guest script
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
          // 加载失败不影响其他功能
        }
      } else {
        console.log('[AppSessionService] actr_url is empty');
      }

      // 2. 获取 Token
      const { userInfo, sessionToken, setUserInfo, setSessionToken } =
        useUserStore.getState();
      let token = sessionToken;
      if (!token) {
        token = await AuthRepository.getToken(CURRENT_ENV);
      }

      if (token) {
        // 3. 同步用户信息 / 登录状态
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

        // 4. 校验授权状态
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

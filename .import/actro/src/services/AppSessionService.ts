import { getAppInfo } from '../api/app';
import { authApi, CURRENT_ENV } from '../api/auth';
import { AppRepository } from '../repository/AppRepository';
import { AuthRepository } from '../repository/AuthRepository';
import { useAppStore } from '../actr-host/store/useAppStore';
import { useUserStore } from '../actr-host/store/useUserStore';
import { useAuthTaskStore } from '../actr-host/store/useAuthTaskStore';
import { AccountUserInfo } from '../api/types';
import { actrLoader } from '../actr-host/utils';
import { Utils } from '../utils/utils';
import { updateTabInfo } from '../native-bridge/tab/TabModule';

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
  private static isAppNotFoundResponse(response: {
    code?: number;
    message?: string;
    data?: unknown;
  }): boolean {
    if (response.code === 10010) {
      return true;
    }

    const message = response.message?.toLowerCase() || '';
    return (
      message.includes('app not found') ||
      message.includes('应用不存在')
    );
  }

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

      // 中文注释：后端会把“应用不存在”放在业务错误码里返回，
      // 这类响应虽然 success=false，但语义不是网络错误，需要先单独识别。
      if (AppSessionService.isAppNotFoundResponse(appRes)) {
        return {
          ok: false,
          reason: 'APP_NOT_FOUND',
          message: appRes?.message,
        };
      }

      // 中文注释：剩余的 success=false 才按链路失败处理，
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
            `[AppSessionService] Fetching binary for hash: ${guestUrl}`,
          );

          // 🆕 1. 直接获取文件二进制数据用于计算哈希
          const response = await fetch(guestUrl);
          if (!response.ok) {
            throw new Error(
              `Failed to fetch .actr file for hash calculation: ${response.status} ${response.statusText}`,
            );
          }
          const arrayBuffer = await response.arrayBuffer();
          const fileData = new Uint8Array(arrayBuffer);

          // 判断文件 sha256 是否正确
          const sha256 = await Utils.sha256(fileData);
          if (sha256 !== guestHashCode) {
            console.error(
              `[AppSessionService] Guest file SHA-256 mismatch for appId ${appId}:`,
              `sha256: ${sha256}`,
              `guestHashCode: ${guestHashCode}`,
            );
          }

          // 🆕 2. 加载并解析 .actr 包
          const actrPackage = await actrLoader.load(guestUrl, appId);

          // 如果哈希校验通过，或者即使失败也尝试运行（取决于业务决策，这里保持校验逻辑）
          if (sha256 === guestHashCode) {
            // 按 appId 存储 guest script
            useAppStore
              .getState()
              .setGuestScript(appId, actrPackage.unifiedScript);

            console.log(
              `[AppSessionService] Guest file loaded and verified for appId: ${appId}, script size: ${actrPackage.unifiedScript.length} bytes`,
            );
          }
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
            if (authRes.success && authRes.code === 0) {
              useAuthTaskStore.getState().addAuthorizedAppId(appId);
            } else if (authRes.success) {
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

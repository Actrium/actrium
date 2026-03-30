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
import { updateTabInfo } from '../../src/native-bridge/tab/TabModule';

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

      // TODO 临时处理
      // appRes.data!.actr_url = 'https://rs1.solution9.net/test/actr.actr';

      // 这里是首跳请求，权限未决或链路异常都会先落到 success=false。
      // 如果误判成 APP_NOT_FOUND，UI 会把时序问题说成“应用不存在”，语义是错的。
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

      // 更新 native 处 tab 信息
      void updateTabInfo({
        tabId: tabId,
        title: appRes.data.app_name,
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
          const actrPackage = await actrLoader.load(guestUrl);

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
        console.warn('[AppSessionService] actr_url is empty');
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

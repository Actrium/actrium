import { db } from '../libs/Sqlite';
import { AppInfo } from '../api/types';
import {
  RUNTIME_APP_STATUS,
  RUNTIME_DB_QUERIES,
  type RuntimeAppStatus,
} from '../actr-host/coreContract';

export class AppRepository {
  /**
   * 保存或更新应用信息
   */
  static async saveApp(appInfo: AppInfo) {
    try {
      const defaultLang = appInfo.info?.default_language || 'en';
      const langContent = appInfo.language_contents?.[defaultLang];
      const appStatus = appInfo.app_status ?? RUNTIME_APP_STATUS.alive;
      const normalizedAppInfo: AppInfo = {
        ...appInfo,
        app_status: appStatus,
      };

      await db.executeSql(RUNTIME_DB_QUERIES.apps.upsert, [
        normalizedAppInfo.app_id,
        appStatus,
        normalizedAppInfo.app_name,
        normalizedAppInfo.instance_name,
        langContent?.description || '',
        langContent?.title || '',
        normalizedAppInfo.info?.logo || '',
        normalizedAppInfo.url || '', // 保存原始 URL
        JSON.stringify(normalizedAppInfo),
        Date.now(),
      ]);
      console.log(`[AppRepository] App ${normalizedAppInfo.app_id} saved to SQLite`);
    } catch (error) {
      console.error('[AppRepository] Failed to save app info', error);
    }
  }

  /**
   * 获取本地存储的应用信息
   */
  static async getApp(appId: string): Promise<AppInfo | null> {
    try {
      const rows = await db.selectAll<{ details: string; app_status: RuntimeAppStatus | null }>(
        RUNTIME_DB_QUERIES.apps.selectDetailsByAppId,
        [appId]
      );
      if (rows.length > 0) {
        const parsed = JSON.parse(rows[0].details) as AppInfo;
        return {
          ...parsed,
          app_status: rows[0].app_status ?? parsed.app_status ?? RUNTIME_APP_STATUS.alive,
        };
      }
      return null;
    } catch (error) {
      console.error('[AppRepository] Failed to fetch app', error);
      return null;
    }
  }

  /**
   * 更新最近打开时间
   */
  static async updateLastOpened(appId: string) {
    try {
      await db.executeSql(RUNTIME_DB_QUERIES.apps.updateLastOpened, [
        Date.now(),
        appId,
      ]);
    } catch (error) {
      console.error('[AppRepository] Failed to update last_opened', error);
    }
  }

  /**
   * 更新应用可用状态
   */
  static async updateAppStatus(
    appId: string,
    appStatus: RuntimeAppStatus,
  ): Promise<boolean> {
    try {
      const app = await AppRepository.getApp(appId);
      if (!app) {
        return false;
      }

      const normalizedApp = {
        ...app,
        app_status: appStatus,
      };

      await db.executeSql(RUNTIME_DB_QUERIES.apps.updateStatusAndDetails, [
        appStatus,
        JSON.stringify(normalizedApp),
        appId,
      ]);
      return true;
    } catch (error) {
      console.error('[AppRepository] Failed to update app_status', error);
      return false;
    }
  }
}

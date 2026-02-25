import { db } from '../libs/Sqlite';
import { AppInfo } from '../api/types';

export class AppRepository {
  /**
   * 保存或更新应用信息
   */
  static async saveApp(appInfo: AppInfo) {
    try {
      const defaultLang = appInfo.info?.default_language || 'en';
      const langContent = appInfo.language_contents?.[defaultLang];

      const sql = `
        INSERT OR REPLACE INTO apps 
        (app_id, app_name, full_description, full_title, logo, details, last_opened) 
        VALUES (?, ?, ?, ?, ?, ?, ?)
      `;

      await db.executeSql(sql, [
        appInfo.app_id,
        appInfo.app_name,
        langContent?.description || '',
        langContent?.title || '',
        appInfo.info?.logo || '',
        JSON.stringify(appInfo),
        Date.now(),
      ]);
      console.log(`[AppRepository] App ${appInfo.app_id} saved to SQLite`);
    } catch (error) {
      console.error('[AppRepository] Failed to save app info', error);
    }
  }

  /**
   * 获取本地存储的应用信息
   */
  static async getApp(appId: string): Promise<AppInfo | null> {
    try {
      const rows = await db.selectAll<{ details: string }>(
        'SELECT details FROM apps WHERE app_id = ?',
        [appId]
      );
      if (rows.length > 0) {
        return JSON.parse(rows[0].details);
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
      await db.executeSql('UPDATE apps SET last_opened = ? WHERE app_id = ?', [
        Date.now(),
        appId,
      ]);
    } catch (error) {
      console.error('[AppRepository] Failed to update last_opened', error);
    }
  }
}

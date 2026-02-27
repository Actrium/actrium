import { db } from '../libs/Sqlite';

export enum Progress {
  pending = 0,
  processing = 1,
  completed = 2,
  failed = 3,
  canceled = 4,
  overdue = 5,
}

export interface IMessage {
  message_id: string;
  app_id: string; // 所属 App ID
  session_id: string; // 所属会话 ID
  is_preset: number;
  question_time: number;
  question_content: string;
  answer_time: number;
  answer_content: string;
  attachments?: string;       // 附件信息 (JSON string)
  is_edited?: number;         // 是否编辑过
  is_read?: number;           // 是否已读 (0 未读/1 已读)
  progress: number;
}

export class MessageRepository {
  /**
   * 获取某 App 下的消息（支持分页）
   */
  static async getAllMessages(
    appId: string,
    limit: number = 20,
    offset: number = 0
  ): Promise<IMessage[]> {
    try {
      const rows = await db.selectAll<IMessage>(
        'SELECT * FROM app_chat_history WHERE app_id = ? ORDER BY question_time DESC LIMIT ? OFFSET ?',
        [appId, limit, offset]
      );
      return rows;
    } catch (error) {
      console.error('[MessageRepository] Failed to fetch messages', error);
      return [];
    }
  }

  /**
   * 插入或更新消息
   */
  static async saveMessage(msg: IMessage) {
    const sql = `
      INSERT OR REPLACE INTO app_chat_history
      (message_id, app_id, session_id, question_time, question_content, answer_time, answer_content, attachments, is_preset, is_edited, is_read, progress)
      VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
    `;
    return await db.executeSql(sql, [
      msg.message_id,
      msg.app_id,
      msg.session_id,
      msg.question_time,
      msg.question_content,
      msg.answer_time,
      msg.answer_content,
      msg.attachments || null,
      msg.is_preset,
      msg.is_edited || 0,
      msg.is_read || 0,
      msg.progress,
    ]);
  }

  /**
   * 删除消息
   */
  static async deleteMessage(message_id: string) {
    return await db.executeSql('DELETE FROM app_chat_history WHERE message_id = ?', [
      message_id,
    ]);
  }

  /**
   * 标记指定应用的所有消息为已读
   */
  static async markAllAsReadByAppId(appId: string): Promise<void> {
    try {
      await db.executeSql(
        'UPDATE app_chat_history SET is_read = 1 WHERE app_id = ? AND is_read = 0',
        [appId]
      );
    } catch (error) {
      console.error('[MessageRepository] Failed to mark messages as read', error);
    }
  }

  /**
   * 查询指定应用的未读消息数量
   */
  static async getUnreadCountByAppId(appId: string): Promise<number> {
    try {
      const rows = await db.selectAll<{ count: number }>(
        'SELECT COUNT(*) as count FROM app_chat_history WHERE app_id = ? AND is_read = 0',
        [appId]
      );
      return rows[0]?.count || 0;
    } catch (error) {
      console.error('[MessageRepository] Failed to get unread count', error);
      return 0;
    }
  }

  /**
   * 查询所有应用的未读消息数量（返回 appId -> count 的映射）
   */
  static async getAllUnreadCounts(): Promise<Record<string, number>> {
    try {
      const rows = await db.selectAll<{ app_id: string; count: number }>(
        'SELECT app_id, COUNT(*) as count FROM app_chat_history WHERE is_read = 0 GROUP BY app_id'
      );
      return rows.reduce((acc, row) => {
        acc[row.app_id] = row.count;
        return acc;
      }, {} as Record<string, number>);
    } catch (error) {
      console.error('[MessageRepository] Failed to get all unread counts', error);
      return {};
    }
  }

  /**
   * 检查指定应用是否有未读消息
   */
  static async hasUnreadMessages(appId: string): Promise<boolean> {
    try {
      const rows = await db.selectAll<{ exists: number }>(
        'SELECT EXISTS(SELECT 1 FROM app_chat_history WHERE app_id = ? AND is_read = 0 LIMIT 1) as exists',
        [appId]
      );
      return rows[0]?.exists === 1;
    } catch (error) {
      console.error('[MessageRepository] Failed to check unread messages', error);
      return false;
    }
  }
}

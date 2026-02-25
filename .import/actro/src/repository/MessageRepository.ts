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
      (message_id, app_id, session_id, question_time, question_content, answer_time, answer_content, attachments, is_preset, is_edited, progress) 
      VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
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
}

import { db } from '../libs/Sqlite';
import {
  RUNTIME_DB_QUERIES,
  RUNTIME_MESSAGE_PROGRESS,
  RUNTIME_MESSAGE_TYPES,
  type RuntimeMessageProgress,
  type RuntimeMessageType,
} from '../actr-host/coreContract';

export const Progress = RUNTIME_MESSAGE_PROGRESS;

export type MessageType = RuntimeMessageType;

export interface IVoicePayload {
  local_file_uri: string;      // 本地文件 URI，供播放器直接使用
  local_file_path: string;     // 本地文件绝对路径，供文件检查/删除使用
  duration_ms: number;         // 语音时长（毫秒）
  mime_type: string;           // 文件 MIME 类型，例如 audio/wav
  file_size: number;           // 文件大小（字节）
  sample_rate: number;         // 采样率，例如 16000
  channels: number;            // 声道数，当前通常为 1（单声道）
  bits_per_sample: number;     // 每个采样点的位深，例如 16
  status: 'recording' | 'ready'; // 本地语音文件状态：录制中 / 已就绪
}

export interface IMessage {
  message_id: string;
  app_id: string; // 所属 App ID
  tab_id: string; // 所属 Tab ID
  session_id: string; // 所属会话 ID
  question_type?: MessageType;          // 提问类型 (text/voice)
  is_preset: number;
  question_time: number;
  question_content: string;
  question_voice_payload?: string;      // 提问语音元数据 (JSON string)
  answer_type?: MessageType;            // 回答类型 (text/voice)
  answer_time: number;
  answer_content: string;
  answer_voice_payload?: string;        // 回答语音元数据 (JSON string)
  attachments?: string;                 // 附件信息 (JSON string)
  is_edited?: number;                   // 是否编辑过
  is_read?: number;                     // 是否已读 (0 未读/1 已读)
  progress: RuntimeMessageProgress;
}

export class MessageRepository {
  /**
   * 获取某 Tab 下的消息（支持分页）
   */
  static async getAllMessages(
    tabId: string,
    limit: number = 20,
    offset: number = 0
  ): Promise<IMessage[]> {
    try {
      const rows = await db.selectAll<IMessage>(
        RUNTIME_DB_QUERIES.messages.selectByTabId,
        [tabId, limit, offset]
      );
      return rows;
    } catch (error) {
      console.error('[MessageRepository] Failed to fetch messages', error);
      return [];
    }
  }

  /**
   * 检查指定 tab + app 是否存在历史消息
   */
  static async hasMessagesByTabIdAndAppId(
    tabId: string,
    appId: string,
  ): Promise<boolean> {
    try {
      const rows = await db.selectAll<{ count: number }>(
        RUNTIME_DB_QUERIES.messages.countByTabIdAndAppId,
        [tabId, appId],
      );
      return (rows[0]?.count || 0) > 0;
    } catch (error) {
      console.error(
        '[MessageRepository] Failed to check messages by tabId and appId',
        error,
      );
      return false;
    }
  }

  /**
   * 插入或更新消息
   */
  static async saveMessage(msg: IMessage) {
    return await db.executeSql(RUNTIME_DB_QUERIES.messages.upsert, [
      msg.message_id,
      msg.app_id,
      msg.tab_id,
      msg.session_id,
      msg.question_type || RUNTIME_MESSAGE_TYPES.text,
      msg.question_time,
      msg.question_content,
      msg.question_voice_payload || null,
      msg.answer_type || RUNTIME_MESSAGE_TYPES.text,
      msg.answer_time,
      msg.answer_content,
      msg.answer_voice_payload || null,
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
    return await db.executeSql(RUNTIME_DB_QUERIES.messages.deleteByMessageId, [
      message_id,
    ]);
  }

  /**
   * 删除指定 Tab 的全部消息
   */
  static async deleteMessagesByTabId(tabId: string) {
    return await db.executeSql(RUNTIME_DB_QUERIES.messages.deleteByTabId, [
      tabId,
    ]);
  }

  /**
   * 标记指定 Tab 的所有消息为已读
   */
  static async markAllAsReadByTabId(tabId: string): Promise<void> {
    try {
      await db.executeSql(
        RUNTIME_DB_QUERIES.messages.markTabAsRead,
        [tabId]
      );
    } catch (error) {
      console.error('[MessageRepository] Failed to mark messages as read', error);
    }
  }

  /**
   * 查询指定 Tab 的未读消息数量
   */
  static async getUnreadCountByTabId(tabId: string): Promise<number> {
    try {
      const rows = await db.selectAll<{ count: number }>(
        RUNTIME_DB_QUERIES.messages.countUnreadByTabId,
        [tabId]
      );
      return rows[0]?.count || 0;
    } catch (error) {
      console.error('[MessageRepository] Failed to get unread count', error);
      return 0;
    }
  }

  /**
   * 查询所有 Tab 的未读消息数量（返回 tabId -> count 的映射）
   */
  static async getAllUnreadCounts(): Promise<Record<string, number>> {
    try {
      const rows = await db.selectAll<{ tab_id: string; count: number }>(
        RUNTIME_DB_QUERIES.messages.countUnreadByAllTabs
      );
      return rows.reduce((acc, row) => {
        acc[row.tab_id] = row.count;
        return acc;
      }, {} as Record<string, number>);
    } catch (error) {
      console.error('[MessageRepository] Failed to get all unread counts', error);
      return {};
    }
  }

  /**
   * 检查指定 Tab 是否有未读消息
   */
  static async hasUnreadMessages(tabId: string): Promise<boolean> {
    try {
      const rows = await db.selectAll<{ exists: number }>(
        RUNTIME_DB_QUERIES.messages.existsUnreadByTabId,
        [tabId]
      );
      return rows[0]?.exists === 1;
    } catch (error) {
      console.error('[MessageRepository] Failed to check unread messages', error);
      return false;
    }
  }
}

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
  local_file_uri: string;      // Local File URI, for the player to use directly
  local_file_path: string;     // Absolute path to local files for file checking/Remove Usage
  duration_ms: number;         // Voice length (ms)
  mime_type: string;           // Documentation MIME Type, e.g. audio/wav
  file_size: number;           // File Size (bytes)
  sample_rate: number;         // Sample rate, e.g. 16000
  channels: number;            // Channels, which are now usually 1(Unional channel)
  bits_per_sample: number;     // Depth of each sample point, for example 16
  status: 'recording' | 'ready'; // Local voice file status: recording / Ready
}

export interface IMessage {
  message_id: string;
  app_id: string; // belong to App ID
  tab_id: string; // belong to Tab ID
  session_id: string; // Organisation ID
  question_type?: MessageType;          // Question type (text/voice)
  is_preset: number;
  question_time: number;
  question_content: string;
  question_voice_payload?: string;      // Question voice metadata (JSON string)
  answer_type?: MessageType;            // Type of answer (text/voice)
  answer_time: number;
  answer_content: string;
  answer_voice_payload?: string;        // Answer voice metadata (JSON string)
  attachments?: string;                 // Can not open message (JSON string)
  is_edited?: number;                   // Edited or not
  is_read?: number;                     // Read it or not? (0 Unread/1 Read)
  progress: RuntimeMessageProgress;
}

export class MessageRepository {
  /**
   * Get some Tab Next Message (Supporting page break)
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
   * Check Assign tab + app Is there history?
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
   * Insert or update messages
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
   * Can not open message
   */
  static async deleteMessage(message_id: string) {
    return await db.executeSql(RUNTIME_DB_QUERIES.messages.deleteByMessageId, [
      message_id,
    ]);
  }

  /**
   * Delete Assign Tab All messages
   */
  static async deleteMessagesByTabId(tabId: string) {
    return await db.executeSql(RUNTIME_DB_QUERIES.messages.deleteByTabId, [
      tabId,
    ]);
  }

  /**
   * Tag Assign Tab Other Organiser
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
   * Query Assign Tab Number of unread messages
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
   * Query All Tab Number of unread messages (returns) tabId -> count Map)
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
   * Check Assign Tab Is there any unread messages?
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

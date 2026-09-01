import { useEffect, useMemo } from 'react';
import RNFS from 'react-native-fs';
import {
  ActrConnectionManager,
  generateActrToml,
  generateActrLockToml,
  AttachmentType,
  TActrMessageResponse,
  TLocation,
} from '../libs/actr';
import { AppInfo } from '../api/types';
import { IAttachmentItem } from '../actr-host/types/attachment';
import { Utils } from '../utils/utils';
import { useMessageStore } from '../actr-host/store/useMessageStore';
import { Progress } from '../repository/MessageRepository';
import { GlobalAlert } from '../components';
import i18n from '../i18n';

/**
 * useActrConnection Hook
 * 负责管理 ActrConnection 的生命周期，自动处理绑定 and 解绑
 *
 * @param appId 应用 ID
 * @param config 动态配置信息
 * @param callbacks 消息回调
 */
export function useActrConnection({
  appId,
  tabId,
  config,
  callbacks,
}: {
  appId: string;
  tabId: string;
  config?: {
    app_id: string;
    key_id: string;
    authority_services: AppInfo['authority_services'];
    signaling_services: AppInfo['signaling_services'];
    stun_services: AppInfo['stun_services'];
    turn_services: AppInfo['turn_services'];
    server_actor_type: AppInfo['server_actor_type'];
    realm_id: AppInfo['realm_id'];
  };
  callbacks?: {
    onMessage?: (data: TActrMessageResponse) => void;
    onVoice?: (data: TActrMessageResponse) => void;
  };
}) {
  const connection = useMemo(() => {
    if (!appId || !config?.server_actor_type) return null;

    return ActrConnectionManager.getOrCreate({
      appId,
      tabId,
      actorType: `${config?.server_actor_type?.manufacturer}+${config?.server_actor_type?.name}`,
      // actorType: 'bot+bot-service'
    });
  }, [appId, config]);

  // 处理动态初始化
  useEffect(() => {
    // 要等待 guest 解析完再连接，否则会连续触发多次
    if (!connection || !config || !appId) return;

    // 如果尚未连接，则使用传入的配置进行初始化
    if (!connection.isConnected()) {
      const runInit = async () => {
        try {
          const { manufacturer, name } = config.server_actor_type || {};

          // 1. 在 RN 侧生成 TOML 字符串
          const tomlString = generateActrToml({
            ...config,
            manufacturer,
            name,
          });
          const lockTomlString = generateActrLockToml({ manufacturer, name });

          // 2. 生成配置文件
          const fileName = `Actr.toml`;
          const lockFileName = `Actr.lock.toml`;
          const configPath = `${RNFS.TemporaryDirectoryPath}${fileName}`;
          const lockConfigPath = `${RNFS.TemporaryDirectoryPath}${lockFileName}`;

          await RNFS.writeFile(configPath, tomlString, 'utf8');
          await RNFS.writeFile(lockConfigPath, lockTomlString, 'utf8');
          console.log(
            `[useActrConnection:${appId}] ${fileName}, ${lockFileName} generated ${configPath}`,
          );

          await connection.initialize(configPath);
        } catch (err: any) {
          // console.error(
          //   `[useActrConnection:${appId}] Auto initialization failed:`,
          //   err.message || err,
          // );
        }
      };

      runInit();
    }
  }, [connection, !!config, appId]);

  // 管理连接引用计数和资源回收
  useEffect(() => {
    if (!connection) return;

    connection.acquire();
    return () => {
      connection.release();
    };
  }, [connection]);

  useEffect(() => {
    if (!connection) return;

    // 如果没有任何回调，说明只是单纯获取实例，不执行订阅逻辑
    if (!callbacks?.onMessage && !callbacks?.onVoice) return;

    console.log(`[useActrConnection:${appId}] Subscribing to messages`);

    let unsubscribeChat: (() => void) | undefined;
    let unsubscribeVoice: (() => void) | undefined;

    if (callbacks?.onMessage) {
      unsubscribeChat = connection.subscribeChatMessage(callbacks.onMessage);
    }
    if (callbacks?.onVoice) {
      unsubscribeVoice = connection.subscribeVoiceMessage(callbacks.onVoice);
    }

    return () => {
      console.log(`[useActrConnection:${appId}] Unsubscribing from messages`);
      unsubscribeChat?.();
      unsubscribeVoice?.();
    };
  }, [connection, callbacks?.onMessage, callbacks?.onVoice, appId]);

  // 发送消息
  const sendMessage = async ({
    message_id,
    session_id,
    message,
    attachments,
    location,
  }: {
    message: string;
    message_id: string;
    session_id: string;
    attachments?: IAttachmentItem[];
    location?: TLocation;
  }) => {
    if (!connection) return;

    try {
      const attachmentIds = attachments?.length
        ? await uploadAttachment(attachments, session_id)
        : [];

      const reqMessage = {
        messageId: message_id, // message_id
        sessionId: session_id, // session_id
        text: message, // Text question (if applicable)
        location: location, // Optional location context
        attachmentIds: attachmentIds, // Optional: Attachment IDs
      };

      const res = await connection.sendMessage(reqMessage);
      return res;
    } catch (e: any) {
      useMessageStore.getState().updateMessage(
        tabId,
        message_id,
        {
          progress: Progress.failed,
          answer_time: Date.now(),
        },
        true,
      );
      GlobalAlert.alert(i18n.t('actr.chat.sendMessageFailedTitle'), e.message);
      return null;
    }
  };

  /**
   * 上传附件，拿到 ids
   */
  const uploadAttachment = async (
    attachments: IAttachmentItem[],
    sessionId: string,
  ): Promise<string[]> => {
    const ids = await Promise.all(
      attachments.map(async (item) =>
        connection!.uploadAttachment({
          // prepare/result 协议都以 attachmentId 作为唯一键，这里优先复用输入态附件 ID 与宿主对齐。
          attachmentId: item.id || Utils.generateId(),
          sessionId,
          fileName: item.file_name || 'unknown',
          filePath: item.local_url,
        }),
      ),
    );
    return ids;
  };

  /**
   * 上传音频开始，通知 native
   */
  const startVoice = async ({
    message_id,
    session_id,
    attachments,
    location,
  }: {
    message_id: string;
    session_id: string;
    attachments?: IAttachmentItem[];
    location?: TLocation;
  }) => {
    const attachmentIds = attachments?.length
      ? await uploadAttachment(attachments, session_id)
      : [];

    const req = {
      sessionId: session_id,
      messageId: message_id,
      location: location,
      attachmentIds: attachmentIds,
    };

    return await connection!.startVoice(req);
  };

  /**
   * 上传音频，拿到 id
   */
  const sendVoiceChunk = async (chunk: Uint8Array) =>
    await connection!.sendVoiceChunk(chunk);

  /**
   * 上传音频结束，通知 native，拿到 id
   */
  const endVoice = async () => await connection!.endVoice();

  return {
    sendMessage,
    startVoice,
    sendVoiceChunk,
    endVoice,
    session_id: connection?.session_id || '',
    connection,
  };
}

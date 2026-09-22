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
import { IAttachmentItem } from '../runtime-host/types/attachment';
import { Utils } from '../utils/utils';
import { useMessageStore } from '../runtime-host/store/useMessageStore';
import { Progress } from '../repository/MessageRepository';
import { GlobalAlert } from '../components';
import i18n from '../i18n';

/**
 * useActrConnection Hook
 * Management ActrConnection ♪ The life cycle of the world, the auto-processing of bindings ♪ and Untie.
 *
 * @param appId Apply ID
 * @param config Dynamic Configuration Information
 * @param callbacks Message echoes
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
      actorType: `${config?.server_actor_type?.publisher}+${config?.server_actor_type?.name}`,
      // actorType: 'bot+bot-service'
    });
  }, [appId, config]);

  // Process dynamic initialization
  useEffect(() => {
    // Wait. guest Once you've been parsed and connected, you can trigger it multiple times in a row.
    if (!connection || !config || !appId) return;

    // Initialize with the incoming configuration if not connected
    if (!connection.isConnected()) {
      const runInit = async () => {
        try {
          const { publisher, name } = config.server_actor_type || {};

          // 1. Yes. RN Side Generate TOML String
          const tomlString = generateActrToml({
            ...config,
            publisher,
            name,
          });
          const lockTomlString = generateActrLockToml({ publisher, name });

          // 2. Generate Profile
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

  // Manage connection reference counts and resource recovery
  useEffect(() => {
    if (!connection) return;

    connection.acquire();
    return () => {
      connection.release();
    };
  }, [connection]);

  useEffect(() => {
    if (!connection) return;

    // If there is no echo, the statement is simply a case of obtaining, not following the subscription logic
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

  // Send Message
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
   * Uploading attachments. Get them. ids
   */
  const uploadAttachment = async (
    attachments: IAttachmentItem[],
    sessionId: string,
  ): Promise<string[]> => {
    const ids = await Promise.all(
      attachments.map(async (item) =>
        connection!.uploadAttachment({
          // prepare/result The protocol is in place. attachmentId As the only key, re-use input attachments here as the only key ID And with the host.
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
   * Upload audio start, call. native
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
   * Upload the audio. id
   */
  const sendVoiceChunk = async (chunk: Uint8Array) =>
    await connection!.sendVoiceChunk(chunk);

  /**
   * Uploading audio is over, call. nativeGot it. id
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

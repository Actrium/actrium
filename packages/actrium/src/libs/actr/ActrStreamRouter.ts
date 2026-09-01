import { Buffer } from 'buffer';
import { ActrEvents, ActrStreamMessageEvent } from '../../native-bridge/actr/ActrEvents';
import { ActrNativeBridge } from '../../native-bridge/actr/ActrNativeBridge';
import { Utils } from '../../utils/utils';
import {
  AttachmentUploadStatus,
  TActrMessageResponse,
  TAttachmentUploadResult,
} from './ActrConnection.type';
import { ActrConnection } from './ActrConnection';
import { ACTR_BRIDGE_CONTRACT } from '../../runtime-host/coreContract';

export type TActrStreamMap = {
  requestStreamId: string;
  voiceResponseStreamId: string;
  textResponseStreamId: string;
  messageId: string;
};

type TStreamRegistration = {
  type:
    | typeof ACTR_BRIDGE_CONTRACT.streamTypes.text
    | typeof ACTR_BRIDGE_CONTRACT.streamTypes.voice;
  streamMap: TActrStreamMap;
  actorType: string;
  tabId: string;
  connectionId: string;
  chatListeners: Set<(data: TActrMessageResponse) => void>;
  voiceListeners: Set<(data: TActrMessageResponse) => void>;
};

type TAttachmentResultWaiter = {
  actorType: string;
  tabId: string;
  connectionId: string;
  resolve: (result: TAttachmentUploadResult) => void;
  reject: (error: Error) => void;
  timeout: ReturnType<typeof setTimeout>;
};

// Attachments have not been streamed eos Shut up,RN Side-side prodigious timeout. Avoid. native/When service is abnormal promise Long-term suspension.
const ATTACHMENT_RESULT_TIMEOUT_MS =
  ACTR_BRIDGE_CONTRACT.attachment.resultTimeoutMs;

/**
 * Global Message Router (single case)
 * Holds a single original event listening and presses streamId Exact distribution of messages.
 */
export class ActrStreamRouter {
  private subscription: any = null;

  private streamRegistry = new Map<string, TStreamRegistration>();
  private attachmentResultWaiters = new Map<string, TAttachmentResultWaiter>();

  private constructor() {
    this.subscription = ActrEvents.onStreamMessage((data) =>
      this.handleMessage(data),
    );
  }

  static getInstance(): ActrStreamRouter {
    if (!(global as any).__actrStreamRouter) {
      (global as any).__actrStreamRouter = new ActrStreamRouter();
    }
    return (global as any).__actrStreamRouter as ActrStreamRouter;
  }

  bindStream(messageId: string, connection: ActrConnection): TActrStreamMap {
    const streamMap: TActrStreamMap = {
      requestStreamId: Utils.generateId(),
      textResponseStreamId: Utils.generateId(),
      voiceResponseStreamId: Utils.generateId(),
      messageId,
    };

    const shared = {
      streamMap,
      actorType: connection.getActorType(),
      tabId: connection.getTabId(),
      connectionId: connection.getAppId(),
      chatListeners: connection.getChatListeners(),
      voiceListeners: connection.getVoiceListeners(),
    };

    this.streamRegistry.set(streamMap.textResponseStreamId, {
      ...shared,
      type: ACTR_BRIDGE_CONTRACT.streamTypes.text,
    });
    this.streamRegistry.set(streamMap.voiceResponseStreamId, {
      ...shared,
      type: ACTR_BRIDGE_CONTRACT.streamTypes.voice,
    });

    console.log(
      `[ActrStreamRouter] Registered streams for connection:${connection.getAppId()}, ` +
        `text:${streamMap.textResponseStreamId}, voice:${streamMap.voiceResponseStreamId}`,
    );

    return streamMap;
  }

  unregisterStream(responseStreamId: string) {
    this.streamRegistry.delete(responseStreamId);
  }

  // Uploading final results of registration for individual attachments, waiter. Copy attachmentResult Send text after final/Voice request.
  waitForAttachmentResult(
    streamId: string,
    connection: ActrConnection,
  ): Promise<TAttachmentUploadResult> {
    const existedWaiter = this.attachmentResultWaiters.get(streamId);
    if (existedWaiter) {
      clearTimeout(existedWaiter.timeout);
      this.attachmentResultWaiters.delete(streamId);
    }

    return new Promise<TAttachmentUploadResult>((resolve, reject) => {
      // Every one. resultStreamId Only the current attachment is uploaded, and errors are reported directly after the time elapsed, avoiding the indefinite waiting of the transmission chain.
      const timeout = setTimeout(() => {
        this.attachmentResultWaiters.delete(streamId);
        this.sendCloseStream(streamId, connection.getActorType(), connection.getTabId());
        reject(new Error('Waiting for attachment upload results to be overtime'));
      }, ATTACHMENT_RESULT_TIMEOUT_MS);

      this.attachmentResultWaiters.set(streamId, {
        actorType: connection.getActorType(),
        tabId: connection.getTabId(),
        connectionId: connection.getAppId(),
        resolve,
        reject,
        timeout,
      });
    });
  }

  // Actively unplugs and closes attachments when uploading failed native result stream, avoid leaving no-or-or-no-or-no-or-no.
  cancelAttachmentResultWait(
    streamId: string,
    connection: ActrConnection,
  ) {
    const waiter = this.attachmentResultWaiters.get(streamId);
    if (!waiter) return;

    clearTimeout(waiter.timeout);
    this.attachmentResultWaiters.delete(streamId);
    this.sendCloseStream(streamId, connection.getActorType(), connection.getTabId());
  }

  // Cannot initialise Evolution's mail component. - Usually it's a mistake. Simulate the message back.
  emitLocalTextMessage(
    responseStreamId: string,
    payload: {
      message: string;
      status?: number;
      isEnd?: boolean;
    },
  ): boolean {
    const reg = this.streamRegistry.get(responseStreamId);
    if (!reg || reg.type !== ACTR_BRIDGE_CONTRACT.streamTypes.text) {
      console.warn(
        `[ActrStreamRouter] emitLocalTextMessage failed, stream not found: ${responseStreamId}`,
      );
      return false;
    }

    const {
      streamMap: { messageId },
    } = reg;
    const status = payload.status ?? 3;
    const isEnd = payload.isEnd ?? true;

    reg.chatListeners.forEach((listener) =>
      listener({
        message_id: messageId,
        message: payload.message,
        status,
        isEnd,
        isAbort: false
      }),
    );

    if (isEnd) {
      // There is no remote stream to close when sending failed, only local registration is cleared and residual dirty mapping is avoided.
      this.clearStreamMap(reg.streamMap);
    }

    return true;
  }

  clearConnectionStreams(connection: ActrConnection) {
    for (const [streamId, reg] of this.streamRegistry) {
      if (reg.connectionId === connection.getAppId()) {
        this.streamRegistry.delete(streamId);
      }
    }
    for (const [streamId, waiter] of this.attachmentResultWaiters) {
      // When the connection was released, the attachments that were still waiting for were interrupted and the logical transmissions from the upper layer were avoided.
      if (waiter.connectionId === connection.getAppId()) {
        clearTimeout(waiter.timeout);
        this.attachmentResultWaiters.delete(streamId);
        waiter.reject(new Error('Connection disconnected, attachment upload pending cancellation'));
      }
    }
    console.log(
      `[ActrStreamRouter] Cleared all streams for connection:${connection.getAppId()}`,
    );
  }

  private handleMessage(data: ActrStreamMessageEvent) {
    try {
      // Attachment Upload Results By Independent resultStreamId Back push, do not participate in text/Voice message. streamRegistry Distribution.
      if (data.type === ACTR_BRIDGE_CONTRACT.streamTypes.attachmentResult) {
        this.handleAttachmentResultMessage(data);
        return;
      }

      const reg = this.streamRegistry.get(data.streamID);
      if (!reg) {
        console.log(
          `[ActrStreamRouter: ???] Received message:`,
          JSON.stringify(data),
        );
        return;
      }

      console.log(
        `[ActrStreamRouter:${reg.connectionId}] Received message:`,
        data,
      );

      const { streamMap, actorType, tabId } = reg;
      const { messageId } = streamMap;

      const isEnd =
        data.metadata?.[0]?.value ===
          ACTR_BRIDGE_CONTRACT.streamMetadata.terminalValue &&
        data.metadata?.[0]?.key === ACTR_BRIDGE_CONTRACT.streamMetadata.eosKey;
      
      // FIXME Not yet. abort
      const isAbort = false;
        // data.metadata?.[0]?.value ===
        //   ACTR_BRIDGE_CONTRACT.streamMetadata.terminalValue &&
        // data.metadata?.[0]?.key === ACTR_BRIDGE_CONTRACT.streamMetadata.aosKey;

      if (isAbort) {
        console.log(`[ActrStreamRouter:${reg.connectionId}]: Abort message`, data)
      }

      switch (reg.type) {
        case ACTR_BRIDGE_CONTRACT.streamTypes.voice: {
          const chunk = Buffer.from(data.message || '', 'base64');
          reg.voiceListeners.forEach((listener) =>
            listener({
              message_id: messageId,
              message: chunk,
              status: data.status,
              sequence: data.sequence,
              isEnd,
              isAbort
            }),
          );
          break;
        }
        case ACTR_BRIDGE_CONTRACT.streamTypes.text: {
          reg.chatListeners.forEach((listener) =>
            listener({
              message_id: messageId,
              message: data.message || '',
              status: data.status,
              sequence: data.sequence,
              isEnd,
              isAbort
            }),
          );
          break;
        }
        default:
          console.error(
            `[ActrStreamRouter:${reg.connectionId}] handleIncomingMessage error:`,
            'streamId not found',
          );
          break;
      }

      // User cancellation and normal closure are end-of-life; first, the final message is distributed to the business level and then the original is unified stream And clean up the local map.
      if (isEnd || isAbort) {
        this.sendCloseStream(data.streamID, actorType, tabId);
        this.clearStreamMap(streamMap);
      }
    } catch (error) {
      console.error(`[ActrStreamRouter] handleIncomingMessage error:`, error);
    }
  }

  // attachmentResult Do Not Move Text/Voice registration form, press alone resultStreamId Wake up for attachment upload promise.
  private handleAttachmentResultMessage(data: ActrStreamMessageEvent) {
    console.log(
      `[ActrStreamRouter: attachment] Received attachmentResult:`,
      JSON.stringify(data),
    );
    const waiter = this.attachmentResultWaiters.get(data.streamID);
    if (!waiter) {
      console.log(
        `[ActrStreamRouter: attachment] Received message without waiter:`,
        JSON.stringify(data),
      );
      return;
    }

    const isTerminal =
      data.status === AttachmentUploadStatus.COMPLETED ||
      data.status === AttachmentUploadStatus.FAILED;
    if (!isTerminal) {
      return;
    }

    clearTimeout(waiter.timeout);
    this.attachmentResultWaiters.delete(data.streamID);
    this.sendCloseStream(data.streamID, waiter.actorType, waiter.tabId);
    waiter.resolve({
      id: data.id || '',
      status: data.status as AttachmentUploadStatus,
      statusCode: data.statusCode ?? 0,
      errorMessage: data.errorMessage || '',
      metadata: data.metadata,
      sequence: data.sequence,
      timestampMs: data.timestampMs,
    });
  }

  private async sendCloseStream(
    streamId: string,
    actorType: string,
    tabId: string,
  ) {
    try {
      console.log('[ActrStreamRouter] sendCloseStreamMessage', streamId);
      const res = await ActrNativeBridge.unregisterStream({
        streamId,
        actorType,
        tabId,
      });
      console.log(res, 'unregisterStream');
    } catch (error) {
      console.error(`[ActrStreamRouter] sendCloseStreamMessage error:`, error);
    }
  }

  private clearStreamMap(streamMap: TActrStreamMap) {
    this.streamRegistry.delete(streamMap.textResponseStreamId);
    this.streamRegistry.delete(streamMap.voiceResponseStreamId);
  }
}

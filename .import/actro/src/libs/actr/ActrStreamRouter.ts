import { Buffer } from 'buffer';
import { ActrEvents, ActrStreamMessageEvent } from '../../native-bridge/actr/ActrEvents';
import { ActrNativeBridge } from '../../native-bridge/actr/ActrNativeBridge';
import { Utils } from '../../utils/utils';
import { TActrMessageResponse } from './ActrConnection.type';
import { ActrConnection } from './ActrConnection';

export type TActrStreamMap = {
  requestStreamId: string;
  voiceResponseStreamId: string;
  textResponseStreamId: string;
  messageId: string;
};

type TStreamRegistration = {
  type: 'text' | 'voice';
  streamMap: TActrStreamMap;
  actorType: string;
  tabId: string;
  connectionId: string;
  chatListeners: Set<(data: TActrMessageResponse) => void>;
  voiceListeners: Set<(data: TActrMessageResponse) => void>;
};

/**
 * 全局消息路由器（单例）
 * 持有唯一的原生事件监听，并按 streamId 精准分发消息。
 */
export class ActrStreamRouter {
  private subscription: any = null;

  private streamRegistry = new Map<string, TStreamRegistration>();

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
      type: 'text',
    });
    this.streamRegistry.set(streamMap.voiceResponseStreamId, {
      ...shared,
      type: 'voice',
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

  // 本地消息返回 - 一般是有报错，模拟消息返回
  emitLocalTextMessage(
    responseStreamId: string,
    payload: {
      message: string;
      status?: number;
      isEnd?: boolean;
    },
  ): boolean {
    const reg = this.streamRegistry.get(responseStreamId);
    if (!reg || reg.type !== 'text') {
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
      }),
    );

    if (isEnd) {
      // 发送失败时没有远端流可关闭，这里只清理本地注册，避免残留脏映射。
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
    console.log(
      `[ActrStreamRouter] Cleared all streams for connection:${connection.getAppId()}`,
    );
  }

  private handleMessage(data: ActrStreamMessageEvent) {
    try {
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
        data.metadata?.[0]?.value === 'true' &&
        data.metadata?.[0]?.key === 'eos';

      if (isEnd) {
        this.sendCloseStream(data.streamID, actorType, tabId);
        this.clearStreamMap(streamMap);
      }

      switch (reg.type) {
        case 'voice': {
          const chunk = Buffer.from(data.message, 'base64');
          reg.voiceListeners.forEach((listener) =>
            listener({
              message_id: messageId,
              message: chunk,
              status: data.status,
              sequence: data.sequence,
              isEnd,
            }),
          );
          break;
        }
        case 'text': {
          reg.chatListeners.forEach((listener) =>
            listener({
              message_id: messageId,
              message: data.message,
              status: data.status,
              sequence: data.sequence,
              isEnd,
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
    } catch (error) {
      console.error(`[ActrStreamRouter] handleIncomingMessage error:`, error);
    }
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

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

type TAttachmentResultWaiter = {
  actorType: string;
  tabId: string;
  connectionId: string;
  resolve: (result: TAttachmentUploadResult) => void;
  reject: (error: Error) => void;
  timeout: ReturnType<typeof setTimeout>;
};

// 中文注释：附件结果流没有 eos 收口，RN 侧主动加超时，避免 native/服务端异常时 promise 长时间悬空。
const ATTACHMENT_RESULT_TIMEOUT_MS = 30000;

/**
 * 全局消息路由器（单例）
 * 持有唯一的原生事件监听，并按 streamId 精准分发消息。
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

  // 中文注释：为单个附件上传注册最终结果等待器，收到 attachmentResult 终态后再继续发送文本/语音请求。
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
      // 中文注释：每个 resultStreamId 只服务当前这次附件上传，超时后直接报错，避免发送链路无限等待。
      const timeout = setTimeout(() => {
        this.attachmentResultWaiters.delete(streamId);
        this.sendCloseStream(streamId, connection.getActorType(), connection.getTabId());
        reject(new Error('等待附件上传结果超时'));
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

  // 中文注释：附件上传中途失败时，主动撤销等待器并关闭 native result stream，避免遗留无主回调。
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
        isAbort: false
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
    for (const [streamId, waiter] of this.attachmentResultWaiters) {
      // 中文注释：连接被释放时，中断仍在等待的附件上传结果，避免上层发送逻辑一直挂起。
      if (waiter.connectionId === connection.getAppId()) {
        clearTimeout(waiter.timeout);
        this.attachmentResultWaiters.delete(streamId);
        waiter.reject(new Error('连接已断开，附件上传结果等待被取消'));
      }
    }
    console.log(
      `[ActrStreamRouter] Cleared all streams for connection:${connection.getAppId()}`,
    );
  }

  private handleMessage(data: ActrStreamMessageEvent) {
    try {
      // 中文注释：附件上传结果由独立 resultStreamId 回推，不参与文本/语音消息的 streamRegistry 分发。
      if (data.type === 'attachmentResult') {
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
        data.metadata?.[0]?.value === 'true' &&
        data.metadata?.[0]?.key === 'eos';
      
      // FIXME 先不处理 abort
      const isAbort = false;
        // data.metadata?.[0]?.value === 'true' &&
        // data.metadata?.[0]?.key === 'aos';

      if (isAbort) {
        console.log(`[ActrStreamRouter:${reg.connectionId}]: Abort message`, data)
      }

      switch (reg.type) {
        case 'voice': {
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
        case 'text': {
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

      // 中文注释：用户取消和正常结束都属于流终态；先把终态消息分发给业务层，再统一关闭原生 stream 并清理本地映射。
      if (isEnd || isAbort) {
        this.sendCloseStream(data.streamID, actorType, tabId);
        this.clearStreamMap(streamMap);
      }
    } catch (error) {
      console.error(`[ActrStreamRouter] handleIncomingMessage error:`, error);
    }
  }

  // 中文注释：attachmentResult 不走文本/语音注册表，单独按 resultStreamId 唤醒对应附件上传 promise。
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

import { NativeModules, NativeEventEmitter } from 'react-native';
import {
  TActrMessageRequest,
  TActrMessageResponse,
  TActrVoiceRequest,
} from './ActrConnection.type';
import { Utils } from '../../utils/utils';
import { Buffer } from 'buffer';

const { ActrBridge } = NativeModules;

type TActrStreamMap = {
  requestStreamId: string;
  voiceResponseStreamId: string;
  textResponseStreamId: string;
  messageId: string;
};

/**
 * 路由注册项
 * 每个 responseStreamId（text 或 voice）对应一条注册记录
 */
type TStreamRegistration = {
  type: 'text' | 'voice';
  streamMap: TActrStreamMap;
  actorType: string;
  tabId: string;
  connectionId: string; // appId，用于日志
  chatListeners: Set<(data: TActrMessageResponse) => void>;
  voiceListeners: Set<(data: TActrMessageResponse) => void>;
};

/**
 * 全局消息路由器（单例）
 * 持有唯一的 NativeEventEmitter 监听 ActrStreamMessage
 * 管理所有 connection 的 streamMessageMap，按 streamId 精准路由消息
 */
export class ActrStreamRouter {
  private eventEmitter: NativeEventEmitter;
  private subscription: any = null;

  // responseStreamId (text 或 voice) -> 注册项，O(1) 查找
  private streamRegistry = new Map<string, TStreamRegistration>();

  private constructor() {
    this.eventEmitter = new NativeEventEmitter(ActrBridge);
    this.subscription = this.eventEmitter.addListener(
      'ActrStreamMessage',
      (data) => this.handleMessage(data),
    );
  }

  /**
   * 使用 global 存储单例，确保热更新时不会重建实例导致旧 listener 丢失
   */
  static getInstance(): ActrStreamRouter {
    if (!(global as any).__actrStreamRouter) {
      (global as any).__actrStreamRouter = new ActrStreamRouter();
    }
    return (global as any).__actrStreamRouter as ActrStreamRouter;
  }

  /**
   * 需要在客户端
   * 创建好 streamId，同时绑定 messageId，并注册到路由表
   */
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

    // 注册 text 和 voice 的 responseStreamId
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

  /**
   * 注销指定 responseStreamId
   */
  unregisterStream(responseStreamId: string) {
    this.streamRegistry.delete(responseStreamId);
  }

  /**
   * 注销某个 connection 的所有 stream
   */
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

  /**
   * 唯一的事件回调入口
   */
  private handleMessage(data: {
    streamID: string;
    message: string;
    status: number;
    metadata?: { value: string; key: string }[];
  }) {
    try {
      const reg = this.streamRegistry.get(data.streamID);
      if (!reg) {
        // 找不到注册项属于正常情况，说明该 streamID 不属于当前已注册的任何 connection
        return;
      }

      console.log(
        `[ActrStreamRouter:${reg.connectionId}] Received message:`,
        data,
      );

      const { streamMap, actorType, tabId } = reg;
      const { messageId } = streamMap;

      // 消息已发送完成
      const isEnd =
        data.metadata?.[0]?.value === 'true' &&
        data.metadata?.[0]?.key === 'eos';

      if (isEnd) {
        this.sendCloseStream(data.streamID, actorType, tabId);
        // 注销已完成的 stream
        this.unregisterStream(data.streamID);
      }

      switch (reg.type) {
        case 'voice':
          {
            const chunk = Buffer.from(data.message, 'base64');
            reg.voiceListeners.forEach((listener) =>
              listener({
                message_id: messageId,
                message: chunk,
                status: data.status,
                isEnd,
              }),
            );
          }
          break;
        case 'text':
          {
            // const chunk = data.message
            //   ? Buffer.from(data.message, 'base64').toString('utf-8')
            //   : '';
            reg.chatListeners.forEach((listener) =>
              listener({
                message_id: messageId,
                message: data.message,
                status: data.status,
                isEnd,
              }),
            );
          }
          break;
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

  /**
   * 发送关闭通道消息
   */
  private async sendCloseStream(
    streamId: string,
    actorType: string,
    tabId: string,
  ) {
    try {
      console.log('[ActrStreamRouter] sendCloseStreamMessage', streamId);
      const res = await ActrBridge.unregisterStream({
        streamId,
        actorType,
        tabId,
      });
      console.log(res, 'unregisterStream');
    } catch (error) {
      console.error(`[ActrStreamRouter] sendCloseStreamMessage error:`, error);
    }
  }
}

/**
 * Actr 连接管理类
 * 每个实例对应一个特定 App 的连接
 */
export class ActrConnection {
  private appId: string = '';
  private refCount: number = 0;
  private isInitialized: boolean = false;
  private disconnectTimer: any = null;
  private actorType: string = ''; // actor id
  private tabId: string = '';

  session_id: string = '';
  // FIXEM 临时保存初始化失败原因
  reason: string = '';

  // 当前用于上传语音的 streamId
  private currentVoiceRequestStreamId: string = '';
  // 记录当前 voice chunk 序号
  private currentVoiceChunkCount: number = 0;

  private chatListeners = new Set<(data: TActrMessageResponse) => void>();
  private voiceListeners = new Set<(data: TActrMessageResponse) => void>();

  constructor({
    appId,
    actorType,
    tabId,
  }: {
    appId: string;
    actorType: string;
    tabId: string;
  }) {
    this.appId = appId;
    this.session_id = Utils.generateId();
    this.actorType = actorType;
    this.tabId = tabId;
  }

  /**
   * 供 ActrStreamRouter 使用的访问器
   */
  getActorType(): string {
    return this.actorType;
  }

  getAppId(): string {
    return this.appId;
  }

  getTabId(): string {
    return this.tabId;
  }

  getChatListeners(): Set<(data: TActrMessageResponse) => void> {
    return this.chatListeners;
  }

  getVoiceListeners(): Set<(data: TActrMessageResponse) => void> {
    return this.voiceListeners;
  }

  /**
   * 增加引用计数
   */
  acquire() {
    this.refCount++;
    console.log(
      `[ActrConnection:${this.appId}] Acquired, count: ${this.refCount}`,
    );

    // 如果当前有等待中的断连任务，直接取消
    if (this.disconnectTimer) {
      console.log(
        `[ActrConnection:${this.appId}] Pending disconnect cancelled.`,
      );
      clearTimeout(this.disconnectTimer);
      this.disconnectTimer = null;
    }
  }

  /**
   * 减少引用计数，当计数归零时延迟断连（防止热更新导致连接闪断）
   */
  async release() {
    this.refCount--;
    console.log(
      `[ActrConnection:${this.appId}] Released, count: ${this.refCount}`,
    );

    if (this.refCount <= 0) {
      this.refCount = 0;

      // 如果已经有定时器在跑了，先清理
      if (this.disconnectTimer) {
        clearTimeout(this.disconnectTimer);
      }

      // 设置 2 秒延迟断连
      console.log(
        `[ActrConnection:${this.appId}] Last instance released, scheduling disconnect in 2s...`,
      );
      this.disconnectTimer = setTimeout(async () => {
        this.disconnectTimer = null;
        if (this.refCount <= 0) {
          await this.disconnect();
        }
      }, 2000);
    }
  }

  /**
   * 订阅聊天消息
   */
  subscribeChatMessage(callback: (data: TActrMessageResponse) => void) {
    this.chatListeners.add(callback);
    return () => this.unsubscribeChatMessage(callback);
  }

  unsubscribeChatMessage(callback: (data: TActrMessageResponse) => void) {
    this.chatListeners.delete(callback);
  }

  /**
   * 订阅语音消息
   */
  subscribeVoiceMessage(callback: (data: TActrMessageResponse) => void) {
    this.voiceListeners.add(callback);
    return () => this.unsubscribeVoiceMessage(callback);
  }

  unsubscribeVoiceMessage(callback: (data: TActrMessageResponse) => void) {
    this.voiceListeners.delete(callback);
  }

  /**
   * 初始化 Actr 连接 (通过配置文件路径)
   */
  async initialize(configPath: string): Promise<void> {
    try {
      console.log(`[ActrConnection:${this.appId}] Initializing...`);

      // await this.disconnect();
      await ActrBridge.initializeService({
        path: configPath,
        actorType: this.actorType,
        tabId: this.tabId,
      });
      this.isInitialized = true;
      this.reason = '';
      console.log(`[ActrConnection:${this.appId}] ActrService initialized.`);
    } catch (error: any) {
      this.reason = error.message || error;

      console.error(
        // `[ActrConnection:${this.appId}] Failed to initialize ActrService:`,
        error.message || error,
      );
      this.isInitialized = false;
      throw error;
    }
  }

  /**
   *
   * 发送消息
   */
  async sendMessage(content: TActrMessageRequest): Promise<void> {
    if (!this.isInitialized) {
      throw new Error(`[ActrConnection:${this.appId}] Not initialized.`);
    }

    const router = ActrStreamRouter.getInstance();
    let streamIdMap: TActrStreamMap | null = null;

    try {
      console.log(`[ActrConnection:${this.appId}] Start send message`);

      streamIdMap = router.bindStream(content.messageId, this);

      const req = {
        messageId: content.messageId,
        sessionId: content.sessionId,
        text: content.text,
        location: content.location,
        attachmentIds: content.attachmentIds,
        streamId: streamIdMap.requestStreamId,
        textResponseStreamId: streamIdMap.textResponseStreamId,
        voiceResponseStreamId: streamIdMap.voiceResponseStreamId,
        actorType: this.actorType,
        tabId: this.tabId,
      };

      console.log(
        `[ActrConnection:${this.appId}] Send message ${JSON.stringify(req)}`,
      );

      return await ActrBridge.sendTextMessage(req);
    } catch (error: any) {
      if (streamIdMap) {
        router.unregisterStream(streamIdMap.textResponseStreamId);
        router.unregisterStream(streamIdMap.voiceResponseStreamId);
      }
      console.error(
        `[ActrConnection:${this.appId}] Send message failed:`,
        error.message || error,
      );
      throw error;
    }
  }

  /**
   * 开始上传音频
   * @return stream_id
   */
  async startVoice(content: TActrVoiceRequest): Promise<string> {
    if (!this.isInitialized) {
      throw new Error(`[ActrConnection:${this.appId}] Not initialized.`);
    }

    try {
      const router = ActrStreamRouter.getInstance();
      const streamIdMap = router.bindStream(content.messageId, this);
      this.currentVoiceRequestStreamId = streamIdMap.requestStreamId;

      const req = {
        messageId: content.messageId,
        sessionId: content.sessionId,
        location: content.location,
        attachmentIds: content.attachmentIds,
        streamId: streamIdMap.requestStreamId,
        textResponseStreamId: streamIdMap.textResponseStreamId,
        voiceResponseStreamId: streamIdMap.voiceResponseStreamId,
        actorType: this.actorType,
        tabId: this.tabId,
      };

      console.log(
        `[ActrConnection:${this.appId}] Start voice ${JSON.stringify(req)}`,
      );
      const res = await ActrBridge.startVoice(req);
      console.log(res, 'start');

      return res.streamId;
    } catch (e: any) {
      // 清理记录的 voice 上传的 streamId 和 chunk 序号
      this.currentVoiceRequestStreamId = '';
      this.currentVoiceChunkCount = 0;

      console.error(
        `[ActrConnection:${this.appId}] Start voice failed:`,
        e.message,
      );
      throw e;
    }
  }

  /**
   * 上传音频
   * @return stream_id
   */
  async sendVoiceChunk(data: Uint8Array): Promise<string> {
    if (!this.isInitialized) {
      throw new Error(`[ActrConnection:${this.appId}] Not initialized.`);
    }

    try {
      this.currentVoiceChunkCount++;

      console.log('发送音频切片: ', this.currentVoiceChunkCount);

      const base64Data = Buffer.from(data).toString('base64');
      const res = await ActrBridge.sendVoiceChunk(
        {
          streamId: this.currentVoiceRequestStreamId,
          chunkId: this.currentVoiceChunkCount.toString(),
          actorType: this.actorType,
          tabId: this.tabId,
        },
        base64Data,
      );
      console.log('收到音频切片上传响应');

      return res;
    } catch (e: any) {
      console.error(
        `[ActrConnection:${this.appId}] Send voice chunk failed:`,
        e.message,
      );
      throw e;
    }
  }

  /**
   * 结束上传音频
   * @return stream_id
   */
  async endVoice(): Promise<string> {
    if (!this.isInitialized) {
      throw new Error(`[ActrConnection:${this.appId}] Not initialized.`);
    }

    try {
      console.log(
        `[ActrConnection:${this.appId}] End voice`,
        this.currentVoiceRequestStreamId,
      );
      const res = await ActrBridge.sendVoiceChunk(
        {
          streamId: this.currentVoiceRequestStreamId,
          chunkId: (this.currentVoiceChunkCount + 1).toString(),
          actorType: this.actorType,
          tabId: this.tabId,
          metadata: [
            {
              value: 'true',
              key: 'eos',
            },
          ],
        },
        '',
      );
      console.log(res, 'end');

      return res;
    } catch (e: any) {
      console.error(
        `[ActrConnection:${this.appId}] End voice failed:`,
        e.message,
      );
      throw e;
    } finally {
      // 清理记录的 voice 上传的 streamId 和 chunk 序号
      this.currentVoiceRequestStreamId = '';
      this.currentVoiceChunkCount = 0;
    }
  }

  /**
   * 上传附件
   */
  async uploadAttachment(
    streamId: string,
    data: {
      fieldData: Uint8Array;
    },
  ): Promise<string> {
    if (!this.isInitialized) {
      throw new Error(`[ActrConnection:${this.appId}] Not initialized.`);
    }
    console.log('发送附件', data.fieldData.length);
    const res = await ActrBridge.uploadAttachment({
      streamId,
      type: 2,
      fileData: Utils.uint8ArrayToBase64(data.fieldData),
      actorType: this.actorType,
      tabId: this.tabId,
    });

    if (res.status_code !== 0) {
      console.error(
        `[ActrConnection:${this.appId}] Upload attachment failed: ${res.status_code}, ${JSON.stringify(res)}`,
      );
    }

    return res.id;
  }

  /**
   * 上传附件 切片上传
   */
  async uploadAttachmentSlice(
    streamId: string,
    data: {
      fieldData: Uint8Array;
    },
  ): Promise<string> {
    if (!this.isInitialized) {
      throw new Error(`[ActrConnection:${this.appId}] Not initialized.`);
    }
    console.log('发送附件', data.fieldData.length);
    // TODO: 切片上传

    return '';
  }

  /**
   * 断开连接，清理原生侧资源
   */
  async disconnect() {
    // 【核心修复】如果当前又有人在用了，直接退出
    if (this.refCount > 0) {
      console.log(
        `[ActrConnection:${this.appId}] Disconnect aborted: new instance found.`,
      );
      return;
    }

    console.log(`[ActrConnection:${this.appId}] Disconnecting...`);
    try {
      // 1. 停掉原生服务
      await ActrBridge.shutdownService({
        actorType: this.actorType,
        tabId: this.tabId,
      });

      // 双重检查，防止异步期间又有人申请
      if (this.refCount > 0) {
        console.log(
          `[ActrConnection:${this.appId}] Disconnect rollbacked because a new instance acquired during shutdown.`,
        );
        return;
      }

      // 2. 只有确认真的没人用了，才真正清理
      // 从全局路由器中注销本 connection 的所有 stream
      ActrStreamRouter.getInstance().clearConnectionStreams(this);

      this.isInitialized = false;
      this.chatListeners.clear();
      this.voiceListeners.clear();
      console.log(`[ActrConnection:${this.appId}] Disconnected.`);
    } catch (error: any) {
      console.error(
        `[ActrConnection:${this.appId}] Cleanup failed:`,
        error.message,
      );
    }
  }

  isConnected(): boolean {
    return this.isInitialized;
  }
}

/**
 * 连接管理器
 * 实现在多 Tab 间按 tabId 缓存和分发连接实例
 */
export class ConnectionManager {
  /**
   * 使用 global 存储连接 Map，确保热更新时不会重建 Map 导致旧实例被孤立
   * 旧实例的 disconnect 定时器会杀掉原生服务，而新实例无法取消旧定时器
   * tabId -> ActrConnection
   */
  private static get connections(): Map<string, ActrConnection> {
    if (!(global as any).__actrConnections) {
      (global as any).__actrConnections = new Map<string, ActrConnection>();
    }
    return (global as any).__actrConnections;
  }

  /**
   * 获取或创建指定 App 的连接实例
   */
  static getOrCreate({
    appId,
    actorType,
    tabId,
  }: {
    appId: string;
    actorType: string;
    tabId: string;
  }): ActrConnection {
    if (!appId || !tabId) throw new Error('appId and tabId are required');

    if (!this.connections.has(tabId)) {
      this.connections.set(
        tabId,
        new ActrConnection({ appId, actorType, tabId }),
      );
    }
    return this.connections.get(tabId)!;
  }

  /**
   * 获取现有实例（不创建）
   */
  static get(tabId: string): ConnectionManager | undefined {
    return this.connections.get(tabId) as any;
  }

  /**
   * 移除并断开指定实例
   */
  static async remove(tabId: string) {
    const conn = this.connections.get(tabId);
    if (conn) {
      await conn.disconnect();
      this.connections.delete(tabId);
    }
  }
}

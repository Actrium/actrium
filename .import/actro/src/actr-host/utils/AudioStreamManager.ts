/**
 * 实时语音流式传输工具
 *
 * 功能：
 * 1. 实时录音
 * 2. 自动切片（每 N 毫秒一个切片）
 * 3. 流式发送到服务端
 */

import LiveAudioStream from 'react-native-live-audio-stream';
import { Buffer } from 'buffer';

export interface AudioStreamOptions {
  sampleRate: number;        // 采样率，如 16000
  channels: number;          // 声道数，1=单声道，2=立体声
  bitsPerSample: number;     // 位深度，8 或 16
  audioSource?: number;      // Android 音频源，6=VOICE_RECOGNITION
  bufferSize?: number;       // 缓冲区大小（字节）
  wavFile?: string;          // wav 文件路径（可选）
  onAudioChunk: (chunk: Uint8Array) => void;  // 音频切片回调（base64）
  onError?: (error: Error) => void;
}

export class AudioStreamManager {
  private isRecording = false;
  private options: AudioStreamOptions;
  private hasAttachedDataListener = false;

  constructor(options: AudioStreamOptions) {
    this.options = options;
  }



  /**
   * 开始录音和流式传输
   */
  async start(): Promise<void> {
    if (this.isRecording) {
      console.warn('[AudioStream] 已在录音中');
      return;
    }

    try {
      // 配置音频参数
      LiveAudioStream.init({
        sampleRate: this.options.sampleRate,
        channels: this.options.channels,
        bitsPerSample: this.options.bitsPerSample,
        audioSource: this.options.audioSource,
        bufferSize: this.options.bufferSize,
        wavFile: this.options.wavFile || 'audio.wav',  // 提供默认值
      });

      if (!this.hasAttachedDataListener) {
        this.hasAttachedDataListener = true;
        // 底层库没有暴露 removeListener，多次 start 时这里必须只注册一次 data 监听，避免重复喂音频。
        LiveAudioStream.on('data', (data: string) => {
          // data 是 base64 编码的音频数据
          console.log(`[AudioStream] 收到音频切片: ${data.length} bytes`);
          const binaryData = new Uint8Array(Buffer.from(data, 'base64'));
          this.options.onAudioChunk(binaryData);
          // TODO 暂时用 base64
          // this.options.onAudioChunk(data as any);
        });
      }

      // 开始录音
      LiveAudioStream.start();
      this.isRecording = true;
      console.log('[AudioStream] 开始录音');
    } catch (error) {
      console.error('[AudioStream] 启动失败:', error);
      this.options.onError?.(error instanceof Error ? error : new Error(String(error)));
      throw error;
    }
  }

  /**
   * 停止录音
   */
  stop(): void {
    if (!this.isRecording) {
      console.warn('[AudioStream] 未在录音中');
      return;
    }

    try {
      LiveAudioStream.stop();
      this.isRecording = false;
      console.log('[AudioStream] 停止录音');
    } catch (error) {
      console.error('[AudioStream] 停止失败:', error);
      this.options.onError?.(error instanceof Error ? error : new Error(String(error)));
    }
  }

  /**
   * 获取录音状态
   */
  getIsRecording(): boolean {
    return this.isRecording;
  }
}

/**
 * 使用示例
 */
export function createAudioStream(
  onAudioChunk: (chunk: Uint8Array) => void,
  onError?: (error: Error) => void
): AudioStreamManager {
  return new AudioStreamManager({
    sampleRate: 16000,        // 16kHz，适合语音识别
    channels: 1,              // 单声道
    bitsPerSample: 16,        // 16位
    audioSource: 6,           // VOICE_RECOGNITION
    bufferSize: 1024 * 4,         // 4KB 缓冲区，约 128ms 一个切片
    wavFile: 'audio.wav',     // wav 文件名
    onAudioChunk,
    onError,
  });
}

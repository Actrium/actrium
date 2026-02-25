/**
 * Type definitions for react-native-live-audio-stream
 */

declare module 'react-native-live-audio-stream' {
  export interface AudioStreamConfig {
    sampleRate: number;
    channels: number;
    bitsPerSample: number;
    audioSource?: number;
    bufferSize?: number;
    wavFile?: string;  // 可选的 wav 文件路径
  }

  export interface LiveAudioStreamStatic {
    init(config: AudioStreamConfig): void;
    start(): void;
    stop(): void;
    on(event: 'data', callback: (data: string) => void): void;
  }

  const LiveAudioStream: LiveAudioStreamStatic;
  export default LiveAudioStream;
}

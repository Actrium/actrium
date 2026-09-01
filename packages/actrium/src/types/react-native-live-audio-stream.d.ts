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
    wavFile?: string;  // Optional wav File Path
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

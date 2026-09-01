/**
 * Real-time voice streaming
 *
 * Function:
 * 1. Real-time recording
 * 2. AutoSlice (per cent) N (Mmseconds of a slice)
 * 3. Stream to service
 */

import LiveAudioStream from 'react-native-live-audio-stream';
import { Buffer } from 'buffer';

export interface AudioStreamOptions {
  sampleRate: number;        // Sample rate, e.g. 16000
  channels: number;          // The number of channels,1=Uno,2=Stereo
  bitsPerSample: number;     // Bit depth,8 or 16
  audioSource?: number;      // Android Audio sources,6=VOICE_RECOGNITION
  bufferSize?: number;       // Buffer size (bytes)
  wavFile?: string;          // wav File Path (optional)
  onAudioChunk: (chunk: Uint8Array) => void;  // Audio Slice Retort (Audience)base64)
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
   * Start recording and current transmission.
   */
  async start(): Promise<void> {
    if (this.isRecording) {
      console.warn('[AudioStream] Recording is already active.');
      return;
    }

    try {
      // Configure Audio Parameters
      LiveAudioStream.init({
        sampleRate: this.options.sampleRate,
        channels: this.options.channels,
        bitsPerSample: this.options.bitsPerSample,
        audioSource: this.options.audioSource,
        bufferSize: this.options.bufferSize,
        wavFile: this.options.wavFile || 'audio.wav',  // Provide Default
      });

      if (!this.hasAttachedDataListener) {
        this.hasAttachedDataListener = true;
        // The bottom vault is not exposed. removeListener- A lot. start It's only once. data Listen, and avoid repeating the feeding of audio.
        LiveAudioStream.on('data', (data: string) => {
          // data Yes. base64 Encoded Audio Data
          console.log(`[AudioStream] Audio slices received.: ${data.length} bytes`);
          const binaryData = new Uint8Array(Buffer.from(data, 'base64'));
          this.options.onAudioChunk(binaryData);
          // TODO For now. base64
          // this.options.onAudioChunk(data as any);
        });
      }

      // Start recording.
      LiveAudioStream.start();
      this.isRecording = true;
      console.log('[AudioStream] Start recording.');
    } catch (error) {
      console.error('[AudioStream] Start Failed:', error);
      this.options.onError?.(error instanceof Error ? error : new Error(String(error)));
      throw error;
    }
  }

  /**
   * Stop recording
   */
  stop(): void {
    if (!this.isRecording) {
      console.warn('[AudioStream] Not recorded');
      return;
    }

    try {
      LiveAudioStream.stop();
      this.isRecording = false;
      console.log('[AudioStream] Stop recording');
    } catch (error) {
      console.error('[AudioStream] Stop Failure:', error);
      this.options.onError?.(error instanceof Error ? error : new Error(String(error)));
    }
  }

  /**
   * Get Recorded Status
   */
  getIsRecording(): boolean {
    return this.isRecording;
  }
}

/**
 * Use Example
 */
export function createAudioStream(
  onAudioChunk: (chunk: Uint8Array) => void,
  onError?: (error: Error) => void
): AudioStreamManager {
  return new AudioStreamManager({
    sampleRate: 16000,        // 16kHz, for voice recognition
    channels: 1,              // Mono
    bitsPerSample: 16,        // 16bits
    audioSource: 6,           // VOICE_RECOGNITION
    bufferSize: 1024 * 4,         // 4KB buffer zone, approximately 128ms A slice.
    wavFile: 'audio.wav',     // wav Filename
    onAudioChunk,
    onError,
  });
}

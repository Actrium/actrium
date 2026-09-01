import * as RNFS from 'react-native-fs';
import { createSound } from 'react-native-nitro-sound';
import type { IVoicePayload } from '../../repository/MessageRepository';
import { Utils } from '../../utils/utils';

type SoundPlayer = ReturnType<typeof createSound>;

let playerInstance: SoundPlayer | null = null;

const getPlayer = (): SoundPlayer => {
  if (!playerInstance) {
    playerInstance = createSound();
  }
  return playerInstance;
};

const VOICE_DIR_NAME = 'actr-voices';
const DEFAULT_SAMPLE_RATE = 16000;
const DEFAULT_CHANNELS = 1;
const DEFAULT_BITS_PER_SAMPLE = 16;

export interface SaveVoiceMessageParams {
  messageId: string;
  chunks: Uint8Array[];
  sampleRate?: number;
  channels?: number;
  bitsPerSample?: number;
}

const normalizeFilePath = (path: string): string => path.replace(/^file:\/\//, '');

const ensureVoiceDirectory = async (): Promise<string> => {
  const voiceDir = `${RNFS.DocumentDirectoryPath}/${VOICE_DIR_NAME}`;
  const exists = await RNFS.exists(voiceDir);
  if (!exists) {
    await RNFS.mkdir(voiceDir);
  }
  return voiceDir;
};

const mergeChunks = (chunks: Uint8Array[]): Uint8Array => {
  const totalLength = chunks.reduce((sum, chunk) => sum + chunk.length, 0);
  const merged = new Uint8Array(totalLength);
  let offset = 0;

  for (const chunk of chunks) {
    merged.set(chunk, offset);
    offset += chunk.length;
  }

  return merged;
};

const createWavBytes = (
  pcmData: Uint8Array,
  sampleRate: number,
  channels: number,
  bitsPerSample: number,
): Uint8Array => {
  const header = new ArrayBuffer(44);
  const view = new DataView(header);
  const byteRate = sampleRate * channels * (bitsPerSample / 8);
  const blockAlign = channels * (bitsPerSample / 8);
  const dataLength = pcmData.length;

  // Harmonization of standards for completeness WAV Boss, promise. iOS / Android You can play local files directly.
  view.setUint32(0, 0x52494646, false);
  view.setUint32(4, 36 + dataLength, true);
  view.setUint32(8, 0x57415645, false);
  view.setUint32(12, 0x666d7420, false);
  view.setUint32(16, 16, true);
  view.setUint16(20, 1, true);
  view.setUint16(22, channels, true);
  view.setUint32(24, sampleRate, true);
  view.setUint32(28, byteRate, true);
  view.setUint16(32, blockAlign, true);
  view.setUint16(34, bitsPerSample, true);
  view.setUint32(36, 0x64617461, false);
  view.setUint32(40, dataLength, true);

  const wavBytes = new Uint8Array(44 + dataLength);
  wavBytes.set(new Uint8Array(header), 0);
  wavBytes.set(pcmData, 44);
  return wavBytes;
};

export const buildRecordingVoicePayload = (): IVoicePayload => ({
  local_file_uri: '',
  local_file_path: '',
  duration_ms: 0,
  mime_type: 'audio/wav',
  file_size: 0,
  sample_rate: DEFAULT_SAMPLE_RATE,
  channels: DEFAULT_CHANNELS,
  bits_per_sample: DEFAULT_BITS_PER_SAMPLE,
  status: 'recording',
});

export const parseVoicePayload = (
  voicePayload?: string,
): IVoicePayload | null => {
  if (!voicePayload) return null;
  try {
    return JSON.parse(voicePayload) as IVoicePayload;
  } catch (error) {
    console.error('[VoiceMessageStorage] Failed to parse voice message:', error);
    return null;
  }
};

export const formatVoiceDuration = (durationMs: number): string => {
  const totalSeconds = Math.max(1, Math.round(durationMs / 1000));
  // The air bubbles are presented in seconds, consistent with common voice message styles, for example, 3".
  return `${totalSeconds}"`;
};

export const saveVoiceMessageWav = async (
  params: SaveVoiceMessageParams,
): Promise<IVoicePayload> => {
  const {
    messageId,
    chunks,
    sampleRate = DEFAULT_SAMPLE_RATE,
    channels = DEFAULT_CHANNELS,
    bitsPerSample = DEFAULT_BITS_PER_SAMPLE,
  } = params;

  const voiceDir = await ensureVoiceDirectory();
  const pcmData = mergeChunks(chunks);
  const wavBytes = createWavBytes(pcmData, sampleRate, channels, bitsPerSample);
  const filePath = `${voiceDir}/${messageId}.wav`;

  await RNFS.writeFile(filePath, Utils.uint8ArrayToBase64(wavBytes), 'base64');

  const bytesPerSecond = sampleRate * channels * (bitsPerSample / 8);
  const durationMs =
    bytesPerSecond > 0 ? Math.round((pcmData.length / bytesPerSecond) * 1000) : 0;

  return {
    local_file_uri: `file://${filePath}`,
    local_file_path: filePath,
    duration_ms: durationMs,
    mime_type: 'audio/wav',
    file_size: wavBytes.length,
    sample_rate: sampleRate,
    channels,
    bits_per_sample: bitsPerSample,
    status: 'ready',
  };
};

export const cleanupVoiceMessageFile = async (
  voicePayload?: string,
): Promise<void> => {
  const payload = parseVoicePayload(voicePayload);
  const filePath = payload?.local_file_path;
  if (!filePath) return;

  try {
    const exists = await RNFS.exists(filePath);
    if (exists) {
      await RNFS.unlink(filePath);
    }
  } catch (error) {
    console.error('[VoiceMessageStorage] Failed to delete voice file:', error);
  }
};

export const playVoiceMessageFile = async (voicePayload?: string): Promise<void> => {
  const payload = parseVoicePayload(voicePayload);
  const filePath = payload?.local_file_path || normalizeFilePath(payload?.local_file_uri || '');
  if (!filePath) {
    throw new Error('Missing voice file path');
  }

  const exists = await RNFS.exists(filePath);
  if (!exists) {
    throw new Error('Voice file does not exist');
  }

  const player = getPlayer();

  return new Promise<void>((resolve, reject) => {
    player.removePlaybackEndListener();
    player.addPlaybackEndListener(() => {
      player.removePlaybackEndListener();
      resolve();
    });

    player.startPlayer(filePath).catch((err) => {
      player.removePlaybackEndListener();
      reject(err instanceof Error ? err : new Error(String(err)));
    });
  });
};

export const stopVoiceMessagePlayback = (): void => {
  try {
    if (playerInstance) {
      playerInstance.stopPlayer().catch((err) => {
        console.error('[VoiceMessageStorage] Stop Play Failed:', err);
      });
    }
  } catch (error) {
    console.error('[VoiceMessageStorage] Stop Play Failed:', error);
  }
};

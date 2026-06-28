// 16-bit PCM 的满量程基准，用于把整数采样点归一化为 -1 到 1。
const PCM_INT16_FULL_SCALE = 32768;

// 把低于该分贝的环境声视为底噪，小声说话仍能保留可见变化。
const PCM_METERING_FLOOR_DB = -58;

// 把接近该分贝的语音视为视觉满幅，避免正常说话时柱子长期打满。
const PCM_METERING_PEAK_DB = -6;

// 略低于 1 的曲线指数保留轻声反馈，同时避免底噪被过度放大。
const PCM_METERING_RESPONSE_POWER = 0.78;

// PCM 归一化后低于该比例的输入视为底噪，避免静音环境下波形持续漂浮。
const PCM_METERING_NOISE_GATE = 0.16;

// 语音识别原生库的音量事件 0 以下基本不可闻，直接作为视觉静音起点。
const NATIVE_VOLUME_FLOOR = 0;

// 语音识别原生库的音量事件上限约为 10，对应视觉满幅。
const NATIVE_VOLUME_PEAK = 10;

// 原生音量归一化后低于该比例的输入视为环境噪音，保证说完话能回到底线。
const NATIVE_VOLUME_NOISE_GATE = 0.24;

function applyMeteringNoiseGate(value: number, noiseGate: number): number {
  // 门限以下直接归零，用来过滤空调、底噪等持续小音量输入。
  if (value <= noiseGate) return 0;

  return (value - noiseGate) / (1 - noiseGate);
}

/**
 * 中文注释：把 16-bit Little Endian PCM 切片转换成 0 到 1 的视觉音量值。
 */
export function calculatePcmMetering(chunk: Uint8Array): number {
  // 空切片没有可计算的采样点，直接返回静音值。
  if (!chunk || chunk.length === 0) return 0;

  let sumSquares = 0;
  const sampleCount = Math.floor(chunk.length / 2);

  for (let i = 0; i < chunk.length - 1; i += 2) {
    let sample = chunk[i] | (chunk[i + 1] << 8);
    // 高位为 1 时按二补码还原成有符号 16-bit 采样值。
    if (sample > 32767) sample -= 65536;

    const normalized = sample / PCM_INT16_FULL_SCALE;
    sumSquares += normalized * normalized;
  }

  const rms = Math.sqrt(sumSquares / (sampleCount || 1));
  const db = 20 * Math.log10(rms + 1e-6);
  const normalizedMetering =
    (db - PCM_METERING_FLOOR_DB) /
    (PCM_METERING_PEAK_DB - PCM_METERING_FLOOR_DB);
  const clampedMetering = Math.min(1, Math.max(0, normalizedMetering));
  const gatedMetering = applyMeteringNoiseGate(
    clampedMetering,
    PCM_METERING_NOISE_GATE,
  );

  return Math.pow(gatedMetering, PCM_METERING_RESPONSE_POWER);
}

/**
 * 中文注释：把语音识别原生库的 volumechange 值转换成 0 到 1 的视觉音量值。
 */
export function calculateNativeVolumeMetering(value: number): number {
  // 过滤 NativeEventEmitter 可能传入的异常值，避免波形高度出现 NaN。
  if (!Number.isFinite(value)) return 0;

  const normalizedMetering =
    (value - NATIVE_VOLUME_FLOOR) / (NATIVE_VOLUME_PEAK - NATIVE_VOLUME_FLOOR);
  const clampedMetering = Math.min(1, Math.max(0, normalizedMetering));
  const gatedMetering = applyMeteringNoiseGate(
    clampedMetering,
    NATIVE_VOLUME_NOISE_GATE,
  );

  return Math.pow(gatedMetering, PCM_METERING_RESPONSE_POWER);
}

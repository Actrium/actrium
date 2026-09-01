// 16-bit PCM , which allows for the integration of integer sample points into one -1 Present. 1.
const PCM_INT16_FULL_SCALE = 32768;

// The sound below the decibels is considered to be the bottom noise, and the whispering still retains the visible change.
const PCM_METERING_FLOOR_DB = -58;

// The voice close to the debut is considered to be full of vision and avoids long-term plume fillings when speaking normally.
const PCM_METERING_PEAK_DB = -6;

// Zero below 1 The curved index retains a soft feedback while avoiding over-expanding the bottom noise.
const PCM_METERING_RESPONSE_POWER = 0.78;

// PCM Inputs below this proportion after integration are considered to be undernocations, avoiding continued waveform drift in silent environments.
const PCM_METERING_NOISE_GATE = 0.16;

// Voice recognition of volume events in the original library 0 The following are largely unheard of and directly serve as the starting point for the visual silence.
const NATIVE_VOLUME_FLOOR = 0;

// The voice recognition base has a maximum volume event of approximately 10- It's full of vision.
const NATIVE_VOLUME_PEAK = 10;

// Inputs below this ratio after the original volume has been unified are considered to be environmental noise, ensuring that they can be returned to the end of the line after they have been delivered.
const NATIVE_VOLUME_NOISE_GATE = 0.24;

function applyMeteringNoiseGate(value: number, noiseGate: number): number {
  // The doors are below the limit and are directly zero, which is used to filter ongoing small volume input, such as air conditioning, bottom noise, etc.
  if (value <= noiseGate) return 0;

  return (value - noiseGate) / (1 - noiseGate);
}

/**
 * * Reissued for technical reasons. 16-bit Little Endian PCM Slice Converted To 0 Present. 1 . The visual volume value.
 */
export function calculatePcmMetering(chunk: Uint8Array): number {
  // Empty slices do not have a calculable sampling point and directly return the silent value.
  if (!chunk || chunk.length === 0) return 0;

  let sumSquares = 0;
  const sampleCount = Math.floor(chunk.length / 2);

  for (let i = 0; i < chunk.length - 1; i += 2) {
    let sample = chunk[i] | (chunk[i + 1] << 8);
    // High to 1 When you press a two-code to return to a symbol 16-bit Sample value.
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
 * Note: Recognize the original library volumechange Convert Value to 0 Present. 1 . The visual volume value.
 */
export function calculateNativeVolumeMetering(value: number): number {
  // Filter NativeEventEmitter Possible inflow of abnormalities to avoid wave heights NaN.
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

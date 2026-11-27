/**
 * Minimal, dependency-free base64 helpers shared by Host modules.
 *
 * `atob`/`btoa` are unavailable in the Hermes/QuickJS sandboxes, so these
 * implement strict RFC 4648 (padded) encoding and decoding directly.
 */

const BASE64_ALPHABET =
  'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/';
const BASE64_INDEX = new Map(
  Array.from(BASE64_ALPHABET, (character, index) => [character, index])
);

/**
 * Strict RFC 4648 (padded) base64, matching the contract boundary check.
 * Implemented as an O(n) scan: a character-class regex backtracks
 * quadratically on multi-megabyte payloads.
 */
function isValidBase64(text: string): boolean {
  const length = text.length;
  if (length % 4 !== 0) return false;
  const dataLength = length - (text.endsWith('==') ? 2 : text.endsWith('=') ? 1 : 0);
  for (let index = 0; index < length; index += 1) {
    const character = text[index];
    if (index < dataLength) {
      if (!BASE64_INDEX.has(character)) return false;
    } else if (character !== '=') {
      return false;
    }
  }
  // A final quad holds 0, 2 or 3 data characters ("xx=="/"xxx="); 1 is
  // not representable ("x===" is not RFC 4648).
  return dataLength % 4 !== 1;
}

/** Decoded byte length of a validated, padded base64 string. */
export function base64DecodedLength(text: string): number {
  const padding = text.endsWith('==') ? 2 : text.endsWith('=') ? 1 : 0;
  return (text.length / 4) * 3 - padding;
}

/** Decode padded base64 without relying on `atob` (unavailable in Hermes). */
export function decodeBase64(text: string): Uint8Array {
  if (!isValidBase64(text)) {
    throw new Error('base64 must be standard base64 (RFC 4648, padded)');
  }
  const bytes = new Uint8Array(base64DecodedLength(text));
  let offset = 0;
  for (let index = 0; index < text.length; index += 4) {
    const chunk = [
      BASE64_INDEX.get(text[index]) ?? 0,
      BASE64_INDEX.get(text[index + 1]) ?? 0,
      text[index + 2] === '=' ? 0 : (BASE64_INDEX.get(text[index + 2]) ?? 0),
      text[index + 3] === '=' ? 0 : (BASE64_INDEX.get(text[index + 3]) ?? 0),
    ];
    const combined =
      (chunk[0] << 18) | (chunk[1] << 12) | (chunk[2] << 6) | chunk[3];
    if (offset < bytes.length) bytes[offset++] = (combined >> 16) & 0xff;
    if (offset < bytes.length) bytes[offset++] = (combined >> 8) & 0xff;
    if (offset < bytes.length) bytes[offset++] = combined & 0xff;
  }
  return bytes;
}

/** Encode bytes as padded base64 without relying on `btoa`. */
export function encodeBase64(bytes: Uint8Array): string {
  let output = '';
  for (let index = 0; index < bytes.length; index += 3) {
    const remaining = bytes.length - index;
    const combined =
      (bytes[index] << 16) |
      (remaining > 1 ? bytes[index + 1] << 8 : 0) |
      (remaining > 2 ? bytes[index + 2] : 0);
    output += BASE64_ALPHABET[(combined >> 18) & 0x3f];
    output += BASE64_ALPHABET[(combined >> 12) & 0x3f];
    output += remaining > 1 ? BASE64_ALPHABET[(combined >> 6) & 0x3f] : '=';
    output += remaining > 2 ? BASE64_ALPHABET[combined & 0x3f] : '=';
  }
  return output;
}

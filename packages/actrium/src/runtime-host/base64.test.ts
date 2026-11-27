import { describe, expect, test } from 'bun:test';

import { base64DecodedLength, decodeBase64, encodeBase64 } from './base64';

const CHUNK_SIZE = 49152;

function bytesOf(size: number, seed = 0): Uint8Array {
  const bytes = new Uint8Array(size);
  for (let index = 0; index < size; index += 1) {
    bytes[index] = (index + seed) % 256;
  }
  return bytes;
}

describe('base64 helpers', () => {
  test('round-trips padded base64', () => {
    for (const size of [0, 1, 2, 3, 100, CHUNK_SIZE, CHUNK_SIZE + 1]) {
      const bytes = bytesOf(size);
      expect(decodeBase64(encodeBase64(bytes))).toEqual(bytes);
    }
  });

  test('computes decoded length including padding', () => {
    expect(base64DecodedLength('')).toBe(0);
    expect(base64DecodedLength('AAEC')).toBe(3);
    expect(base64DecodedLength('AQI=')).toBe(2);
    expect(base64DecodedLength('AQ==')).toBe(1);
  });

  test('rejects malformed base64', () => {
    expect(() => decodeBase64('not base64!!')).toThrow('standard base64');
    expect(() => decodeBase64('AAE')).toThrow('standard base64');
  });
});

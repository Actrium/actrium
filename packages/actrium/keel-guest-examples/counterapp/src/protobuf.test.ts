import { describe, expect, test } from 'bun:test';

import { decodeEchoResponse, encodeEchoRequest } from './protobuf';

describe('echo protobuf codec', () => {
  test('encodes the EchoRequest wire format', () => {
    expect(encodeEchoRequest('Hi')).toBe('CgJIaQ==');
    expect(encodeEchoRequest('\u4f60\u597d')).toBe('CgbkvaDlpb0=');
  });

  test('decodes reply and timestamp fields', () => {
    expect(decodeEchoResponse('CgtFY2hvOiBoZWxsbxAH')).toEqual({
      reply: 'Echo: hello',
      timestamp: 7,
    });
  });

  test('ignores unknown length-delimited fields', () => {
    expect(decodeEchoResponse('CgtFY2hvOiBoZWxsbxoBeBAH')).toEqual({
      reply: 'Echo: hello',
      timestamp: 7,
    });
  });

  test('ignores unknown fixed64 fields', () => {
    expect(decodeEchoResponse('CgVoZWxsbxkBAAAAAAAAAA==')).toEqual({
      reply: 'hello',
      timestamp: 0,
    });
  });

  test('ignores unknown fixed32 fields', () => {
    expect(decodeEchoResponse('CgVoZWxsbyUCAAAA')).toEqual({
      reply: 'hello',
      timestamp: 0,
    });
  });

  test('rejects a field length that exceeds the payload', () => {
    expect(() => decodeEchoResponse('CgVoaQ==')).toThrow();
  });

  test('rejects a truncated varint', () => {
    expect(() => decodeEchoResponse('CoA=')).toThrow();
  });

  test('accepts a zero-length reply field', () => {
    expect(decodeEchoResponse('CgA=')).toEqual({ reply: '', timestamp: 0 });
  });
});

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
});

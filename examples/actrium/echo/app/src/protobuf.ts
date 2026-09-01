import {
  createUtf8Decoder,
  createUtf8Encoder,
  decodeBase64,
  encodeBase64,
} from 'keel/guest';

function encodeVarint(value: number): number[] {
  const bytes: number[] = [];
  let remaining = Math.max(0, Math.floor(value));
  do {
    const byte = remaining % 128;
    remaining = Math.floor(remaining / 128);
    bytes.push(remaining > 0 ? byte | 0x80 : byte);
  } while (remaining > 0);
  return bytes;
}

function decodeVarint(
  bytes: Uint8Array,
  start: number,
): { value: number; next: number } {
  let value = 0;
  let multiplier = 1;
  let cursor = start;
  while (cursor < bytes.length) {
    const byte = bytes[cursor++]!;
    value += (byte & 0x7f) * multiplier;
    if ((byte & 0x80) === 0) return { value, next: cursor };
    multiplier *= 128;
    if (multiplier > Number.MAX_SAFE_INTEGER) break;
  }
  throw new Error('Invalid protobuf varint');
}

export function encodeEchoRequest(message: string): string {
  const messageBytes = createUtf8Encoder()(message);
  return encodeBase64(
    Uint8Array.from([
      0x0a,
      ...encodeVarint(messageBytes.length),
      ...messageBytes,
    ]),
  );
}

export function decodeEchoResponse(payloadBase64: string): {
  reply: string;
  timestamp: number;
} {
  const bytes = decodeBase64(payloadBase64);
  let cursor = 0;
  let reply = '';
  let timestamp = 0;

  while (cursor < bytes.length) {
    const tag = decodeVarint(bytes, cursor);
    cursor = tag.next;
    const fieldNumber = Math.floor(tag.value / 8);
    const wireType = tag.value % 8;
    if (fieldNumber === 1 && wireType === 2) {
      const length = decodeVarint(bytes, cursor);
      cursor = length.next;
      reply = createUtf8Decoder()(
        bytes.slice(cursor, cursor + length.value),
      );
      cursor += length.value;
    } else if (fieldNumber === 2 && wireType === 0) {
      const value = decodeVarint(bytes, cursor);
      timestamp = value.value;
      cursor = value.next;
    } else if (wireType === 0) {
      cursor = decodeVarint(bytes, cursor).next;
    } else if (wireType === 2) {
      const length = decodeVarint(bytes, cursor);
      cursor = length.next + length.value;
    } else {
      throw new Error(`Unsupported protobuf wire type: ${wireType}`);
    }
  }

  return { reply, timestamp };
}

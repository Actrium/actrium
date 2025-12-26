/**
 * 由脚本自动生成，请勿手改。
 *
 * 来源：specs/contracts/ask.contracts.v1.json
 * 生成：bun run scripts/generate-contracts.ts
 */

export const ASK_CONTRACT_NAME = 'ask' as const;
export const ASK_CONTRACT_VERSION = 1 as const;

export type AskContractName = typeof ASK_CONTRACT_NAME;
export type AskContractVersion = typeof ASK_CONTRACT_VERSION;

export type HostToGuestEventPayloads = {
  HOST_VISIBILITY: { tabId?: string; visible: boolean };
  RECEIVER_BACKPRESSURE: { applied: number; batchId?: number; skipped: number; total: number };
};
export type HostToGuestEventName = keyof HostToGuestEventPayloads;
export const HOST_TO_GUEST_EVENT_NAMES = ['HOST_VISIBILITY', 'RECEIVER_BACKPRESSURE'] as const;
export function isHostToGuestEventName(name: string): name is HostToGuestEventName {
  return (HOST_TO_GUEST_EVENT_NAMES as readonly string[]).includes(name);
}

export type GuestToHostEventPayloads = {
  ASKIT_HAPTIC_TRIGGER: { type?: string };
  ASKIT_TOAST_SHOW: { message: string; options?: unknown };
  GUEST_SLEEP_STATE: { reason?: string; sleeping: boolean; tabId?: string };
};
export type GuestToHostEventName = keyof GuestToHostEventPayloads;
export const GUEST_TO_HOST_EVENT_NAMES = [
  'ASKIT_HAPTIC_TRIGGER',
  'ASKIT_TOAST_SHOW',
  'GUEST_SLEEP_STATE',
] as const;
export function isGuestToHostEventName(name: string): name is GuestToHostEventName {
  return (GUEST_TO_HOST_EVENT_NAMES as readonly string[]).includes(name);
}

export type HostToGuestEvent<E extends HostToGuestEventName = HostToGuestEventName> = {
  name: E;
  payload: HostToGuestEventPayloads[E];
};

export type GuestToHostEvent<E extends GuestToHostEventName = GuestToHostEventName> = {
  name: E;
  payload: GuestToHostEventPayloads[E];
};

type PrimitiveTypeName = ['string', 'number', 'boolean', 'unknown'][number];
type FieldSchema = readonly [type: PrimitiveTypeName, optional: boolean];

function isPlainObject(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}

function validatePayloadAgainstSchema(
  payload: unknown,
  schema: Record<string, FieldSchema>
): boolean {
  if (!isPlainObject(payload)) return false;
  const obj = payload as Record<string, unknown>;
  for (const key of Object.keys(schema)) {
    const field = schema[key];
    if (!field) continue;
    const [type, optional] = field;

    if (!(key in obj)) {
      if (!optional) return false;
      continue;
    }

    const value = obj[key];
    if (value === undefined) {
      if (!optional) return false;
      continue;
    }

    if (type === 'unknown') continue;
    if (typeof value !== type) return false;
  }
  return true;
}

export const HOST_TO_GUEST_PAYLOAD_SCHEMA = {
  HOST_VISIBILITY: { tabId: ['string', true] as const, visible: ['boolean', false] as const },
  RECEIVER_BACKPRESSURE: {
    applied: ['number', false] as const,
    batchId: ['number', true] as const,
    skipped: ['number', false] as const,
    total: ['number', false] as const,
  },
} as const;
export function validateHostToGuestPayload<E extends HostToGuestEventName>(
  name: E,
  payload: unknown
): payload is HostToGuestEventPayloads[E] {
  const schema = (HOST_TO_GUEST_PAYLOAD_SCHEMA as Record<string, Record<string, FieldSchema>>)[
    name
  ];
  if (!schema) return false;
  return validatePayloadAgainstSchema(payload, schema);
}

export const GUEST_TO_HOST_PAYLOAD_SCHEMA = {
  ASKIT_HAPTIC_TRIGGER: { type: ['string', true] as const },
  ASKIT_TOAST_SHOW: { message: ['string', false] as const, options: ['unknown', true] as const },
  GUEST_SLEEP_STATE: {
    reason: ['string', true] as const,
    sleeping: ['boolean', false] as const,
    tabId: ['string', true] as const,
  },
} as const;
export function validateGuestToHostPayload<E extends GuestToHostEventName>(
  name: E,
  payload: unknown
): payload is GuestToHostEventPayloads[E] {
  const schema = (GUEST_TO_HOST_PAYLOAD_SCHEMA as Record<string, Record<string, FieldSchema>>)[
    name
  ];
  if (!schema) return false;
  return validatePayloadAgainstSchema(payload, schema);
}

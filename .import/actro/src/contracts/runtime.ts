import type {
  GuestToHostEventName,
  GuestToHostEventPayloads,
  HostToGuestEventName,
  HostToGuestEventPayloads,
} from './generated';
import {
  isGuestToHostEventName,
  isHostToGuestEventName,
  validateGuestToHostPayload,
  validateHostToGuestPayload,
} from './generated';

export type ContractDirection = 'hostToGuest' | 'guestToHost';
export type ContractViolationKind = 'unknown_event' | 'invalid_payload' | 'missing_permission';

export type ContractViolation = {
  at: number;
  direction: ContractDirection;
  kind: ContractViolationKind;
  eventName: string;
  payload?: unknown;
  reason?: string;
};

export type ContractViolationSummary = {
  total: number;
  last: ContractViolation | null;
  recent: readonly ContractViolation[];
};

export function createContractViolationCollector(options?: { max?: number }): {
  record: (violation: Omit<ContractViolation, 'at'> & { at?: number }) => void;
  summary: () => ContractViolationSummary;
} {
  const max = Math.max(1, options?.max ?? 50);
  let total = 0;
  let last: ContractViolation | null = null;
  const recent: ContractViolation[] = [];

  return {
    record(v) {
      const violation: ContractViolation = { at: v.at ?? Date.now(), ...v };
      total += 1;
      last = violation;
      recent.push(violation);
      if (recent.length > max) recent.splice(0, recent.length - max);
    },
    summary() {
      return { total, last, recent };
    },
  };
}

export function createHostToGuestSender(engine: {
  sendEvent: (eventName: string, payload?: unknown) => void;
}): <E extends HostToGuestEventName>(eventName: E, payload: HostToGuestEventPayloads[E]) => void;
export function createHostToGuestSender(
  engine: {
    sendEvent: (eventName: string, payload?: unknown) => void;
  },
  options?: {
    onViolation?: (violation: ContractViolation) => void;
    mode?: 'warn' | 'throw' | 'silent';
  }
): <E extends HostToGuestEventName>(eventName: E, payload: HostToGuestEventPayloads[E]) => void;
export function createHostToGuestSender(
  engine: {
    sendEvent: (eventName: string, payload?: unknown) => void;
  },
  options?: {
    onViolation?: (violation: ContractViolation) => void;
    mode?: 'warn' | 'throw' | 'silent';
  }
): <E extends HostToGuestEventName>(eventName: E, payload: HostToGuestEventPayloads[E]) => void {
  const mode = options?.mode ?? 'warn';

  return (eventName, payload) => {
    if (!isHostToGuestEventName(eventName)) {
      const v: ContractViolation = {
        at: Date.now(),
        direction: 'hostToGuest',
        kind: 'unknown_event',
        eventName,
        payload,
        reason: 'not declared in hostToGuest contracts',
      };
      options?.onViolation?.(v);
      if (mode === 'throw') throw new Error(`[ask/contracts] 未声明事件: ${eventName}`);
      if (mode === 'warn') console.warn('[ask/contracts] 未声明事件:', v);
      return;
    }

    if (!validateHostToGuestPayload(eventName, payload)) {
      const v: ContractViolation = {
        at: Date.now(),
        direction: 'hostToGuest',
        kind: 'invalid_payload',
        eventName,
        payload,
        reason: 'payload does not satisfy contracts schema',
      };
      options?.onViolation?.(v);
      if (mode === 'throw') throw new Error(`[ask/contracts] payload 不合法: ${eventName}`);
      if (mode === 'warn') console.warn('[ask/contracts] payload 不合法:', v);
      return;
    }

    engine.sendEvent(eventName, payload);
  };
}

export function createGuestToHostSender(
  sendToHost: (eventName: string, payload?: unknown) => void
): <E extends GuestToHostEventName>(eventName: E, payload: GuestToHostEventPayloads[E]) => void;
export function createGuestToHostSender(
  sendToHost: (eventName: string, payload?: unknown) => void,
  options?: {
    onViolation?: (violation: ContractViolation) => void;
    mode?: 'warn' | 'throw' | 'silent';
  }
): <E extends GuestToHostEventName>(eventName: E, payload: GuestToHostEventPayloads[E]) => void;
export function createGuestToHostSender(
  sendToHost: (eventName: string, payload?: unknown) => void,
  options?: {
    onViolation?: (violation: ContractViolation) => void;
    mode?: 'warn' | 'throw' | 'silent';
  }
): <E extends GuestToHostEventName>(eventName: E, payload: GuestToHostEventPayloads[E]) => void {
  const mode = options?.mode ?? 'warn';

  return (eventName, payload) => {
    if (!isGuestToHostEventName(eventName)) {
      const v: ContractViolation = {
        at: Date.now(),
        direction: 'guestToHost',
        kind: 'unknown_event',
        eventName,
        payload,
        reason: 'not declared in guestToHost contracts',
      };
      options?.onViolation?.(v);
      if (mode === 'throw') throw new Error(`[ask/contracts] 未声明事件: ${eventName}`);
      if (mode === 'warn') console.warn('[ask/contracts] 未声明事件:', v);
      return;
    }

    if (!validateGuestToHostPayload(eventName, payload)) {
      const v: ContractViolation = {
        at: Date.now(),
        direction: 'guestToHost',
        kind: 'invalid_payload',
        eventName,
        payload,
        reason: 'payload does not satisfy contracts schema',
      };
      options?.onViolation?.(v);
      if (mode === 'throw') throw new Error(`[ask/contracts] payload 不合法: ${eventName}`);
      if (mode === 'warn') console.warn('[ask/contracts] payload 不合法:', v);
      return;
    }

    sendToHost(eventName, payload);
  };
}

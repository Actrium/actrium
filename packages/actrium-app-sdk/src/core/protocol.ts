/**
 * askit <-> host protocol (aligned to keel HostEvent model)
 *
 * Guest -> Host messages are delivered via keel's `__sendEventToHost(eventName, payload)`.
 * Host -> Guest uses keel's `Engine.sendEvent(eventName, payload)`.
 *
 * We reserve `ASKIT_*` event names for askit internal/module commands.
 */

export const ASKIT_TOAST_SHOW = 'ASKIT_TOAST_SHOW' as const;
export const ASKIT_HAPTIC_TRIGGER = 'ASKIT_HAPTIC_TRIGGER' as const;

export type AskitHostEventName = typeof ASKIT_TOAST_SHOW | typeof ASKIT_HAPTIC_TRIGGER;

export type AskitToastShowPayload = {
  message: string;
  options?: unknown;
};

export type AskitHapticTriggerPayload = {
  type?: string;
};

import { useCallback, useEffect, useRef } from 'react';
import { EventEmitter } from './EventEmitter.guest';

type PendingRequest<T> = {
  resolve: (value: T) => void;
  reject: (reason?: unknown) => void;
  timer: ReturnType<typeof setTimeout>;
};

type EventBridgeOptions = {
  timeoutMs?: number;
};

type RequestPayload = {
  requestId?: string;
  [key: string]: unknown;
};

function createRequestId(eventName: string): string {
  return `${eventName}:${Date.now()}:${Math.random().toString(36).slice(2)}`;
}

export function useEventBridge<TRequest extends RequestPayload = RequestPayload, TResponse = unknown>(
  requestEvent: string,
  responseEvent: string,
  options: EventBridgeOptions = {}
) {
  const timeoutMs = options.timeoutMs ?? 15000;
  const pendingRef = useRef(new Map<string, PendingRequest<TResponse>>());

  useEffect(() => {
    return EventEmitter.on<TResponse & { requestId?: string }>(responseEvent, (payload) => {
      const requestId = payload?.requestId;
      if (!requestId) return;

      const pending = pendingRef.current.get(requestId);
      if (!pending) return;

      clearTimeout(pending.timer);
      pendingRef.current.delete(requestId);
      pending.resolve(payload as TResponse);
    });
  }, [responseEvent]);

  useEffect(() => {
    return () => {
      pendingRef.current.forEach((pending) => {
        clearTimeout(pending.timer);
        pending.reject(new Error(`[useEventBridge] Disposed: ${requestEvent} -> ${responseEvent}`));
      });
      pendingRef.current.clear();
    };
  }, [requestEvent, responseEvent]);

  const request = useCallback(
    (payload: TRequest): Promise<TResponse> => {
      const requestId = payload.requestId ?? createRequestId(requestEvent);
      const message = { ...payload, requestId };

      return new Promise<TResponse>((resolve, reject) => {
        const timer = setTimeout(() => {
          pendingRef.current.delete(requestId);
          reject(new Error(`[useEventBridge] Timeout: ${requestEvent} -> ${responseEvent} (${requestId})`));
        }, timeoutMs);

        pendingRef.current.set(requestId, { resolve, reject, timer });
        EventEmitter.emit(requestEvent, message);
      });
    },
    [requestEvent, responseEvent, timeoutMs]
  );

  return { request };
}

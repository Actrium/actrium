import { HostHttpHandler } from 'askit/host-events';

import type { ServiceCallOutput, ServiceConnection } from './contract';
import type { ActorCall, ServiceCallResult } from './host-modules';

type ActorCallRequest = Parameters<ActorCall>[0];

type NativeService = {
  call?: (tabId: string, route: string, payloadBase64: string, timeoutMs: number) =>
    Promise<ServiceCallOutput>;
  callBound?: (
    tabId: string, route: string, payloadBase64: string, timeoutMs: number,
    reloadKey: number, connection: ServiceConnection,
  ) => Promise<ServiceCallOutput>;
};

function serializeServiceError(error: unknown): string {
  if (typeof error === 'string') return error;
  try {
    const serialized = JSON.stringify(error, (_key, value: unknown) => {
      if (value instanceof Error) {
        const { nativeStackIOS: _nativeStackIOS, ...fields } = value as Error & {
          nativeStackIOS?: unknown;
        };
        return { ...fields, name: value.name, message: value.message, cause: value.cause };
      }
      return value;
    });
    return serialized ?? String(error);
  } catch {
    return error instanceof Error ? error.message : 'Service error could not be serialized.';
  }
}

export async function callHttpService(
  url: string,
  { requestId, route, payloadBase64, timeoutMs, connection }: ActorCallRequest
): Promise<ServiceCallResult> {
  if (connection) {
    return { success: false, message: 'The HTTP Service transport cannot bind a connection.' };
  }
  const result = await HostHttpHandler.handleRequest<{ success?: boolean; payloadBase64?: string }>({
    requestId,
    url,
    method: 'POST',
    body: { routeKey: route, payloadBase64, timeoutMs },
  });
  if (result.success && result.data?.success === true && typeof result.data.payloadBase64 === 'string') {
    return { success: true, payloadBase64: result.data.payloadBase64 };
  }
  if (result.rawBody) return { success: false, message: result.rawBody };
  if (result.rawBody === '' && result.status !== undefined && result.status >= 400) {
    return { success: false, message: `HTTP ${result.status}` };
  }
  const message = result.cause === undefined ? result.message : serializeServiceError(result.cause);
  return { success: false, message: message || 'Service call failed' };
}

export async function callNativeService(
  service: NativeService | undefined,
  tabId: string,
  { route, payloadBase64, timeoutMs, connection }: ActorCallRequest,
  reloadKey = 0,
): Promise<ServiceCallResult> {
  try {
    const response = connection
      ? await service?.callBound?.(tabId, route, payloadBase64, timeoutMs, reloadKey, connection)
      : await service?.call?.(tabId, route, payloadBase64, timeoutMs);
    if (!response) return { success: false, message: 'The requested native Realm Service bridge is unavailable.' };
    return { success: true, payloadBase64: response.payloadBase64,
      ...(response.connectionId ? { connectionId: response.connectionId } : {}) };
  } catch (error) {
    return { success: false, message: serializeServiceError(error) || 'Service call failed' };
  }
}

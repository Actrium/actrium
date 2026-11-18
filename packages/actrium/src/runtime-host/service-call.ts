import { HostHttpHandler } from 'askit/host-events';

import type { ActorCall, ServiceCallResult } from './host-modules';

type ActorCallRequest = Parameters<ActorCall>[0];

type NativeService = {
  call?: (tabId: string, route: string, payloadBase64: string, timeoutMs: number) =>
    Promise<{ payloadBase64: string }>;
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
  { requestId, route, payloadBase64, timeoutMs }: ActorCallRequest
): Promise<ServiceCallResult> {
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
  { route, payloadBase64, timeoutMs }: ActorCallRequest
): Promise<ServiceCallResult> {
  if (!service?.call) {
    return { success: false, message: 'The native Realm Service bridge is unavailable.' };
  }
  try {
    const response = await service.call(tabId, route, payloadBase64, timeoutMs);
    return { success: true, payloadBase64: response.payloadBase64 };
  } catch (error) {
    return { success: false, message: serializeServiceError(error) || 'Service call failed' };
  }
}

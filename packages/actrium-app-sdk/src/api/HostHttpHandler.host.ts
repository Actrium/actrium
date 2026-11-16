export type HostHttpRequest = {
  requestId?: string;
  url: string;
  method?: string;
  headers?: Record<string, string>;
  body?: unknown;
};

export type HostHttpResponse<T = unknown> = {
  requestId?: string;
  success: boolean;
  status?: number;
  headers?: Record<string, string>;
  data?: T;
  message?: string;
  rawBody?: string;
  cause?: unknown;
};

function normalizeBody(body: unknown, headers: Record<string, string>): RequestInit['body'] {
  if (body == null || typeof body === 'string') return body as RequestInit['body'];
  if (
    (typeof FormData !== 'undefined' && body instanceof FormData) ||
    (typeof URLSearchParams !== 'undefined' && body instanceof URLSearchParams) ||
    (typeof Blob !== 'undefined' && body instanceof Blob) ||
    body instanceof ArrayBuffer
  ) {
    return body as RequestInit['body'];
  }

  const hasContentType = Object.keys(headers).some(
    (key) => key.toLowerCase() === 'content-type'
  );
  if (!hasContentType) headers['Content-Type'] = 'application/json';
  return JSON.stringify(body);
}

function parseResponse(text: string, contentType: string): unknown {
  if (contentType.includes('application/json')) return JSON.parse(text);
  try {
    return JSON.parse(text);
  } catch {
    return text;
  }
}

export const HostHttpHandler = {
  async handleRequest<T = unknown>(request: HostHttpRequest): Promise<HostHttpResponse<T>> {
    try {
      const headers = { ...(request.headers ?? {}) };
      const response = await fetch(request.url, {
        method: request.method ?? 'GET',
        headers,
        body: normalizeBody(request.body, headers),
      });

      const metadata = {
        requestId: request.requestId,
        status: response.status,
        headers: Object.fromEntries(response.headers.entries()),
      };
      let rawBody: string | undefined;
      try {
        rawBody = await response.text();
        const data = parseResponse(rawBody, response.headers.get('content-type') ?? '') as T;
        return { ...metadata, success: response.ok, data, rawBody, message: response.statusText };
      } catch (error) {
        return {
          ...metadata,
          success: false,
          rawBody,
          message: error instanceof Error ? error.message : String(error),
          cause: error,
        };
      }
    } catch (error) {
      return {
        requestId: request.requestId,
        success: false,
        message: error instanceof Error ? error.message : String(error),
        cause: error,
      };
    }
  },
};

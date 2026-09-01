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

async function readResponse(response: Response): Promise<unknown> {
  const contentType = response.headers.get('content-type') ?? '';
  if (contentType.includes('application/json')) {
    return response.json();
  }

  const text = await response.text();
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

      const data = await readResponse(response);
      return {
        requestId: request.requestId,
        success: response.ok,
        status: response.status,
        headers: Object.fromEntries(response.headers.entries()),
        data: data as T,
        message: response.statusText,
      };
    } catch (error) {
      return {
        requestId: request.requestId,
        success: false,
        message: error instanceof Error ? error.message : String(error),
      };
    }
  },
};

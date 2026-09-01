// Actro/ActroRuntime/src/api/https.ts
import { HostHttpHandler } from 'askit';
import { authApiResponseSuccess } from '../runtime-host/coreContract';

/**
 * The results of the business level are finally coming.
 */
export interface IBusinessResult<T> {
  success: boolean;
  code?: number;
  message: string;
  data: T | null;
}

export const http = {
  async get<T>(url: string, headers?: any): Promise<IBusinessResult<T>> {
    const res = await HostHttpHandler.handleRequest({
      requestId: '',
      url,
      method: 'GET',
      headers,
    });

    return transformResponse<T>(res);
  },

  async post<T>(
    url: string,
    body?: any,
    headers?: any,
  ): Promise<IBusinessResult<T>> {
    const res = await HostHttpHandler.handleRequest({
      requestId: '',
      url,
      method: 'POST',
      body,
      headers,
    });

    return transformResponse<T>(res);
  },

  async delete<T>(
    url: string,
    body?: any,
    headers?: any,
  ): Promise<IBusinessResult<T>> {
    const res = await HostHttpHandler.handleRequest({
      requestId: '',
      url,
      method: 'DELETE',
      body,
      headers,
    });

    return transformResponse<T>(res);
  },
};

/**
 * Logic of unsettled UDP formats (dismantling)
 */
function transformResponse<T>(res: any): IBusinessResult<T> {
  // 1. If the network/HTTPThe floor is a failure.
  if (!res?.success) {
    return {
      success: false,
      message: JSON.stringify(res),
      data: null,
    };
  }

  const body = res.data;

  // 2. If the standard shell format is returned { code, data, message }
  if (body && typeof body === 'object' && 'code' in body && 'data' in body) {
    return {
      success: authApiResponseSuccess(body.code),
      code: body.code,
      message: body.message || '',
      data: body.data as T, // 👈 This step is to strip the data out.
    };
  }

  // 3. If it's not standard format to return (directly) T) returns directly
  return {
    success: true,
    message: '',
    data: body as T,
  };
}

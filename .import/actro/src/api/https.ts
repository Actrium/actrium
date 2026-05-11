// Canvas/Subsystem/src/api/https.ts
import { HostHttpHandler } from 'askit';

/**
 * 业务层最终拿到的结果
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
 * 统一处理格式不固定的逻辑（拆箱）
 */
function transformResponse<T>(res: any): IBusinessResult<T> {
  // 1. 如果网络/HTTP层就失败了
  if (!res?.success) {
    return {
      success: false,
      message: JSON.stringify(res),
      data: null,
    };
  }

  const body = res.data;

  // 2. 如果返回的是标准外壳格式 { code, data, message }
  if (body && typeof body === 'object' && 'code' in body && 'data' in body) {
    return {
      success: body.code === 200 || body.code === 0, // 根据你的后端约定改判断条件
      code: body.code,
      message: body.message || '',
      data: body.data as T, // 👈 这一步直接把数据剥离出来
    };
  }

  // 3. 如果返回的是非标准格式（直接就是 T），则直接返回
  return {
    success: true,
    message: '',
    data: body as T,
  };
}

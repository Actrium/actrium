import { CONSOLE_BASE } from './app';
import { http } from './https';
import { AccountUserInfo, AuthAppInfo } from './types';
import {
  AUTH_ENDPOINT_CONTRACT,
  AUTH_ENVIRONMENT_CONTRACT,
  authAccessControlPath,
  authAccountBaseUrl,
  authCheckSessionPath,
  authClientAppPath,
  type AuthEnvironment,
} from '../actr-host/coreContract';

// Environments
export const ENVS: Record<AuthEnvironment, string> = {
  [AUTH_ENVIRONMENT_CONTRACT.environments.dev]: authAccountBaseUrl(AUTH_ENVIRONMENT_CONTRACT.environments.dev),
  [AUTH_ENVIRONMENT_CONTRACT.environments.beta]: authAccountBaseUrl(AUTH_ENVIRONMENT_CONTRACT.environments.beta),
  [AUTH_ENVIRONMENT_CONTRACT.environments.prod]: authAccountBaseUrl(AUTH_ENVIRONMENT_CONTRACT.environments.prod),
};

export const CURRENT_ENV: AuthEnvironment = AUTH_ENVIRONMENT_CONTRACT.environments.prod;
export const BASE_URL = authAccountBaseUrl(CURRENT_ENV);

export const authApi = {
  /**
   * 判断用户是否已登录
   */
  checkSession: (token: string) =>
    http.get<string>(`${BASE_URL}${authCheckSessionPath(token)}`),

  /**
   * 获取用户信息
   */
  getUserInfo: (token: string) =>
    http.get<AccountUserInfo>(`${BASE_URL}${AUTH_ENDPOINT_CONTRACT.paths.userInfo}`, {
      'x-a1-authorization': `Bearer ${token}`,
    }),

  /**
   * 获取入驻应用的基本信息
   */
  // getAppInfo: (clientId: string) =>
  //   http.get<AuthAppInfo>(`${BASE_URL}/protocol/appinfo?client_id=${clientId}`),

  /**
   * 通过微信的 code 获取登录凭证
   */
  loginWithWechat: (code: string) =>
    http.post<string>(`${BASE_URL}${AUTH_ENDPOINT_CONTRACT.paths.wechatSession}`, { code }),

  /**
   * 通过苹果的 id_token 获取登录凭证
   */
  loginWithApple: (idToken: string) =>
    http.post<string>(`${BASE_URL}${AUTH_ENDPOINT_CONTRACT.paths.appleSession}`, {
      id_token: idToken,
    }),

  /**
   * 通过 Google 的 id_token 获取登录凭证
   */
  loginWithGoogle: (idToken: string) =>
    http.post<string>(`${BASE_URL}${AUTH_ENDPOINT_CONTRACT.paths.googleSession}`, {
      id_token: idToken,
    }),

  /**
   * 用户点击授权后请求临时授权码
   */
  authorize: (clientId: string, loginHint: string, token: string) =>
    http.post<{ code: string; redirect_uri: string }>(
      `${BASE_URL}${AUTH_ENDPOINT_CONTRACT.paths.authorize}`,
      {
        client_id: clientId,
        login_hint: loginHint,
      },
      {
        'X-A1-Authorization': `Bearer ${token}`,
      },
    ),

  // 判断是否已授权
  getIsAuthorized: (params: { appId: string; userId: string }) => {
    return http.get(`${CONSOLE_BASE}${authAccessControlPath(params.appId, params.userId)}`);
  },

  /**
   * 注销账号
   */
  deleteAccount: (sessionToken: string) =>
    http.delete<string>(`${BASE_URL}${AUTH_ENDPOINT_CONTRACT.paths.deleteAccount}`, { session_token: sessionToken }, { 'x-a1-authorization': `Bearer ${sessionToken}` }),

  /**
   * 使用临时授权码进行授权
   **/
  getAccessTokenWithTempCode: (
    params: { code: string; appId: string },
    token: string,
  ) => {
    return http.post<{
      access_token: string;
      token_type: string;
      expires_in: number;
      refresh_token: string;
    }>(
      `${CONSOLE_BASE}${authClientAppPath(params.appId)}`,
      {
        code: params.code,
      },
      // {
      //   'X-A1-Authorization': `Bearer ${token}`,
      // },
    );
  },
};

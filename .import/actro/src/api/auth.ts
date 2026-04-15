import { CONSOLE_BASE } from './app';
import { http } from './https';
import { AccountUserInfo, AuthAppInfo } from './types';

// Environments
export const ENVS = {
  dev: 'http://account-askaway.develenv.com',
  beta: 'https://account-askaway.betaenv.com',
  prod: 'https://account.askaway.chat',
};

export const CURRENT_ENV: keyof typeof ENVS = 'prod';
export const BASE_URL = ENVS[CURRENT_ENV];

export const authApi = {
  /**
   * 判断用户是否已登录
   */
  checkSession: (token: string) =>
    http.get<string>(`${BASE_URL}/sso/session?idp_token=${token}`),

  /**
   * 获取用户信息
   */
  getUserInfo: (token: string) =>
    http.get<AccountUserInfo>(`${BASE_URL}/protocol/userinfo`, {
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
    http.post<string>(`${BASE_URL}/sso/providers/wechat/session`, { code }),

  /**
   * 通过苹果的 id_token 获取登录凭证
   */
  loginWithApple: (idToken: string) =>
    http.post<string>(`${BASE_URL}/sso/providers/apple/session`, {
      id_token: idToken,
    }),

  /**
   * 通过 Google 的 id_token 获取登录凭证
   */
  loginWithGoogle: (idToken: string) =>
    http.post<string>(`${BASE_URL}/sso/providers/google/session`, {
      id_token: idToken,
    }),

  /**
   * 用户点击授权后请求临时授权码
   */
  authorize: (clientId: string, loginHint: string, token: string) =>
    http.post<{ code: string; redirect_uri: string }>(
      `${BASE_URL}/protocol/authorize`,
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
    return http.get(`${CONSOLE_BASE}/api/access_control/${params.appId}/${params.userId}`);
  },

  /**
   * 注销账号
   */
  deleteAccount: (sessionToken: string) =>
    http.delete<string>(`${BASE_URL}/sso/account`, { session_token: sessionToken }, { 'x-a1-authorization': `Bearer ${sessionToken}` }),

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
      `${CONSOLE_BASE}/api/client/app/${params.appId}`,
      {
        code: params.code,
      },
      // {
      //   'X-A1-Authorization': `Bearer ${token}`,
      // },
    );
  },
};

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
   * Determines whether the user is logged in
   */
  checkSession: (token: string) =>
    http.get<string>(`${BASE_URL}${authCheckSessionPath(token)}`),

  /**
   * Access to user information
   */
  getUserInfo: (token: string) =>
    http.get<AccountUserInfo>(`${BASE_URL}${AUTH_ENDPOINT_CONTRACT.paths.userInfo}`, {
      'x-a1-authorization': `Bearer ${token}`,
    }),

  /**
   * Access to basic information for in-house applications
   */
  // getAppInfo: (clientId: string) =>
  //   http.get<AuthAppInfo>(`${BASE_URL}/protocol/appinfo?client_id=${clientId}`),

  /**
   * By micro-intelligence. code Get Login Certificates
   */
  loginWithWechat: (code: string) =>
    http.post<string>(`${BASE_URL}${AUTH_ENDPOINT_CONTRACT.paths.wechatSession}`, { code }),

  /**
   * Through the apple. id_token Get Login Certificates
   */
  loginWithApple: (idToken: string) =>
    http.post<string>(`${BASE_URL}${AUTH_ENDPOINT_CONTRACT.paths.appleSession}`, {
      id_token: idToken,
    }),

  /**
   * Pass. Google It's... id_token Get Login Certificates
   */
  loginWithGoogle: (idToken: string) =>
    http.post<string>(`${BASE_URL}${AUTH_ENDPOINT_CONTRACT.paths.googleSession}`, {
      id_token: idToken,
    }),

  /**
   * Request temporary authorization code after user clicking on authorization
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

  // - To determine whether it has been authorized.
  getIsAuthorized: (params: { appId: string; userId: string }) => {
    return http.get(`${CONSOLE_BASE}${authAccessControlPath(params.appId, params.userId)}`);
  },

  /**
   * Write-off account number
   */
  deleteAccount: (sessionToken: string) =>
    http.delete<string>(`${BASE_URL}${AUTH_ENDPOINT_CONTRACT.paths.deleteAccount}`, { session_token: sessionToken }, { 'x-a1-authorization': `Bearer ${sessionToken}` }),

  /**
   * Use of temporary authorization codes for authorization
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

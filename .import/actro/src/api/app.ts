import { AppInfo } from './types';
import { http } from './https';

// export const CONSOLE_BASE = 'https://console.actrium-beta.example';
export const CONSOLE_BASE = 'https://console-v2.actrium.example';

export const getAppInfo = async (appId: string) => {
  const res = await http.get<AppInfo>(
    `${CONSOLE_BASE}/api/client/app/${appId}`
  );

  if (res?.success && res?.data) {
    // @ts-ignore
    res.data.language_contents = {}
    for (const lang of res.data!.languages) {
      // @ts-ignore
      res.data!.language_contents[lang] = res.data.info[lang]
    }
    // res.data.allow_anonymous_access = true
  }

  return res
};

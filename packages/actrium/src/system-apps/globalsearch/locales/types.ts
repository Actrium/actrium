export type SearchLocale = 'zh-Hans' | 'zh-Hant' | 'en' | 'es' | 'fr' | 'ja' | 'ko' | 'ru';

export interface LocalizedSettingSearchCopy {
  settingsKeywords: string[];
  feedbackKeywords: string[];
  aboutName: string;
  aboutKeywords: string[];
  aboutAppName: string;
  aboutAppKeywords: string[];
  icpName: string;
  icpKeywords: string[];
  logoutKeywords: string[];
}

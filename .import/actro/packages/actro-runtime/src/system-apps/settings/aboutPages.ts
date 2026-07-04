export type AboutPageKey = 'aboutApp' | 'userAgreement' | 'privacyPolicy' | 'icp';

const ABOUT_PAGE_CONFIG: Record<AboutPageKey, { path: string }> = {
  aboutApp: { path: 'Actro.html' },
  userAgreement: { path: 'user.html' },
  privacyPolicy: { path: 'privacy.html' },
  icp: { path: 'icp.html' },
};

const getLanguageSegment = (language: string): 'zh-Hans' | 'en' => {
  return language.toLowerCase().startsWith('zh') ? 'zh-Hans' : 'en';
};

export const getAboutPageUrl = (page: AboutPageKey, language: string): string => {
  const languageSegment = getLanguageSegment(language);
  return `https://www.actro.chat/corp/${languageSegment}/${ABOUT_PAGE_CONFIG[page].path}`;
};

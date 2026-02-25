import zhHans from './locales/zh-Hans.json';

export type I18nResource = typeof zhHans;

declare module 'i18next' {
  interface CustomTypeOptions {
    defaultNS: 'translation';
    resources: {
      translation: I18nResource;
    };
  }
}

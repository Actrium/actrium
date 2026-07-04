import i18n from 'i18next';
import { initReactI18next } from 'react-i18next';
import * as RNLocalize from 'react-native-localize';
import './types';

import en from './locales/en.json';
import es from './locales/es.json';
import fr from './locales/fr.json';
import ja from './locales/ja.json';
import ko from './locales/ko.json';
import ru from './locales/ru.json';
import zhHans from './locales/zh-Hans.json';
import zhHant from './locales/zh-Hant.json';

export const resources = {
  en: { translation: en },
  es: { translation: es },
  fr: { translation: fr },
  ja: { translation: ja },
  ko: { translation: ko },
  ru: { translation: ru },
  'zh-Hans': { translation: zhHans },
  'zh-Hant': { translation: zhHant },
};

// 识别逻辑
const languageDetector = () => {
  const locales = RNLocalize.getLocales();
  if (!locales || locales.length === 0) return 'en';

  const { languageTag, languageCode, scriptCode, countryCode } = locales[0];

  // 1. 特殊处理中文变体
  if (languageCode === 'zh') {
    // 优先看 script（Hans/Hant），避免 zh-Hans-HK 被误判为繁体
    const normalizedScript = (scriptCode || '').toLowerCase();
    const normalizedTag = (languageTag || '').toLowerCase();

    if (
      normalizedScript === 'hans' ||
      normalizedTag.includes('hans')
    ) {
      return 'zh-Hans';
    }

    if (
      normalizedScript === 'hant' ||
      normalizedTag.includes('hant')
    ) {
      return 'zh-Hant';
    }

    // script 缺失时，再按区域兜底
    if (['TW', 'HK', 'MO'].includes(countryCode || '')) {
      return 'zh-Hant';
    }

    return 'zh-Hans';
  }

  // 2. 匹配其他 6 种支持的语言
  const supportedBaseLanguages = ['en', 'es', 'fr', 'ja', 'ko', 'ru'];
  if (supportedBaseLanguages.includes(languageCode)) {
    return languageCode;
  }

  // 3. 不在支持列表中，默认为 en
  return 'en';
};

i18n
  .use(initReactI18next)
  .init({
    compatibilityJSON: 'v4',
    resources,
    lng: languageDetector(),
    fallbackLng: 'zh-Hans', // 如果 key 缺失，默认使用 zh-Hans
    interpolation: {
      escapeValue: false,
    },
  });

export default i18n;

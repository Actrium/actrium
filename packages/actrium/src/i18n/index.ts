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

// Recognize Logic
const languageDetector = () => {
  const locales = RNLocalize.getLocales();
  if (!locales || locales.length === 0) return 'en';

  const { languageTag, languageCode, scriptCode, countryCode } = locales[0];

  // 1. Special handling of Chinese variants
  if (languageCode === 'zh') {
    // First priority. script(Hans/Hant) , avoid zh-Hans-HK I'm mistaken for a big deal.
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

    // script When missing, press the area bottom
    if (['TW', 'HK', 'MO'].includes(countryCode || '')) {
      return 'zh-Hant';
    }

    return 'zh-Hans';
  }

  // 2. Matches Other 6 Language supported
  const supportedBaseLanguages = ['en', 'es', 'fr', 'ja', 'ko', 'ru'];
  if (supportedBaseLanguages.includes(languageCode)) {
    return languageCode;
  }

  // 3. Not in Support List, Default is en
  return 'en';
};

i18n
  .use(initReactI18next)
  .init({
    compatibilityJSON: 'v4',
    resources,
    lng: languageDetector(),
    fallbackLng: 'zh-Hans', // If key Missing, default zh-Hans
    interpolation: {
      escapeValue: false,
    },
  });

export default i18n;

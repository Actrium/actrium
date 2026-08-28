import type { Theme } from 'vitepress';
import DefaultTheme from 'vitepress/theme';
import { persistLocaleOnManualSwitch } from './prefer-locale';

const theme: Theme = {
  extends: DefaultTheme,
  enhanceApp({ siteData }) {
    if (typeof window === 'undefined') return;
    persistLocaleOnManualSwitch(siteData.value.base);
  },
};

export default theme;

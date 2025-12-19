// rill/vitest.config.ts
import { defineConfig } from 'vitest/config';
import { webdriverio } from '@vitest/browser-webdriverio';

export default defineConfig({
  test: {
    browser: {
      enabled: true,
      name: 'chrome', // webdriverio needs a specific browser
      provider: webdriverio,
    },
  },
});

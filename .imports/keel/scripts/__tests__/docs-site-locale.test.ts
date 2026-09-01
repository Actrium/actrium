import { describe, expect, test } from 'bun:test';
import vm from 'node:vm';
import {
  browserLocaleBootstrapScript,
  decideLocaleRedirect,
  localeFromPath,
  STORAGE_KEY,
  swapLocalePath,
} from '../../docs/.vitepress/theme/prefer-locale';

const BASE = '/keel/';

describe('localeFromPath / swapLocalePath', () => {
  test('treats /zh/ as Chinese and everything else as English', () => {
    expect(localeFromPath('/keel/', BASE)).toBe('en');
    expect(localeFromPath('/keel/guides/cli.html', BASE)).toBe('en');
    expect(localeFromPath('/keel/zh/', BASE)).toBe('zh');
    expect(localeFromPath('/keel/zh/guides/cli.html', BASE)).toBe('zh');
    expect(localeFromPath('/keel/zh', BASE)).toBe('zh');
  });

  test('swaps the locale prefix without losing the rest of the path', () => {
    expect(swapLocalePath('/keel/guides/cli.html', BASE, 'zh')).toBe('/keel/zh/guides/cli.html');
    expect(swapLocalePath('/keel/zh/guides/cli.html', BASE, 'en')).toBe('/keel/guides/cli.html');
    expect(swapLocalePath('/keel/', BASE, 'zh')).toBe('/keel/zh/');
    expect(swapLocalePath('/keel/zh/', BASE, 'en')).toBe('/keel/');
  });
});

describe('decideLocaleRedirect', () => {
  test('default is English: an English browser is not moved', () => {
    expect(
      decideLocaleRedirect({
        pathname: '/keel/guides/cli.html',
        base: BASE,
        stored: null,
        browserLanguage: 'en-US',
      })
    ).toBeNull();
  });

  test('a Chinese browser on an English URL is sent to /zh/', () => {
    expect(
      decideLocaleRedirect({
        pathname: '/keel/guides/cli.html',
        search: '?from=nav',
        hash: '#timeout',
        base: BASE,
        stored: null,
        browserLanguage: 'zh-CN',
      })
    ).toBe('/keel/zh/guides/cli.html?from=nav#timeout');
  });

  test('a shared Chinese URL is not stolen from an English browser', () => {
    expect(
      decideLocaleRedirect({
        pathname: '/keel/zh/api/',
        base: BASE,
        stored: null,
        browserLanguage: 'en-US',
      })
    ).toBeNull();
  });

  test('a saved manual choice wins over the browser', () => {
    expect(
      decideLocaleRedirect({
        pathname: '/keel/zh/guides/cli.html',
        base: BASE,
        stored: 'en',
        browserLanguage: 'zh-CN',
      })
    ).toBe('/keel/guides/cli.html');
    expect(
      decideLocaleRedirect({
        pathname: '/keel/guides/cli.html',
        base: BASE,
        stored: 'zh',
        browserLanguage: 'en-US',
      })
    ).toBe('/keel/zh/guides/cli.html');
  });

  test('a root base (private Pages unique origin) still swaps / and /zh/', () => {
    expect(localeFromPath('/', '/')).toBe('en');
    expect(localeFromPath('/zh/', '/')).toBe('zh');
    expect(swapLocalePath('/guides/cli.html', '/', 'zh')).toBe('/zh/guides/cli.html');
    expect(
      decideLocaleRedirect({
        pathname: '/guides/cli.html',
        base: '/',
        stored: null,
        browserLanguage: 'zh-CN',
      })
    ).toBe('/zh/guides/cli.html');
  });
});

describe('browserLocaleBootstrapScript', () => {
  test('the inlined head script applies the same redirect rules', () => {
    const script = browserLocaleBootstrapScript(BASE);
    let replaced: string | null = null;
    const context = {
      location: {
        pathname: '/keel/getting-started/',
        search: '',
        hash: '',
        replace(url: string) {
          replaced = url;
        },
      },
      localStorage: {
        getItem(key: string) {
          expect(key).toBe(STORAGE_KEY);
          return null;
        },
      },
      navigator: { language: 'zh-TW' },
    };
    vm.runInNewContext(script, context);
    expect(replaced).toBe('/keel/zh/getting-started/');
  });
});

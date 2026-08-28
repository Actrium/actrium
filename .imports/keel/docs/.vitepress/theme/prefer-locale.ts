/**
 * Locale preference for the docs site.
 *
 * Default published locale is English (`foo.md` at `/`). Chinese lives at
 * `/zh/` from `foo.zh.md`. On a first visit with no saved choice, a Chinese
 * browser (`zh`, `zh-CN`, `zh-TW`, ...) is sent to the matching `/zh/` URL.
 * A language-switcher click saves the choice and wins over the browser.
 */

export const STORAGE_KEY = 'keel-docs-locale';

export type DocsLocale = 'en' | 'zh';

export function stripBase(pathname: string, base: string): string {
  const normalizedBase = base.endsWith('/') ? base : `${base}/`;
  if (pathname === normalizedBase.slice(0, -1)) return '';
  if (pathname.startsWith(normalizedBase)) return pathname.slice(normalizedBase.length);
  return pathname.replace(/^\//, '');
}

export function localeFromPath(pathname: string, base: string): DocsLocale {
  const rest = stripBase(pathname, base);
  if (rest === 'zh' || rest.startsWith('zh/')) return 'zh';
  return 'en';
}

export function swapLocalePath(pathname: string, base: string, locale: DocsLocale): string {
  const rest = stripBase(pathname, base);
  const withoutZh = rest === 'zh' ? '' : rest.startsWith('zh/') ? rest.slice('zh/'.length) : rest;
  const normalizedBase = base.endsWith('/') ? base : `${base}/`;
  if (locale === 'zh') return `${normalizedBase}zh/${withoutZh}`;
  return `${normalizedBase}${withoutZh}`;
}

/**
 * Where the browser should go, or `null` to stay.
 *
 * Stored manual choice always wins. Otherwise a Chinese browser viewing an
 * English URL is sent to `/zh/` (the site default is English, so an English
 * browser is never auto-moved). An explicit `/zh/` URL with no stored choice
 * is left alone so a shared Chinese link still opens in Chinese.
 *
 * This function is inlined into the head script via `Function#toString`, so
 * it must not close over anything in this module.
 */
export function decideLocaleRedirect(input: {
  pathname: string;
  search?: string;
  hash?: string;
  base: string;
  stored: string | null;
  browserLanguage: string;
}): string | null {
  const base = input.base.endsWith('/') ? input.base : `${input.base}/`;
  let rest = input.pathname.startsWith(base)
    ? input.pathname.slice(base.length)
    : input.pathname === base.slice(0, -1)
      ? ''
      : input.pathname.replace(/^\//, '');
  const current = rest === 'zh' || rest.startsWith('zh/') ? 'zh' : 'en';
  const stored = input.stored === 'en' || input.stored === 'zh' ? input.stored : null;
  const want =
    stored ?? (/^zh\b/i.test(input.browserLanguage) && current === 'en' ? 'zh' : current);
  if (want === current) return null;
  if (rest === 'zh') rest = '';
  else if (rest.startsWith('zh/')) rest = rest.slice(3);
  const nextPath = want === 'zh' ? `${base}zh/${rest}` : `${base}${rest}`;
  return `${nextPath}${input.search ?? ''}${input.hash ?? ''}`;
}

export function browserLocaleBootstrapScript(base: string): string {
  return `(function(){try{var KEY=${JSON.stringify(STORAGE_KEY)};var next=(${decideLocaleRedirect.toString()})({pathname:location.pathname,search:location.search,hash:location.hash,base:${JSON.stringify(base)},stored:localStorage.getItem(KEY),browserLanguage:navigator.language||navigator.userLanguage||""});if(next)location.replace(next)}catch(e){}})();`;
}

/** Persist only when the user crosses locales (the language menu). */
export function persistLocaleOnManualSwitch(base: string): void {
  document.addEventListener(
    'click',
    (event) => {
      const target = event.target;
      if (!(target instanceof Element)) return;
      const anchor = target.closest('a');
      if (!anchor) return;
      const href = anchor.getAttribute('href');
      if (!href || href.startsWith('#') || href.startsWith('mailto:')) return;
      let destination: URL;
      try {
        destination = new URL(href, window.location.href);
      } catch {
        return;
      }
      if (destination.origin !== window.location.origin) return;
      const from = localeFromPath(window.location.pathname, base);
      const to = localeFromPath(destination.pathname, base);
      if (from === to) return;
      try {
        localStorage.setItem(STORAGE_KEY, to);
      } catch {
        // Private mode can throw; the URL still changes, the next visit
        // falls back to browser language.
      }
    },
    true
  );
}

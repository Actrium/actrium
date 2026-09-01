/**
 * VitePress site config.
 *
 * The file is `.mts` (not `.ts`) because this package is not
 * `"type": "module"`. Vite's config bundler would otherwise `require()`
 * the ESM-only `vitepress` package and fail. Scripts in package.json
 * invoke the VitePress CLI with Node for the same reason -- `bun run`
 * would hit the same loader.
 */
import { fileURLToPath } from 'node:url';
import { defineConfig } from 'vitepress';
import { enLocale } from './locales/en';
import { zhLocale, zhSearch } from './locales/zh';
import {
  rewriteMarkdownHref,
  rewritePagePath,
  SITE_EXCLUDED_PAGES,
  sourceFileFromEnv,
} from './rewrite';
import { browserLocaleBootstrapScript } from './theme/prefer-locale';

const srcDir = fileURLToPath(new URL('..', import.meta.url));

/**
 * Private Pages are served at a unique `*.pages.github.io` origin with the
 * artifact at `/`, not at `https://actrium.github.io/keel/` (that path is
 * the public-project-site URL, which this site is not).
 */
const BASE = '/';

export default defineConfig({
  title: 'Keel',
  description: 'Lightweight, headless, sandboxed dynamic UI rendering engine for React Native.',
  lang: 'en-US',
  base: BASE,
  srcDir: '.',
  srcExclude: [...SITE_EXCLUDED_PAGES, '.vitepress/**'],
  lastUpdated: true,
  cleanUrls: false,
  ignoreDeadLinks: [
    (url) => url.includes('contracts/'),
    /^https?:\/\/(localhost|127\.0\.0\.1)[:/]/,
  ],
  rewrites: rewritePagePath,
  head: [['script', {}, browserLocaleBootstrapScript(BASE)]],
  markdown: {
    config(md) {
      const original = md.renderer.rules.link_open;
      md.renderer.rules.link_open = (tokens, idx, options, env, self) => {
        const token = tokens[idx];
        if (token) {
          const href = token.attrGet('href');
          if (href) {
            token.attrSet('href', rewriteMarkdownHref(href, sourceFileFromEnv(env, srcDir)));
          }
        }
        if (original) return original(tokens, idx, options, env, self);
        return self.renderToken(tokens, idx, options);
      };
    },
  },
  locales: {
    root: enLocale,
    zh: zhLocale,
  },
  themeConfig: {
    siteTitle: 'Keel',
    socialLinks: [{ icon: 'github', link: 'https://github.com/Actrium/actrium' }],
    search: {
      provider: 'local',
      options: {
        locales: {
          zh: zhSearch,
        },
      },
    },
  },
});

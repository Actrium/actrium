/**
 * Map the on-disk docs tree onto VitePress routes without moving files.
 *
 * GitHub (and check:cjk) treat a sibling pair as the source of truth:
 * `foo.md` is English, `foo.zh.md` is Chinese. VitePress locales instead
 * want `foo.md` at `/` and `foo.md` again under `/zh/`. Rewrites plus a
 * markdown-it href pass keep both views honest: the files stay put for
 * GitHub, and the site sees a locale tree.
 *
 * `README.md` becomes `index.md` because VitePress (and most static hosts)
 * treat `index` as the folder URL; GitHub already renders `README.md` as
 * the folder page, so renaming on disk would only hurt the GitHub view.
 */

import path from 'node:path';
import posix from 'node:path/posix';

/** Contributor design notes. GitHub keeps them; the product site does not. */
export const SITE_EXCLUDED_PAGES: readonly string[] = [
  'cdp-debugger-test-manual.zh.md',
  'native-guest.zh.md',
  'native-guest-debugging.zh.md',
  'native-guest-deferred.zh.md',
  'native-guest-threads.zh.md',
];

const ZH_SUFFIX = '.zh.md';

export interface DocsMarkdownEnv {
  realPath?: string;
  path?: string;
}

/** Original path relative to `docs/`, using POSIX separators. */
export function sourceFileFromEnv(env: DocsMarkdownEnv, srcDir: string): string {
  const absolute = env.realPath ?? env.path ?? '';
  if (!absolute) return '';
  return posix.normalize(path.relative(srcDir, absolute).split('\\').join('/'));
}

/**
 * VitePress `rewrites`: source page (relative to `docs/`) -> published page.
 * Identity mappings are returned as-is; VitePress ignores those.
 */
export function rewritePagePath(id: string): string {
  const normalized = id.replace(/\\/g, '/');
  const isZh = normalized.endsWith(ZH_SUFFIX);
  let next = isZh ? `${normalized.slice(0, -ZH_SUFFIX.length)}.md` : normalized;
  next = readmeToIndex(next);
  return isZh ? `zh/${next}` : next;
}

/**
 * Rewrite a markdown href as authored on disk (GitHub-relative) so VitePress
 * resolves it against the *rewritten* page path.
 *
 * - Inside a `.zh.md` file, `./foo.zh.md` is the Chinese sibling: drop `.zh`.
 * - Inside an English file, `./foo.zh.md` is a language switch: jump to `/zh/...`.
 * - `README.md` / `README.zh.md` become `index.md` so folder links resolve.
 * - External, protocol-relative, and hash-only hrefs are left alone.
 */
export function rewriteMarkdownHref(href: string, sourceFile: string): string {
  if (shouldLeaveHref(href)) return href;

  const { pathPart, suffix } = splitHref(href);
  if (pathPart === '') return href;

  const sourceIsZh = sourceFile.endsWith(ZH_SUFFIX);

  if (!sourceIsZh && pathPart.endsWith(ZH_SUFFIX)) {
    const resolved = posix.normalize(posix.join(posix.dirname(sourceFile), pathPart));
    return `/${pageHref(rewritePagePath(resolved))}${suffix}`;
  }

  let next = pathPart;
  if (sourceIsZh) {
    next = next.replace(/\.zh\.md$/, '.md');
  }
  next = readmeToIndex(next);
  return `${next}${suffix}`;
}

function readmeToIndex(pagePath: string): string {
  return pagePath.replace(/(^|\/)README\.md$/, '$1index.md');
}

function pageHref(pagePath: string): string {
  if (pagePath === 'index.md') return '';
  if (pagePath.endsWith('/index.md')) return pagePath.slice(0, -'index.md'.length);
  if (pagePath.endsWith('.md')) return pagePath.slice(0, -'.md'.length);
  return pagePath;
}

function shouldLeaveHref(href: string): boolean {
  if (!href) return true;
  if (href.startsWith('#')) return true;
  if (href.startsWith('//')) return true;
  if (href.startsWith('/')) return true;
  if (/^[a-z][a-z0-9+.-]*:/i.test(href)) return true;
  return false;
}

function splitHref(href: string): { pathPart: string; suffix: string } {
  const hashIndex = href.indexOf('#');
  const queryIndex = href.indexOf('?');
  let splitAt = -1;
  if (hashIndex !== -1 && queryIndex !== -1) splitAt = Math.min(hashIndex, queryIndex);
  else if (hashIndex !== -1) splitAt = hashIndex;
  else if (queryIndex !== -1) splitAt = queryIndex;
  if (splitAt === -1) return { pathPart: href, suffix: '' };
  return { pathPart: href.slice(0, splitAt), suffix: href.slice(splitAt) };
}

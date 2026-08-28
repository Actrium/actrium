import { describe, expect, test } from 'bun:test';
import {
  rewriteMarkdownHref,
  rewritePagePath,
  sourceFileFromEnv,
} from '../../docs/.vitepress/rewrite';

describe('rewritePagePath', () => {
  test('leaves English pages in place except README -> index', () => {
    expect(rewritePagePath('guides/cli.md')).toBe('guides/cli.md');
    expect(rewritePagePath('README.md')).toBe('index.md');
    expect(rewritePagePath('getting-started/README.md')).toBe('getting-started/index.md');
  });

  test('moves .zh.md siblings under /zh/ and drops the suffix', () => {
    expect(rewritePagePath('guides/cli.zh.md')).toBe('zh/guides/cli.md');
    expect(rewritePagePath('README.zh.md')).toBe('zh/index.md');
    expect(rewritePagePath('getting-started/README.zh.md')).toBe('zh/getting-started/index.md');
  });
});

describe('rewriteMarkdownHref', () => {
  test('leaves external, hash, and site-absolute hrefs alone', () => {
    expect(rewriteMarkdownHref('https://github.com/Actrium/keel', 'README.md')).toBe(
      'https://github.com/Actrium/keel'
    );
    expect(rewriteMarkdownHref('#audience-guide', 'README.md')).toBe('#audience-guide');
    expect(rewriteMarkdownHref('/zh/api/guest', 'README.md')).toBe('/zh/api/guest');
  });

  test('Chinese pages drop .zh from sibling links and map README', () => {
    expect(rewriteMarkdownHref('./host-integration.zh.md', 'getting-started/README.zh.md')).toBe(
      './host-integration.md'
    );
    expect(rewriteMarkdownHref('./README.zh.md', 'getting-started/guest-development.zh.md')).toBe(
      './index.md'
    );
    expect(
      rewriteMarkdownHref('../reference/sandbox-comparison.zh.md#matrix', 'guides/production.zh.md')
    ).toBe('../reference/sandbox-comparison.md#matrix');
  });

  test('English pages treat .zh.md links as a locale switch', () => {
    expect(rewriteMarkdownHref('./sandbox-comparison.zh.md', 'reference/sandbox-comparison.md')).toBe(
      '/zh/reference/sandbox-comparison'
    );
    expect(rewriteMarkdownHref('./README.zh.md', 'getting-started/README.md')).toBe(
      '/zh/getting-started/'
    );
  });

  test('English README.md links become index.md', () => {
    expect(rewriteMarkdownHref('./README.md', 'getting-started/guest-development.md')).toBe(
      './index.md'
    );
    expect(rewriteMarkdownHref('./getting-started/host-integration.md', 'README.md')).toBe(
      './getting-started/host-integration.md'
    );
  });
});

describe('sourceFileFromEnv', () => {
  test('prefers realPath (original file) over the rewritten path', () => {
    expect(
      sourceFileFromEnv(
        {
          realPath: '/repo/docs/guides/cli.zh.md',
          path: '/repo/docs/zh/guides/cli.md',
        },
        '/repo/docs'
      )
    ).toBe('guides/cli.zh.md');
  });
});

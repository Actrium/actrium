import React, { useContext, useEffect, useLayoutEffect, useMemo, useState } from 'react';
import { Text } from 'react-native';
import '@goaskaway/supramark-markdown-native-rn';
import { parse } from '@goaskaway/supramark-core/rn';
import { Supramark as SupramarkRenderer } from '@goaskaway/supramark-rn';
import { InitialScrollContent } from 'keel/host/preset';
import { DocumentSelectionContext } from './DocumentSelectionSurface';

const config = { features: [{ id: '@supramark/feature-math', enabled: false }] };

function displayNodes(node) {
  const children = node.children?.flatMap(displayNodes);
  switch (node.type) {
    case 'link':
    case 'container':
      return children ?? [];
    case 'image':
      return [{ type: 'text', value: node.alt ?? '' }];
    case 'diagram':
      return [{ type: 'code', value: node.code }];
    default:
      return [children ? { ...node, children } : node];
  }
}

function errorDocument(text) {
  return {
    type: 'root', ast_version: 2, diagnostics: [],
    children: [{ type: 'paragraph', children: [{ type: 'text', value: text }] }],
  };
}

function useMarkdownSelection(rendererKey) {
  const surface = useContext(DocumentSelectionContext);
  // Appended revisions share a mounted renderer and its selection handle.
  const selection = useMemo(() => {
    let handle = null;
    return {
      enabled: true,
      toolbar: false,
      onHandleReady(next) {
        if (handle) surface?.update(handle, false);
        handle = next;
      },
      onSelectionChange(range) {
        if (handle) surface?.update(handle, range != null && (
          range.anchor.unitId !== range.focus.unitId || range.anchor.offset !== range.focus.offset
        ));
      },
    };
  }, [surface, rendererKey]);
  useLayoutEffect(() => () => selection.onHandleReady(null), [selection]);
  return selection;
}

function MarkdownDocument({ markdown, theme, paddingHorizontal, variant }) {
  const [result, setResult] = useState({ document: null, failedMarkdown: null });
  const error = variant === 'error';
  const literalKey = error ? markdown : undefined;
  const styles = useMemo(() => (
    paddingHorizontal === undefined ? undefined : { root: { paddingHorizontal } }
  ), [paddingHorizontal]);
  const literal = useMemo(() => error ? errorDocument(markdown) : null, [error, markdown]);
  const parsed = result.document;
  const matchingDocument = parsed && markdown.startsWith(parsed.markdown) ? parsed : null;
  const document = literal ? { markdown, ast: literal } : matchingDocument;
  const failed = !literal && result.failedMarkdown === markdown;
  const selection = useMarkdownSelection(!failed && document ? literalKey : null);

  useEffect(() => {
    if (error) return;
    let active = true;
    parse(markdown, { config }).then((root) => displayNodes(root)[0]).then(
      (ast) => {
        if (active) setResult({ document: { markdown, ast }, failedMarkdown: null });
      },
      () => {
        if (active) setResult((previous) => ({ ...previous, failedMarkdown: markdown }));
      }
    );
    return () => { active = false; };
  }, [error, markdown]);

  if (failed) {
    return <InitialScrollContent><Text style={styles?.root}>Could not parse Markdown.</Text></InitialScrollContent>;
  }
  // The renderer registers its selection handle on mount, so each literal revision needs a new mount.
  return <InitialScrollContent>
    {document ? <SupramarkRenderer key={literalKey} markdown={document.markdown} ast={document.ast}
      config={config} theme={theme} selection={selection} styles={styles} /> : null}
  </InitialScrollContent>;
}

export function Supramark({ markdown, theme, paddingHorizontal, variant }) {
  if (typeof markdown !== 'string' || (variant !== 'error' && markdown.length > 64 * 1024)) {
    return <Text>Invalid Markdown source.</Text>;
  }
  if (paddingHorizontal !== undefined && (!Number.isFinite(paddingHorizontal) || paddingHorizontal < 0)) {
    return <Text>Invalid Markdown padding.</Text>;
  }
  if (variant !== undefined && variant !== 'error') {
    return <Text>Invalid Markdown variant.</Text>;
  }
  return <MarkdownDocument key={variant} markdown={markdown} theme={theme === 'dark' ? 'dark' : 'light'}
    paddingHorizontal={paddingHorizontal} variant={variant} />;
}

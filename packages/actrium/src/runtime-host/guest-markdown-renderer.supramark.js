import React, { useContext, useEffect, useLayoutEffect, useMemo, useState } from 'react';
import { Text } from 'react-native';
import '@goaskaway/supramark-markdown-native-rn';
import { parse } from '@goaskaway/supramark-core/rn';
import { Supramark as SupramarkRenderer } from '@goaskaway/supramark-rn';
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

function MarkdownDocument({ markdown, theme, paddingHorizontal, variant }) {
  const [result, setResult] = useState(null);
  const surface = useContext(DocumentSelectionContext);
  const error = variant === 'error';
  const styles = useMemo(() => (
    paddingHorizontal === undefined ? undefined : { root: { paddingHorizontal } }
  ), [paddingHorizontal]);
  const literal = useMemo(() => error ? errorDocument(markdown) : null, [error, markdown]);
  // Give each Markdown revision its own handle so late cleanup cannot clear its replacement.
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
  }, [surface, markdown]);
  useLayoutEffect(() => () => selection.onHandleReady(null), [selection]);
  useEffect(() => {
    if (error) return;
    let active = true;
    parse(markdown, { config }).then((root) => displayNodes(root)[0]).then(
      (ast) => {
        if (active) setResult({ markdown, ast });
      },
      () => {
        if (active) setResult({ markdown, error: true });
      }
    );
    return () => { active = false; };
  }, [error, markdown]);

  if (!literal && result?.markdown !== markdown) return null;
  if (!literal && result.error) return <Text style={styles?.root}>Could not parse Markdown.</Text>;
  // The renderer registers its selection handle on mount, so each literal revision needs a new mount.
  return <SupramarkRenderer key={error ? markdown : undefined} markdown={markdown} ast={literal ?? result.ast}
    config={config} theme={theme} selection={selection} styles={styles} />;
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

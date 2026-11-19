import React, { useContext, useEffect, useMemo, useState } from 'react';
import { Text } from 'react-native';
import '@supramark/markdown-native-rn';
import { parse } from '@supramark/core';
import { Supramark as SupramarkRenderer } from '@supramark/rn';
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

function MarkdownDocument({ markdown, theme, paddingHorizontal }) {
  const [result, setResult] = useState(null);
  const surface = useContext(DocumentSelectionContext);
  const styles = useMemo(() => (
    paddingHorizontal === undefined ? undefined : { root: { paddingHorizontal } }
  ), [paddingHorizontal]);
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
        if (handle) surface?.update(handle, range !== null && (
          range.anchor.unitId !== range.focus.unitId || range.anchor.offset !== range.focus.offset
        ));
      },
    };
  }, [surface]);
  useEffect(() => {
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
  }, [markdown]);

  if (result?.markdown !== markdown) return null;
  if (result.error) return <Text style={styles?.root}>Could not parse Markdown.</Text>;
  return <SupramarkRenderer markdown={markdown} ast={result.ast} config={config} theme={theme} selection={selection} styles={styles} />;
}

export function Supramark({ markdown, theme, paddingHorizontal }) {
  if (typeof markdown !== 'string' || markdown.length > 64 * 1024) {
    return <Text>Invalid Markdown source.</Text>;
  }
  if (paddingHorizontal !== undefined && (!Number.isFinite(paddingHorizontal) || paddingHorizontal < 0)) {
    return <Text>Invalid Markdown padding.</Text>;
  }
  return <MarkdownDocument markdown={markdown} theme={theme === 'dark' ? 'dark' : 'light'} paddingHorizontal={paddingHorizontal} />;
}

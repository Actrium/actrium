import React, { useEffect, useState } from 'react';
import { Text } from 'react-native';
import '@supramark/markdown-native-rn';
import { parse } from '@supramark/core';
import { Supramark as SupramarkRenderer } from '@supramark/rn';

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

function MarkdownDocument({ markdown, theme }) {
  const [result, setResult] = useState(null);
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
  if (result.error) return <Text>Could not parse Markdown.</Text>;
  return <SupramarkRenderer markdown={markdown} ast={result.ast} config={config} theme={theme} />;
}

export function Supramark({ markdown, theme }) {
  if (typeof markdown !== 'string' || markdown.length > 64 * 1024) {
    return <Text>Invalid Markdown source.</Text>;
  }
  return <MarkdownDocument markdown={markdown} theme={theme === 'dark' ? 'dark' : 'light'} />;
}

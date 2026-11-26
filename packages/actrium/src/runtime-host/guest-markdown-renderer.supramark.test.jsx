import { afterEach, beforeEach, expect, mock, test } from 'bun:test';
import React, { act, useEffect, useRef } from 'react';
import { create } from 'react-test-renderer';
import { createDocumentSelection } from './document-selection';

const paragraph = (value) => ({ type: 'paragraph', children: [{ type: 'text', value }] });
const parse = mock(async (source) => ({ type: 'root', children: [paragraph('Parsed: ' + source)] }));
const DocumentSelectionContext = React.createContext(null);
const range = {
  anchor: { unitId: 'text', offset: 0 },
  focus: { unitId: 'text', offset: 1 },
};

function Renderer({ ast, selection, theme }) {
  const text = ast.children[0].children[0].value;
  const currentText = useRef(text);
  currentText.current = text;
  // Supramark provides a stable handle for the lifetime of the renderer.
  useEffect(() => {
    selection.onHandleReady({ getSelectedText: () => currentText.current, clearSelection() {} });
    return () => selection.onHandleReady(null);
  }, []);
  return <paragraph theme={theme} onSelect={() => selection.onSelectionChange(range)}>{text}</paragraph>;
}

mock.module('react-native', () => ({ Text: 'text' }));
mock.module('@goaskaway/supramark-markdown-native-rn', () => ({}));
mock.module('@goaskaway/supramark-core/rn', () => ({ parse }));
mock.module('@goaskaway/supramark-rn', () => ({ Supramark: Renderer }));
mock.module('./DocumentSelectionSurface', () => ({ DocumentSelectionContext }));

const { Supramark } = await import('./guest-markdown-renderer.supramark');
let renderer;
let actEnvironment;

beforeEach(() => {
  actEnvironment = globalThis.IS_REACT_ACT_ENVIRONMENT;
  globalThis.IS_REACT_ACT_ENVIRONMENT = true;
});

afterEach(async () => {
  await act(async () => renderer?.unmount());
  renderer = undefined;
  parse.mockClear();
  globalThis.IS_REACT_ACT_ENVIRONMENT = actEnvironment;
});

async function render(props, surface) {
  await act(async () => {
    const tree = (
      <DocumentSelectionContext.Provider value={surface}>
        <Supramark {...props} />
      </DocumentSelectionContext.Provider>
    );
    if (renderer) renderer.update(tree);
    else renderer = create(tree);
  });
}

test.each(['light', 'dark'])('renders literal errors as paragraphs in the %s theme', async (theme) => {
  const markdown = '# Failure in **request** with `code`\nDetails';
  await render({ markdown, theme, variant: 'error' });

  expect(renderer.root.findByType(Renderer).props.ast.children).toEqual([paragraph(markdown)]);
  expect(renderer.root.findByType('paragraph').props).toMatchObject({ children: markdown, theme });
  expect(parse).not.toHaveBeenCalled();
});

test('applies the source length limit only to ordinary Markdown', async () => {
  const markdown = 'x'.repeat(64 * 1024 + 1);
  await render({ markdown });
  expect(renderer.toJSON().children).toEqual(['Invalid Markdown source.']);
  await render({ markdown, variant: 'error' });
  expect(renderer.root.findByType('paragraph').props.children).toBe(markdown);
  await render({ markdown: null, variant: 'error' });
  expect(renderer.toJSON().children).toEqual(['Invalid Markdown source.']);
  expect(parse).not.toHaveBeenCalled();
});

test('parses again when returning from error mode with the same text', async () => {
  const markdown = '**failure**';
  await render({ markdown });
  expect(renderer.root.findByType('paragraph').props.children).toBe('Parsed: ' + markdown);
  expect(parse).toHaveBeenCalledTimes(1);
  await render({ markdown, variant: 'error' });
  expect(renderer.root.findByType('paragraph').props.children).toBe(markdown);
  expect(parse).toHaveBeenCalledTimes(1);

  let finishParse;
  parse.mockImplementationOnce(() => new Promise((resolve) => { finishParse = resolve; }));
  await render({ markdown });
  expect(parse).toHaveBeenCalledTimes(2);
  expect(renderer.toJSON()).toBeNull();
  await act(async () => finishParse({ type: 'root', children: [paragraph('Reparsed: ' + markdown)] }));
  expect(renderer.root.findByType('paragraph').props.children).toBe('Reparsed: ' + markdown);
});

test('reports the new selection to the Host after error text changes', async () => {
  const surface = createDocumentSelection();
  const copy = mock();
  await render({ markdown: 'First error', variant: 'error' }, surface);
  renderer.root.findByType('paragraph').props.onSelect();
  surface.copy(copy);
  expect(copy.mock.calls).toEqual([['First error']]);

  await render({ markdown: 'Updated error', variant: 'error' }, surface);
  surface.copy(copy);
  expect(copy).toHaveBeenCalledTimes(1);
  renderer.root.findByType('paragraph').props.onSelect();
  surface.copy(copy);
  expect(copy.mock.calls).toEqual([['First error'], ['Updated error']]);
  expect(parse).not.toHaveBeenCalled();
});

import { expect, test } from 'bun:test';
import { createDocumentSelection } from './document-selection';

test('copy reads the live owner, preserves the range and isolates tabs', () => {
  const selection = createDocumentSelection();
  const otherTab = createDocumentSelection();
  let text = 'first';
  const writes: string[] = [];
  const write = (value: string) => writes.push(value);
  const first = { getSelectedText: () => text, clearSelection: () => { text = ''; } };
  const second = { getSelectedText: () => 'second', clearSelection() {} };
  selection.update(first, true);
  text = 'updated';
  selection.copy(write);
  selection.copy(write);
  otherTab.copy(write);
  expect(writes).toEqual(['updated', 'updated']);
  selection.update(second, true);
  expect(text).toBe('');
  selection.update(first, false);
  selection.copy(write);
  selection.clear();
  selection.copy(write);
  expect(writes).toEqual(['updated', 'updated', 'second']);
});

test('only ownership transitions update the native menu and stale callbacks cannot disable it', () => {
  const states: boolean[] = [];
  const selection = createDocumentSelection(active => states.push(active));
  const first = { getSelectedText: () => 'a', clearSelection: () => selection.update(first, false) };
  const second = { getSelectedText: () => null, clearSelection() {} };
  selection.update(first, true);
  selection.update(first, true);
  selection.update(second, true);
  selection.update(first, false);
  selection.copy(() => { throw new Error('An empty selection must preserve the clipboard'); });
  selection.update(second, false);
  expect(states).toEqual([true, false]);
});

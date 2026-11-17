import { expect, test } from 'bun:test';
import type React from 'react';
import { TextInput as RNTextInput } from 'react-native';
import { act, create } from 'react-test-renderer';
import { waitFor } from './helpers/test-utils';
import { Engine } from '../../engine';
import { TextInput as NativeTextInput } from '../../preset/components/text-input';
import { TextInput as WebTextInput } from '../../web/components';

const guest = (factory: string) => `
  const React = require('react');
  const { TextInput } = require('keel/guest');
  const { render } = require('keel/reconciler');
  const { jsx } = require('react/jsx-runtime');
  const batches = [];
  let hold = false;
  globalThis.holdBatches = () => { hold = true; };
  globalThis.batchCount = () => batches.length;
  globalThis.deliverBatch = () => globalThis.__keel_sendBatch(batches.shift());

  function App() {
    const [text, setText] = React.useState('');
    globalThis.clearText = () => setText('');
    return ${factory}(TextInput, {
      testID: 'editor',
      value: text,
      multiline: ${factory === 'jsx'},
      submitBehavior: 'submit',
      onChangeText: (next) => {
        if (next !== 'rejected') setText(next);
      },
    });
  }

  render(React.createElement(App), (batch) => {
    if (hold) batches.push(batch);
    else globalThis.__keel_sendBatch(batch);
  });
`;

const cases = [
  { name: 'Web', Host: WebTextInput, field: 'input', factory: 'React.createElement' },
  { name: 'Native', Host: NativeTextInput, field: RNTextInput, factory: 'jsx' },
] as const;

for (const { name, Host, field: tag, factory } of cases) {
  test(`${name} Guest value and onChangeText preserve newer edits, rejection and clear`, async () => {
    const engine = new Engine({ sandbox: 'node-vm', debug: false });
    engine.register({ TextInput: Host });
    const receiver = engine.createReceiver();
    let renderer: ReturnType<typeof create> | undefined;
    try {
      await engine.loadBundle(guest(factory));
      await waitFor(() => receiver.findByTestId('editor') !== undefined, 1500);
      act(() => {
        renderer = create(receiver.render() as React.ReactElement);
      });
      const field = () => renderer!.root.findByType(tag);
      if (name === 'Native') {
        expect(field().props.submitBehavior).toBe('submit');
      }
      const call = <T,>(method: string) => engine.context!.extract<() => T>(method)!();
      const waitForBatch = (count: number) => waitFor(() => call<number>('batchCount') === count, 1000);
      const edit = (text: string, eventCount: number) => {
        act(() => {
          const event = name === 'Native' ? { nativeEvent: { text, eventCount } } : { target: { value: text } };
          field().props.onChange(event);
        });
      };
      const deliver = () => act(() => {
        call('deliverBatch');
        renderer!.update(receiver.render() as React.ReactElement);
      });

      call('holdBatches');
      edit('12', 1);
      await waitForBatch(1);
      edit('123', 2);
      await waitForBatch(2);
      deliver();
      expect(field().props.value).toBe('123');
      deliver();
      expect(field().props.value).toBe('123');

      edit('rejected', 3);
      expect(field().props.value).toBe('rejected');
      await waitForBatch(1);
      deliver();
      expect(field().props.value).toBe('123');

      call('clearText');
      await waitForBatch(1);
      deliver();
      expect(field().props.value).toBe('');
    } finally {
      act(() => renderer?.unmount());
      await engine.destroy();
    }
  });
}

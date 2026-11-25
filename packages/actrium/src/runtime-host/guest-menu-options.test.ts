import { expect, test } from 'bun:test';
import { guestMenuEntries } from './guest-menu-options';

const entry = { value: 'open', label: 'Open', disabled: false };

test('copies only the menu option fields sent to the native bridge', () => {
  expect(guestMenuEntries([{ ...entry, extra: 'ignored' }])).toEqual([entry]);
});

test.each([null, 'invalid', { value: 1, label: 'Invalid' }, { ...entry, disabled: 'false' }])(
  'rejects an invalid entry even alongside a valid option',
  (invalid) => {
    expect(() => guestMenuEntries([entry, invalid])).toThrow('Invalid guest menu option.');
  },
);

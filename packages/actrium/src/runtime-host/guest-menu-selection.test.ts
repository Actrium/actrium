import { expect, test } from 'bun:test';
import { completeGuestMenu } from './guest-menu-selection';

test('releases the menu before running the callback and reports its async rejection', async () => {
  const failure = new Error('selection failed');
  const errors: unknown[] = [];
  let released = false;
  await completeGuestMenu(Promise.resolve('open'), () => { released = true; }, async (choice) => {
    expect(choice).toBe('open');
    expect(released).toBe(true);
    throw failure;
  }, (error) => { errors.push(error); });
  expect(errors).toEqual([failure]);
});

test('releases the menu after a native rejection without dispatching a selection', async () => {
  const failure = new Error('native menu failed');
  const errors: unknown[] = [];
  let released = false;
  let dispatched = false;
  await completeGuestMenu(Promise.reject(failure), () => { released = true; }, () => {
    dispatched = true;
  }, (error) => { errors.push(error); });
  expect(released).toBe(true);
  expect(dispatched).toBe(false);
  expect(errors).toEqual([failure]);
});

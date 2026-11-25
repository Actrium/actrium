import { expect, test } from 'bun:test';
import { normalizeGuestSvg } from './guest-svg';

const graphic = {
  width: 48,
  height: 24,
  viewBox: '0, 0, 48, 24',
  color: '#123456',
  paths: [{ d: 'M0 0 L48 24', stroke: 'currentColor', strokeWidth: 2, fill: 'none' }],
};

test('preserves local path artwork and the viewBox proportions', () => {
  expect(normalizeGuestSvg(graphic)).toMatchObject({
    ...graphic,
    viewBox: '0 0 48 24',
  });
});

test('rejects invalid geometry and strips external paint references', () => {
  expect(normalizeGuestSvg({ ...graphic, viewBox: '0 0 24 0' })).toBeUndefined();
  expect(normalizeGuestSvg({ ...graphic, paths: [{ d: '<image href="https://example.com/a.png"/>' }] })).toBeUndefined();
  expect(normalizeGuestSvg({
    ...graphic, paths: [{ d: 'M0 0 L1 1', fill: 'url(https://example.com/a.svg#gradient)' }],
  })?.paths[0]?.fill).toBeUndefined();
});

test.each(['M0', 'M0 0 L9', 'M0 0 A5 5 0 2 0 9 9', 'M0 0 L8e 9', 'M0 0 L1e39 9', 'M0 0,'])(
  'rejects a malformed path before it reaches the native parser',
  (d) => {
    expect(normalizeGuestSvg({ ...graphic, paths: [{ d }] })).toBeUndefined();
  },
);

test('preserves compact SVG syntax while separating operands for the native parser', () => {
  const d = 'm.5-.5 2.5.5 h3 v4 q1 2 3 4t5 6 s7 8 9 10 c1 2 3 4 5 6 a4 5 0 0110 20z';
  expect(normalizeGuestSvg({ ...graphic, paths: [{ d }] })?.paths[0]?.d).toBe(
    'm.5 -.5 l2.5 .5 h3 v4 q1 2 3 4 t5 6 s7 8 9 10 c1 2 3 4 5 6 a4 5 0 0 1 10 20 z',
  );
});

test('keeps the total input limit even when normalization removes whitespace', () => {
  const padded = { d: 'M0 0' + ' '.repeat(16_000) };
  expect(normalizeGuestSvg({ ...graphic, paths: Array(9).fill(padded) })).toBeUndefined();
});

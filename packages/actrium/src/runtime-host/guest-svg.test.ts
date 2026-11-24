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

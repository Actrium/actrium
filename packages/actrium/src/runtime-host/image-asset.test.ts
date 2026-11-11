import { describe, expect, test } from 'bun:test';
import { isAllowedImageSource } from './image-asset';

describe('isAllowedImageSource', () => {
  test('allows package asset ids', () => {
    expect(isAllowedImageSource(42)).toBe(true);
  });

  test('rejects remote and header-bearing sources', () => {
    expect(isAllowedImageSource('https://example.com/image.png')).toBe(false);
    expect(isAllowedImageSource({ uri: 'https://example.com/image.png' })).toBe(
      false
    );
    expect(
      isAllowedImageSource({
        uri: 'https://example.com/image.png',
        headers: { Authorization: 'secret' },
      })
    ).toBe(false);
    expect(isAllowedImageSource(undefined)).toBe(false);
  });
});

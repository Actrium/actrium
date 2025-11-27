/**
 * Toast Android Platform Tests
 *
 * Tests Android-specific code using dependency injection
 */

import { describe, it, expect, beforeEach, vi } from 'vitest';
import { NativeToast, _injectMocks, _resetReactNative } from './Toast.native';

describe('Toast (Android Platform)', () => {
  const mockShowWithGravity = vi.fn();
  const mockToastAndroid = {
    SHORT: 0,
    LONG: 1,
    TOP: 48,
    CENTER: 17,
    BOTTOM: 80,
    showWithGravity: mockShowWithGravity,
  };
  const mockPlatform = { OS: 'android' };

  beforeEach(() => {
    _resetReactNative();
    _injectMocks(mockPlatform, mockToastAndroid);
    mockShowWithGravity.mockClear();
  });

  it('should call ToastAndroid.showWithGravity on Android', () => {
    const toast = new NativeToast();
    toast.show('Android Toast');

    expect(mockShowWithGravity).toHaveBeenCalledWith(
      'Android Toast',
      mockToastAndroid.SHORT,
      mockToastAndroid.BOTTOM
    );
  });

  it('should use SHORT duration for messages <= 2500ms', () => {
    const toast = new NativeToast();
    toast.show('Short', { duration: 'short' });

    expect(mockShowWithGravity).toHaveBeenCalledWith(
      'Short',
      mockToastAndroid.SHORT,
      mockToastAndroid.BOTTOM
    );
  });

  it('should use LONG duration for messages > 2500ms', () => {
    const toast = new NativeToast();
    toast.show('Long', { duration: 'long' });

    expect(mockShowWithGravity).toHaveBeenCalledWith(
      'Long',
      mockToastAndroid.LONG,
      mockToastAndroid.BOTTOM
    );
  });

  it('should use LONG duration for custom duration > 2500ms', () => {
    const toast = new NativeToast();
    toast.show('Custom Long', { duration: 5000 });

    expect(mockShowWithGravity).toHaveBeenCalledWith(
      'Custom Long',
      mockToastAndroid.LONG,
      mockToastAndroid.BOTTOM
    );
  });

  it('should use TOP gravity for position "top"', () => {
    const toast = new NativeToast();
    toast.show('Top Toast', { position: 'top' });

    expect(mockShowWithGravity).toHaveBeenCalledWith(
      'Top Toast',
      mockToastAndroid.SHORT,
      mockToastAndroid.TOP
    );
  });

  it('should use CENTER gravity for position "center"', () => {
    const toast = new NativeToast();
    toast.show('Center Toast', { position: 'center' });

    expect(mockShowWithGravity).toHaveBeenCalledWith(
      'Center Toast',
      mockToastAndroid.SHORT,
      mockToastAndroid.CENTER
    );
  });

  it('should use BOTTOM gravity for position "bottom"', () => {
    const toast = new NativeToast();
    toast.show('Bottom Toast', { position: 'bottom' });

    expect(mockShowWithGravity).toHaveBeenCalledWith(
      'Bottom Toast',
      mockToastAndroid.SHORT,
      mockToastAndroid.BOTTOM
    );
  });

  it('should combine duration and position options', () => {
    const toast = new NativeToast();
    toast.show('Combined', { duration: 'long', position: 'top' });

    expect(mockShowWithGravity).toHaveBeenCalledWith(
      'Combined',
      mockToastAndroid.LONG,
      mockToastAndroid.TOP
    );
  });

  it('should prefer custom handler over Android native', () => {
    const toast = new NativeToast();
    const calls: string[] = [];

    toast._setCustomHandler((msg) => calls.push(msg));
    toast.show('Custom Handler');

    expect(calls).toEqual(['Custom Handler']);
    expect(mockShowWithGravity).not.toHaveBeenCalled();
  });
});

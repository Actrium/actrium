/**
 * Toast Android Platform Tests
 *
 * Tests Android-specific code using dependency injection
 */

interface MockFn {
  (...args: unknown[]): void;
  mock: { calls: unknown[][] };
  mockClear: () => void;
}

function makeMock(): MockFn {
  const calls: unknown[][] = [];
  const fn = ((...args: unknown[]) => {
    calls.push(args);
  }) as MockFn;
  fn.mock = { calls };
  fn.mockClear = () => {
    calls.length = 0;
  };
  return fn;
}

import {
  _injectMocks,
  _resetReactNative,
  HostToast,
  TOAST_SET_HANDLER as SET_HANDLER_SYMBOL,
} from './Toast.host';

describe('Toast (Android Platform)', () => {
  const mockShowWithGravity = makeMock();
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
    const toast = new HostToast();
    toast.show('Android Toast');

    expect(mockShowWithGravity.mock.calls[0]).toEqual([
      'Android Toast',
      mockToastAndroid.SHORT,
      mockToastAndroid.BOTTOM,
    ]);
  });

  it('should use SHORT duration for messages <= 2500ms', () => {
    const toast = new HostToast();
    toast.show('Short', { duration: 'short' });

    expect(mockShowWithGravity.mock.calls[0]).toEqual([
      'Short',
      mockToastAndroid.SHORT,
      mockToastAndroid.BOTTOM,
    ]);
  });

  it('should use LONG duration for messages > 2500ms', () => {
    const toast = new HostToast();
    toast.show('Long', { duration: 'long' });

    expect(mockShowWithGravity.mock.calls[0]).toEqual([
      'Long',
      mockToastAndroid.LONG,
      mockToastAndroid.BOTTOM,
    ]);
  });

  it('should use LONG duration for custom duration > 2500ms', () => {
    const toast = new HostToast();
    toast.show('Custom Long', { duration: 5000 });

    expect(mockShowWithGravity.mock.calls[0]).toEqual([
      'Custom Long',
      mockToastAndroid.LONG,
      mockToastAndroid.BOTTOM,
    ]);
  });

  it('should use TOP gravity for position "top"', () => {
    const toast = new HostToast();
    toast.show('Top Toast', { position: 'top' });

    expect(mockShowWithGravity.mock.calls[0]).toEqual([
      'Top Toast',
      mockToastAndroid.SHORT,
      mockToastAndroid.TOP,
    ]);
  });

  it('should use CENTER gravity for position "center"', () => {
    const toast = new HostToast();
    toast.show('Center Toast', { position: 'center' });

    expect(mockShowWithGravity.mock.calls[0]).toEqual([
      'Center Toast',
      mockToastAndroid.SHORT,
      mockToastAndroid.CENTER,
    ]);
  });

  it('should use BOTTOM gravity for position "bottom"', () => {
    const toast = new HostToast();
    toast.show('Bottom Toast', { position: 'bottom' });

    expect(mockShowWithGravity.mock.calls[0]).toEqual([
      'Bottom Toast',
      mockToastAndroid.SHORT,
      mockToastAndroid.BOTTOM,
    ]);
  });

  it('should combine duration and position options', () => {
    const toast = new HostToast();
    toast.show('Combined', { duration: 'long', position: 'top' });

    expect(mockShowWithGravity.mock.calls[0]).toEqual([
      'Combined',
      mockToastAndroid.LONG,
      mockToastAndroid.TOP,
    ]);
  });

  it('should prefer custom handler over Android native', () => {
    const toast = new HostToast();
    const calls: string[] = [];

    toast[SET_HANDLER_SYMBOL]((msg) => calls.push(msg));
    toast.show('Custom Handler');

    expect(calls).toEqual(['Custom Handler']);
    expect(mockShowWithGravity.mock.calls.length).toBe(0);
  });
});

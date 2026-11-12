import { describe, expect, test } from 'bun:test';
import { isLegacyEchoTab } from './legacy-echo';

describe('isLegacyEchoTab', () => {
  test('accepts the exact echo app id', () => {
    expect(isLegacyEchoTab('actrium/echo-ui')).toBe(true);
  });

  test('accepts a versioned echo app id', () => {
    expect(isLegacyEchoTab('actrium/echo-ui@0.1.0')).toBe(true);
  });

  test('rejects other packages', () => {
    expect(isLegacyEchoTab('actrium/counter@1.0.0')).toBe(false);
    expect(isLegacyEchoTab('acme/echo-ui@0.1.0')).toBe(false);
    expect(isLegacyEchoTab(undefined)).toBe(false);
  });
});

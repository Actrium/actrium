import { describe, expect, test } from 'bun:test';
import {
  classifyServiceCapability,
  isValidServiceCapability,
  validateServiceCapabilities,
} from './service-capabilities';

describe('service capability grammar', () => {
  test('classifies workspace access modes from v1 08', () => {
    expect(classifyServiceCapability('workspace:read-only')).toBe('workspace');
    expect(classifyServiceCapability('workspace:read-write')).toBe('workspace');
    expect(classifyServiceCapability('workspace:read')).toBeUndefined();
    expect(classifyServiceCapability('workspace:write')).toBeUndefined();
  });

  test('classifies https network endpoints only', () => {
    expect(classifyServiceCapability('net:https://api.notes.example')).toBe(
      'net'
    );
    expect(classifyServiceCapability('net:https://api.notes.example:8443')).toBe(
      'net'
    );
    expect(
      classifyServiceCapability('net:https://api.notes.example/v1/items')
    ).toBe('net');
    expect(
      classifyServiceCapability('net:https://api.notes.example:8443/v1')
    ).toBe('net');
    expect(classifyServiceCapability('net:http://api.notes.example')).toBeUndefined();
    expect(
      classifyServiceCapability('net:https://user:pass@evil.example')
    ).toBeUndefined();
    expect(classifyServiceCapability('net:https://foo bar')).toBeUndefined();
    expect(classifyServiceCapability('net:https://..')).toBeUndefined();
    expect(
      classifyServiceCapability('net:https://a..b.example')
    ).toBeUndefined();
    expect(
      classifyServiceCapability('net:https://api.notes.example:0')
    ).toBeUndefined();
    expect(
      classifyServiceCapability('net:https://api.notes.example:65536')
    ).toBeUndefined();
    expect(
      classifyServiceCapability('net:https://api.notes.example:65535')
    ).toBe('net');
    expect(classifyServiceCapability('net:https://example.com#frag')).toBeUndefined();
    expect(classifyServiceCapability('net:https://')).toBeUndefined();
  });

  test('classifies actor calls with a pinned version', () => {
    expect(
      classifyServiceCapability('actor:call:notes.example:index-worker:1')
    ).toBe('actor');
    expect(
      classifyServiceCapability('actor:call:notes.example:index-worker')
    ).toBeUndefined();
  });

  test('classifies the prototype shell extensions', () => {
    expect(classifyServiceCapability('shell:bookmarks')).toBe('shell');
    expect(classifyServiceCapability('shell:history')).toBe('shell');
    expect(classifyServiceCapability('shell:settings')).toBe('shell');
    expect(classifyServiceCapability('shell:unknown')).toBeUndefined();
  });

  test('rejects unknown service capability forms', () => {
    expect(isValidServiceCapability('workspace:read-write')).toBe(true);
    expect(isValidServiceCapability('net:ftp://example.com')).toBe(false);
    expect(isValidServiceCapability('')).toBe(false);
  });

  test('reports invalid declarations instead of dropping them silently', () => {
    const result = validateServiceCapabilities([
      'workspace:read-write',
      'workspace:write',
      'net:http://insecure.example',
      'actor:call:notes.example:index-worker',
      'shell:unknown',
      'mystery:capability',
    ]);

    expect(result.valid).toEqual(['workspace:read-write']);
    expect(result.invalid.map((entry) => entry.capability)).toEqual([
      'workspace:write',
      'net:http://insecure.example',
      'actor:call:notes.example:index-worker',
      'shell:unknown',
      'mystery:capability',
    ]);
    expect(result.invalid[0].reason).toContain('workspace:read-only');
    expect(result.invalid[1].reason).toContain('net:https://');
    expect(result.invalid[2].reason).toContain('actor:call:');
    expect(result.invalid[3].reason).toContain('shell:bookmarks');
    expect(result.invalid[4].reason).toContain('unknown service capability');
  });
});

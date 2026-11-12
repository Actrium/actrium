import { describe, expect, test } from 'bun:test';
import {
  expandCapabilityName,
  hasDeclaredCapability,
} from './capabilities';

describe('Front capability mapping', () => {
  test('expands method-level capability names', () => {
    expect(expandCapabilityName('service.call')).toEqual(['service.call']);
    expect(expandCapabilityName('host:clipboard.write')).toEqual([
      'host:clipboard.write',
    ]);
    expect(expandCapabilityName('host:net.request')).toEqual([
      'host:net.request',
    ]);
  });

  test('expands a module shorthand to every method', () => {
    expect(expandCapabilityName('service')).toEqual([
      'service.call',
      'service.tell',
      'service.on',
    ]);
    expect(expandCapabilityName('host:clipboard')).toEqual([
      'host:clipboard.write',
    ]);
    expect(expandCapabilityName('host:notifications')).toEqual([
      'host:notifications.show',
    ]);
  });

  test('accepts contract module and method ids', () => {
    expect(expandCapabilityName('host:service')).toEqual([
      'service.call',
      'service.tell',
      'service.on',
    ]);
    expect(expandCapabilityName('host:service.call')).toEqual(['service.call']);
    // The contract method is `writeText`; the capability name is `.write`.
    expect(expandCapabilityName('host:clipboard.writeText')).toEqual([
      'host:clipboard.write',
    ]);
  });

  test('maps the legacy actor.call alias to service.call', () => {
    expect(expandCapabilityName('actor.call')).toEqual(['service.call']);
  });

  test('does not treat ui.render as a Front capability', () => {
    expect(expandCapabilityName('ui.render')).toEqual([]);
    expect(expandCapabilityName('unknown:thing')).toEqual([]);
  });

  test('detects declared capabilities through method and module forms', () => {
    expect(hasDeclaredCapability(['actor.call'], 'service')).toBe(true);
    expect(hasDeclaredCapability(['service'], 'service.call')).toBe(true);
    expect(hasDeclaredCapability(['service.call'], 'service')).toBe(true);
    expect(hasDeclaredCapability(['host:clipboard'], 'host:clipboard.write')).toBe(
      true
    );
    expect(
      hasDeclaredCapability(['host:clipboard.write'], 'host:clipboard')
    ).toBe(true);
    expect(hasDeclaredCapability(['ui.render'], 'service')).toBe(false);
    expect(hasDeclaredCapability([], 'service')).toBe(false);
    expect(hasDeclaredCapability(['service.on'], 'service.call')).toBe(false);
    expect(hasDeclaredCapability(['service.tell'], 'service.call')).toBe(false);
    expect(hasDeclaredCapability(['service.call'], 'service.call')).toBe(true);
  });
});

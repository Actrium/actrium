import { describe, expect, it, vi } from 'vitest';
import type { ActrRef as NativeActrRef } from '../index';
import { ActrRef } from './ref';

describe('ActrRef wrapper', () => {
  it('forwards dependency aliases to the native runtime', async () => {
    const resolved = {
      manufacturer: 'acme',
      name: 'EchoService',
      version: '1.0.0',
    };
    const resolveDependency = vi.fn().mockResolvedValue(resolved);
    const wrapper = new ActrRef({
      resolveDependency,
    } as unknown as NativeActrRef);

    await expect(wrapper.resolveDependency('echo')).resolves.toEqual(resolved);
    expect(resolveDependency).toHaveBeenCalledOnce();
    expect(resolveDependency).toHaveBeenCalledWith('echo');
  });
});

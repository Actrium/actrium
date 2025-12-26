import { describe, expect, it } from 'bun:test';
import vm from 'node:vm';
import { DefaultProvider } from '../default/DefaultProvider';
import { VMProvider } from '../providers/VMProvider';
import { SandboxType } from '../types/provider';

describe('DefaultProvider', () => {
  describe.skipIf(!vm)('Node.js/Bun environment', () => {
    it('should auto-select VMProvider in Node.js environment', () => {
      const provider = DefaultProvider.create();

      // In Node.js/Bun, should use VMProvider
      expect(provider).toBeInstanceOf(VMProvider);
    });

    it('should respect explicit vm sandbox mode', () => {
      const provider = DefaultProvider.create({ sandbox: SandboxType.VM });

      expect(provider).toBeInstanceOf(VMProvider);
    });

    it('should pass timeout to provider', () => {
      const provider = DefaultProvider.create({ timeout: 500 });
      const runtime = provider.createRuntime();
      const context = runtime.createContext();

      // Should timeout on infinite loop
      let threw = false;
      try {
        context.eval('for(;;){}');
      } catch {
        threw = true;
      }

      expect(threw).toBe(true);
      context.dispose();
      runtime.dispose();
    });
  });
});

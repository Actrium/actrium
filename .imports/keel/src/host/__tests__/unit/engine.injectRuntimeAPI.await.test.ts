import { describe, expect, it } from 'bun:test';
import { Engine } from '../../engine';
import type { JSEngineProvider } from '../../engine/types';

/**
 * Regression test: injectRuntimeAPI must await evalCode(RUNTIME_HELPERS_CODE)
 * so that async-only providers don't race (helpers not available yet).
 */
describe('Engine - injectRuntimeAPI awaits runtime helpers', () => {
  it('should make __useHostEvent available before executing guest bundle (async provider)', async () => {
    const globals = new Map<string, unknown>();
    const sandboxGlobalThis: Record<string, unknown> = {};
    const silentConsole = { log() {}, warn() {}, error() {}, debug() {}, info() {} };

    const provider: JSEngineProvider = {
      createRuntime() {
        return {
          createContext() {
            const execute = (code: string): unknown => {
              const globalNames = Array.from(globals.keys());
              const globalValues = Array.from(globals.values());
              const wrapped = `"use strict"; var globalThis = arguments[arguments.length-1]; var console = arguments[arguments.length-2]; ${code}`;
              const fn = new Function(...globalNames, wrapped);
              return fn(...globalValues, silentConsole, sandboxGlobalThis);
            };

            return {
              eval() {
                throw new Error('sync eval should not be used in this test');
              },
              async evalAsync(code: string) {
                // Yield to simulate async worker/WASM
                await Promise.resolve();
                return execute(code);
              },
              setGlobal(name: string, value: unknown) {
                globals.set(name, value);
                sandboxGlobalThis[name] = value;
              },
              getGlobal(name: string) {
                if (name in sandboxGlobalThis) return sandboxGlobalThis[name];
                return globals.get(name);
              },
              dispose() {
                globals.clear();
                for (const k of Object.keys(sandboxGlobalThis)) delete sandboxGlobalThis[k];
              },
            };
          },
          dispose() {},
        };
      },
    };

    const engine = new Engine({ quickjs: provider, debug: false });

    // Guest bundle asserts runtime helpers exist at execution time.
    await engine.loadBundle(`
      if (typeof globalThis.__useHostEvent !== 'function') {
        throw new Error('__useHostEvent missing');
      }
      if (!globalThis.__callbacks || typeof globalThis.__registerCallback !== 'function') {
        throw new Error('__callbacks missing');
      }
      globalThis.__OK = true;
    `);

    expect(engine.context?.getGlobal('__OK')).toBe(true);
    engine.destroy();
  });
});

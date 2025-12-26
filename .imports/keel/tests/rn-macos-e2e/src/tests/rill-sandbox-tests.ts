import { Platform } from 'react-native';
import { registerTest } from '../runner/registry';
import { expect } from '../runner/expect';

// These tests run inside the real RN app runtime.
// They validate that native JSI bindings are injected (global.__*SandboxJSI) and functional.

interface SandboxContext {
  eval(code: string): unknown;
  setGlobal(name: string, value: unknown): void;
  getGlobal(name: string): unknown;
  dispose(): void;
  isDisposed?: boolean;
}

interface SandboxRuntime {
  createContext(): SandboxContext;
  dispose(): void;
}

interface SandboxModule {
  createRuntime(options?: { timeout?: number }): SandboxRuntime;
  isAvailable(): boolean;
}

declare global {
  // eslint-disable-next-line no-var
  var __JSCSandboxJSI: SandboxModule | undefined;
  // eslint-disable-next-line no-var
  var __QuickJSSandboxJSI: SandboxModule | undefined;
}

export type SandboxTarget = 'quickjs' | 'jsc' | 'auto';

function getModule(target: SandboxTarget): SandboxModule {
  const mod =
    target === 'jsc'
      ? global.__JSCSandboxJSI
      : target === 'quickjs'
        ? global.__QuickJSSandboxJSI
        : global.__JSCSandboxJSI ?? global.__QuickJSSandboxJSI;

  if (!mod) {
    throw new Error(
      `JSI module not injected. target=${target} (expected global.__JSCSandboxJSI or global.__QuickJSSandboxJSI)`
    );
  }
  return mod;
}

export function registerRillSandboxTests(target: SandboxTarget) {
  registerTest({
    id: 'env/basic',
    name: 'Environment: platform info',
    tags: ['env'],
    run() {
      // Basic sanity; we are in RN runtime.
      expect(typeof Platform.OS).toBe('string');
    },
  });

  registerTest({
    id: 'sandbox/detect',
    name: 'Sandbox: detect injected JSI globals',
    tags: ['sandbox'],
    run() {
      const hasJSC = typeof global.__JSCSandboxJSI !== 'undefined';
      const hasQuickJS = typeof global.__QuickJSSandboxJSI !== 'undefined';

      if (target === 'jsc') {
        expect(hasJSC).toBe(true);
      } else if (target === 'quickjs') {
        expect(hasQuickJS).toBe(true);
      } else {
        // auto: at least one should exist
        expect(hasJSC || hasQuickJS).toBe(true);
      }
    },
  });

  registerTest({
    id: 'sandbox/smoke-eval',
    name: 'Sandbox: create runtime/context and eval (smoke)',
    tags: ['sandbox'],
    run() {
      const mod = getModule(target);

      expect(typeof mod.isAvailable).toBe('function');
      expect(mod.isAvailable()).toBe(true);

      const runtime = mod.createRuntime({ timeout: 1000 });
      const ctx = runtime.createContext();
      const v = ctx.eval('1 + 2');
      expect(v).toBe(3);
      ctx.dispose();
      runtime.dispose();
    },
  });

  // ============================================
  // Callback bidirectional tests (host ↔ guest)
  // ============================================

  registerTest({
    id: 'callback/host-to-guest',
    name: 'Callback: host function callable from guest',
    tags: ['callback'],
    run() {
      const mod = getModule(target);
      const runtime = mod.createRuntime({ timeout: 5000 });
      const ctx = runtime.createContext();

      let called = false;
      let receivedArg: unknown;

      ctx.setGlobal('hostFn', (arg: unknown) => {
        called = true;
        receivedArg = arg;
        return 'host-response';
      });

      const result = ctx.eval('hostFn("hello-from-guest")');

      expect(called).toBe(true);
      expect(receivedArg).toBe('hello-from-guest');
      expect(result).toBe('host-response');

      ctx.dispose();
      runtime.dispose();
    },
  });

  registerTest({
    id: 'callback/guest-to-host',
    name: 'Callback: guest function callable from host',
    tags: ['callback'],
    run() {
      const mod = getModule(target);
      const runtime = mod.createRuntime({ timeout: 5000 });
      const ctx = runtime.createContext();

      ctx.eval('function guestFn(x) { return x * 2; }');
      const guestFn = ctx.getGlobal('guestFn') as (x: number) => number;

      expect(typeof guestFn).toBe('function');
      const result = guestFn(21);
      expect(result).toBe(42);

      ctx.dispose();
      runtime.dispose();
    },
  });

  registerTest({
    id: 'callback/round-trip',
    name: 'Callback: round-trip host → guest → host',
    tags: ['callback'],
    run() {
      const mod = getModule(target);
      const runtime = mod.createRuntime({ timeout: 5000 });
      const ctx = runtime.createContext();

      const log: string[] = [];

      ctx.setGlobal('step1', () => {
        log.push('step1');
        return 'from-step1';
      });

      ctx.eval(`
        function orchestrate() {
          var r1 = step1();
          return 'guest-saw:' + r1;
        }
      `);

      const orchestrate = ctx.getGlobal('orchestrate') as () => string;
      const result = orchestrate();

      expect(log).toEqual(['step1']);
      expect(result).toBe('guest-saw:from-step1');

      ctx.dispose();
      runtime.dispose();
    },
  });

  // ============================================
  // Complex type serialization tests
  // ============================================

  registerTest({
    id: 'types/primitives',
    name: 'Types: primitive values round-trip',
    tags: ['types'],
    run() {
      const mod = getModule(target);
      const runtime = mod.createRuntime({ timeout: 5000 });
      const ctx = runtime.createContext();

      // Numbers
      expect(ctx.eval('42')).toBe(42);
      expect(ctx.eval('3.14')).toBe(3.14);
      expect(ctx.eval('-0')).toBe(-0);

      // Strings
      expect(ctx.eval('"hello"')).toBe('hello');
      expect(ctx.eval('""')).toBe('');

      // Booleans
      expect(ctx.eval('true')).toBe(true);
      expect(ctx.eval('false')).toBe(false);

      // Null/Undefined
      expect(ctx.eval('null')).toBe(null);
      expect(ctx.eval('undefined')).toBe(undefined);

      ctx.dispose();
      runtime.dispose();
    },
  });

  registerTest({
    id: 'types/arrays',
    name: 'Types: array serialization',
    tags: ['types'],
    run() {
      const mod = getModule(target);
      const runtime = mod.createRuntime({ timeout: 5000 });
      const ctx = runtime.createContext();

      // Arrays are returned as array-like objects (indexed properties)
      // Note: JSC sandbox serialization may not preserve native Array type
      const arr = ctx.eval('[1, 2, 3]') as Record<string, unknown>;
      expect(arr['0']).toBe(1);
      expect(arr['1']).toBe(2);
      expect(arr['2']).toBe(3);

      // Array with string
      const withString = ctx.eval('["a", "b"]') as Record<string, unknown>;
      expect(withString['0']).toBe('a');
      expect(withString['1']).toBe('b');

      ctx.dispose();
      runtime.dispose();
    },
  });

  registerTest({
    id: 'types/objects',
    name: 'Types: object serialization',
    tags: ['types'],
    run() {
      const mod = getModule(target);
      const runtime = mod.createRuntime({ timeout: 5000 });
      const ctx = runtime.createContext();

      const obj = ctx.eval('({ a: 1, b: "two", c: true })') as Record<string, unknown>;
      expect(obj.a).toBe(1);
      expect(obj.b).toBe('two');
      expect(obj.c).toBe(true);

      // Nested objects
      const nested = ctx.eval('({ outer: { inner: 42 } })') as { outer: { inner: number } };
      expect(nested.outer.inner).toBe(42);

      ctx.dispose();
      runtime.dispose();
    },
  });

  registerTest({
    id: 'types/setGlobal-complex',
    name: 'Types: setGlobal with complex objects',
    tags: ['types'],
    run() {
      const mod = getModule(target);
      const runtime = mod.createRuntime({ timeout: 5000 });
      const ctx = runtime.createContext();

      ctx.setGlobal('hostData', {
        name: 'test',
        values: [1, 2, 3],
        nested: { flag: true },
      });

      expect(ctx.eval('hostData.name')).toBe('test');
      expect(ctx.eval('hostData.values[1]')).toBe(2);
      expect(ctx.eval('hostData.nested.flag')).toBe(true);

      ctx.dispose();
      runtime.dispose();
    },
  });

  // ============================================
  // Error handling tests
  // ============================================

  registerTest({
    id: 'error/syntax-error',
    name: 'Error: syntax error throws',
    tags: ['error'],
    run() {
      const mod = getModule(target);
      const runtime = mod.createRuntime({ timeout: 5000 });
      const ctx = runtime.createContext();

      let threw = false;
      try {
        ctx.eval('function {{{ invalid');
      } catch (e) {
        threw = true;
      }

      expect(threw).toBe(true);

      ctx.dispose();
      runtime.dispose();
    },
  });

  registerTest({
    id: 'error/runtime-error',
    name: 'Error: runtime error throws',
    tags: ['error'],
    run() {
      const mod = getModule(target);
      const runtime = mod.createRuntime({ timeout: 5000 });
      const ctx = runtime.createContext();

      let threw = false;
      try {
        ctx.eval('undefinedVariable.foo');
      } catch (e) {
        threw = true;
      }

      expect(threw).toBe(true);

      ctx.dispose();
      runtime.dispose();
    },
  });

  registerTest({
    id: 'error/throw-propagates',
    name: 'Error: explicit throw propagates to host',
    tags: ['error'],
    run() {
      const mod = getModule(target);
      const runtime = mod.createRuntime({ timeout: 5000 });
      const ctx = runtime.createContext();

      let threw = false;
      let errorMessage = '';
      try {
        ctx.eval('throw new Error("custom-error")');
      } catch (e) {
        threw = true;
        errorMessage = String(e);
      }

      expect(threw).toBe(true);
      expect(errorMessage.includes('custom-error')).toBe(true);

      ctx.dispose();
      runtime.dispose();
    },
  });

  // ============================================
  // Multi-context isolation tests
  // ============================================

  registerTest({
    id: 'isolation/separate-globals',
    name: 'Isolation: contexts have separate globals',
    tags: ['isolation'],
    run() {
      const mod = getModule(target);
      const runtime = mod.createRuntime({ timeout: 5000 });

      const ctx1 = runtime.createContext();
      const ctx2 = runtime.createContext();

      ctx1.eval('var sharedName = "ctx1"');
      ctx2.eval('var sharedName = "ctx2"');

      expect(ctx1.eval('sharedName')).toBe('ctx1');
      expect(ctx2.eval('sharedName')).toBe('ctx2');

      ctx1.dispose();
      ctx2.dispose();
      runtime.dispose();
    },
  });

  registerTest({
    id: 'isolation/no-cross-pollution',
    name: 'Isolation: no cross-context pollution',
    tags: ['isolation'],
    run() {
      const mod = getModule(target);
      const runtime = mod.createRuntime({ timeout: 5000 });

      const ctx1 = runtime.createContext();
      const ctx2 = runtime.createContext();

      ctx1.eval('var onlyInCtx1 = 123');

      let threw = false;
      try {
        ctx2.eval('onlyInCtx1');
      } catch (e) {
        threw = true;
      }

      expect(threw).toBe(true);

      ctx1.dispose();
      ctx2.dispose();
      runtime.dispose();
    },
  });

  // ============================================
  // Memory / dispose tests
  // ============================================

  registerTest({
    id: 'memory/dispose-context',
    name: 'Memory: disposed context rejects operations',
    tags: ['memory'],
    run() {
      const mod = getModule(target);
      const runtime = mod.createRuntime({ timeout: 5000 });
      const ctx = runtime.createContext();

      ctx.eval('var x = 1');
      ctx.dispose();

      let threw = false;
      try {
        ctx.eval('x + 1');
      } catch (e) {
        threw = true;
      }

      expect(threw).toBe(true);

      runtime.dispose();
    },
  });

  registerTest({
    id: 'memory/multiple-create-dispose',
    name: 'Memory: multiple create/dispose cycles',
    tags: ['memory'],
    run() {
      const mod = getModule(target);
      const runtime = mod.createRuntime({ timeout: 5000 });

      for (let i = 0; i < 10; i++) {
        const ctx = runtime.createContext();
        ctx.eval(`var iteration = ${i}`);
        expect(ctx.eval('iteration')).toBe(i);
        ctx.dispose();
      }

      runtime.dispose();
    },
  });

  // ============================================
  // Performance tests
  // ============================================

  registerTest({
    id: 'perf/many-evals',
    name: 'Perf: 100 sequential evals',
    tags: ['perf'],
    run() {
      const mod = getModule(target);
      const runtime = mod.createRuntime({ timeout: 10000 });
      const ctx = runtime.createContext();

      const start = Date.now();
      for (let i = 0; i < 100; i++) {
        ctx.eval(`${i} + 1`);
      }
      const elapsed = Date.now() - start;

      // Should complete in reasonable time (< 1s for 100 evals)
      expect(elapsed < 1000).toBe(true);

      ctx.dispose();
      runtime.dispose();
    },
  });

  registerTest({
    id: 'perf/callback-overhead',
    name: 'Perf: 50 host function calls',
    tags: ['perf'],
    run() {
      const mod = getModule(target);
      const runtime = mod.createRuntime({ timeout: 10000 });
      const ctx = runtime.createContext();

      let callCount = 0;
      ctx.setGlobal('increment', () => {
        callCount++;
      });

      ctx.eval('for (var i = 0; i < 50; i++) { increment(); }');

      expect(callCount).toBe(50);

      ctx.dispose();
      runtime.dispose();
    },
  });

  // ============================================
  // React Element simulation tests (critical for rill)
  // ============================================

  registerTest({
    id: 'react/guest-fn-returns-element',
    name: 'React: guest function returns element-like object',
    tags: ['react', 'critical'],
    run() {
      const mod = getModule(target);
      const runtime = mod.createRuntime({ timeout: 5000 });
      const ctx = runtime.createContext();

      // Simulate a React component that returns an element
      ctx.eval(`
        function MyComponent(props) {
          return {
            __rillTypeMarker: '__rill_react_element__',
            type: 'View',
            props: { style: { flex: 1 }, children: 'Hello' }
          };
        }
      `);

      const MyComponent = ctx.getGlobal('MyComponent') as (props: object) => object;
      expect(typeof MyComponent).toBe('function');

      // Call the component and check the returned element
      const element = MyComponent({}) as Record<string, unknown>;
      expect(element.__rillTypeMarker).toBe('__rill_react_element__');
      expect(element.type).toBe('View');
      expect((element.props as Record<string, unknown>).children).toBe('Hello');

      ctx.dispose();
      runtime.dispose();
    },
  });

  registerTest({
    id: 'react/nested-elements',
    name: 'React: guest function returns nested elements',
    tags: ['react', 'critical'],
    run() {
      const mod = getModule(target);
      const runtime = mod.createRuntime({ timeout: 5000 });
      const ctx = runtime.createContext();

      // Simulate nested React elements (like Panel.Left wrapping LeftPanel)
      ctx.eval(`
        function UnifiedApp() {
          return {
            __rillTypeMarker: '__rill_react_element__',
            type: 'View',
            props: {
              style: { flex: 1 },
              children: [
                {
                  __rillTypeMarker: '__rill_react_element__',
                  type: 'PanelMarker',
                  props: { panelId: 'left', children: { type: 'Text', props: { children: 'Left' } } }
                },
                {
                  __rillTypeMarker: '__rill_react_element__',
                  type: 'PanelMarker',
                  props: { panelId: 'right', children: { type: 'Text', props: { children: 'Right' } } }
                }
              ]
            }
          };
        }
      `);

      const UnifiedApp = ctx.getGlobal('UnifiedApp') as () => object;
      const element = UnifiedApp() as Record<string, unknown>;

      expect(element.__rillTypeMarker).toBe('__rill_react_element__');
      expect(element.type).toBe('View');

      const children = (element.props as Record<string, unknown>).children as Array<Record<string, unknown>>;
      expect(children.length).toBe(2);
      expect(children[0].type).toBe('PanelMarker');
      expect((children[0].props as Record<string, unknown>).panelId).toBe('left');
      expect(children[1].type).toBe('PanelMarker');
      expect((children[1].props as Record<string, unknown>).panelId).toBe('right');

      ctx.dispose();
      runtime.dispose();
    },
  });

  registerTest({
    id: 'react/fn-type-preserved',
    name: 'React: element with function type is callable from host',
    tags: ['react', 'critical'],
    run() {
      const mod = getModule(target);
      const runtime = mod.createRuntime({ timeout: 5000 });
      const ctx = runtime.createContext();

      // Simulate React.createElement(UnifiedApp) - type is a function
      ctx.eval(`
        function UnifiedApp(props) {
          return {
            __rillTypeMarker: '__rill_react_element__',
            type: 'View',
            props: { message: 'rendered with ' + (props.name || 'default') }
          };
        }

        var element = {
          __rillTypeMarker: '__rill_react_element__',
          type: UnifiedApp,
          props: { name: 'test' }
        };
      `);

      const element = ctx.getGlobal('element') as Record<string, unknown>;
      expect(element.__rillTypeMarker).toBe('__rill_react_element__');

      // The type should be a callable function
      const typeFn = element.type as (props: object) => object;
      expect(typeof typeFn).toBe('function');

      // Call the function to get the rendered element
      const rendered = typeFn({ name: 'host-call' }) as Record<string, unknown>;
      expect(rendered.__rillTypeMarker).toBe('__rill_react_element__');
      expect(rendered.type).toBe('View');
      expect((rendered.props as Record<string, unknown>).message).toBe('rendered with host-call');

      ctx.dispose();
      runtime.dispose();
    },
  });
}

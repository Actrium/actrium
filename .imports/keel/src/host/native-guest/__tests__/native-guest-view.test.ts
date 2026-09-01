import { describe, expect, it } from 'bun:test';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { defineKeelContract } from '../../../contract';
import { createNativeGuestEngine } from '../native-guest-view';

// A render-only native guest (renders View > [Text, View > Text]).
const UI_GUEST = readFileSync(join(import.meta.dir, 'fixtures/ui-guest.wasm'));

// An empty host-capability contract (ui-guest makes no host:* calls).
const emptyContract = defineKeelContract({ version: '1', hostModules: {}, guestExports: {} });

describe('createNativeGuestEngine — native guest as an EngineViewEngine', () => {
  it('drives a native guest through the EngineViewEngine surface (like useEngineView)', async () => {
    const engine = createNativeGuestEngine({
      wasmBytes: UI_GUEST,
      contract: emptyContract,
      hostModules: {},
      // biome-ignore lint/suspicious/noExplicitAny: opaque materializers, irrelevant here.
      components: { View: 'View' as any, Text: 'Text' as any },
    });

    let updates = 0;
    const unsub = engine.on('update', () => {
      updates++;
    });

    expect(engine.isLoaded).toBe(false);

    // useEngineView's sequence: createReceiver() then loadBundle().
    engine.createReceiver();
    await engine.loadBundle();

    expect(engine.isLoaded).toBe(true);
    expect(updates).toBeGreaterThan(0); // the guest's render batch fired an 'update'

    // Same receiver surface as the JS Engine — content is materialized.
    const tree = engine.getReceiver()?.getComponentTree();
    expect(tree?.type).toBe('View');
    // '__TEXT__' + props.text is the shape renderNode() renders as Text
    // children; asserting the type (not just props.text in the node map)
    // catches a guest emitting text nodes the receiver would not render.
    expect(tree?.children[0].type).toBe('__TEXT__');
    expect(tree?.children[0].props.text).toBe('hello from rust');

    unsub();
    engine.destroy();
    expect(engine.isDestroyed).toBe(true);
  });

  it('forwards events through fire-and-forget and acknowledged surfaces', async () => {
    const engine = createNativeGuestEngine({
      wasmBytes: UI_GUEST,
      contract: emptyContract,
      hostModules: {},
      // biome-ignore lint/suspicious/noExplicitAny: opaque materializers, irrelevant here.
      components: { View: 'View' as any, Text: 'Text' as any },
    });
    engine.createReceiver();
    await engine.loadBundle();
    // ui-guest has no keel_on_event handler; sendEvent must be a safe no-op.
    expect(() => engine.sendEvent('key', 'a')).not.toThrow();
    await expect(engine.sendEventAndWait('key', 'b')).resolves.toBeUndefined();
  });

  it('exposes raw guest exports for diagnostics only after load', async () => {
    const engine = createNativeGuestEngine({
      wasmBytes: UI_GUEST,
      contract: emptyContract,
      hostModules: {},
      // biome-ignore lint/suspicious/noExplicitAny: opaque materializers, irrelevant here.
      components: { View: 'View' as any, Text: 'Text' as any },
    });

    // Pre-load: undefined (not a throw) — mirrors guestAbiVersion's contract.
    expect(engine.exports).toBeUndefined();

    engine.createReceiver();
    await engine.loadBundle();

    // Post-load: the guest's real export surface is visible (memory + ABI exports).
    expect(engine.exports).toBeDefined();
    expect(engine.exports?.memory).toBeInstanceOf(WebAssembly.Memory);
    expect(typeof engine.exports?.keel_alloc).toBe('function');
    engine.destroy();
  });
});

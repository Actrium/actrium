/**
 * End-to-end tests for the WIP binary OP-BATCH wire on the native-guest render
 * channel (`contracts/op-batch-wire.json`, KEEL magic).
 *
 * The runtime gate is a host→guest push: when the host's `binaryOpBatch`
 * option is on it calls the guest's optional `keel_wire_caps(bit0)` export
 * before `keel_init`; the guest then emits the render batch as a binary frame
 * that the host forks on the KEEL magic and decodes via the streaming wire
 * decoder. Every other host×guest combination degrades to JSON.
 *
 * Route-detection trick: with the binary encoder compiled in (the default) but
 * the gate NOT enabled, the guest SDK logs one `[info] binary op-batch encoder
 * present but host did not enable binary op-batch` line, and on a failed binary
 * encode it logs `[warn] op-batch binary encode failed`. So "tree correct AND
 * neither log present" proves the batch really travelled the binary wire,
 * without the test reaching into the channel.
 */
import { describe, expect, it } from 'bun:test';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { ComponentRegistry } from '../../registry';
import { Receiver } from '../../receiver';
import { NativeGuestHost } from '../native-guest-host';

// Same deterministic tree, three builds:
//   ui-guest        — the promoted default: binary-capable, exports keel_wire_caps;
//   ui-binary-guest — same, kept as an explicit binary fixture;
//   ui-guest-json   — a pre-promotion, JSON-only build (--no-default-features),
//                     with NO keel_wire_caps export, for the back-compat path.
const UI_GUEST = readFileSync(join(import.meta.dir, 'fixtures/ui-guest.wasm'));
const UI_BINARY_GUEST = readFileSync(join(import.meta.dir, 'fixtures/ui-binary-guest.wasm'));
const UI_GUEST_JSON = readFileSync(join(import.meta.dir, 'fixtures/ui-guest-json.wasm'));

const GATE_OFF_LOG = 'host did not enable binary op-batch';
const ENCODE_FAILED_LOG = 'op-batch binary encode failed';

async function loadGuest(wasm: Buffer, binaryOpBatch: boolean) {
  const registry = new ComponentRegistry();
  // biome-ignore lint/suspicious/noExplicitAny: registry stores an opaque materializer; irrelevant here.
  registry.register('View', 'View' as any);
  // biome-ignore lint/suspicious/noExplicitAny: registry stores an opaque materializer; irrelevant here.
  registry.register('Text', 'Text' as any);
  const receiver = new Receiver(
    registry,
    () => {},
    () => {}
  );
  const logs: string[] = [];
  const host = new NativeGuestHost({
    dispatch: {},
    binaryOpBatch,
    onLog: (m) => logs.push(m),
    onRenderBatch: (batch) => receiver.applyBatch(batch),
  });
  await host.load(wasm);
  return { receiver, logs };
}

function expectUiGuestTree(receiver: Receiver) {
  const tree = receiver.getComponentTree();
  expect(tree?.type).toBe('View');
  expect(tree?.children).toHaveLength(2);
  expect(tree?.children[0].type).toBe('__TEXT__');
  expect(tree?.children[0].props.text).toBe('hello from rust');
  expect(tree?.children[1].type).toBe('View');
  expect(tree?.children[1].children[0].props.text).toBe('nested');
}

describe('NativeGuestHost — binary op-batch wire (keel_wire_caps gate)', () => {
  it('materializes the batch over the binary wire when the host enables it', async () => {
    const { receiver, logs } = await loadGuest(UI_BINARY_GUEST, true);
    expectUiGuestTree(receiver);
    // Neither degrade log fired => the gate was on and the encode succeeded,
    // i.e. the frame that produced this tree was the binary KEEL wire.
    expect(logs.find((m) => m.includes(GATE_OFF_LOG))).toBeUndefined();
    expect(logs.find((m) => m.includes(ENCODE_FAILED_LOG))).toBeUndefined();
  });

  it('produces the identical tree the JSON ui-guest produces', async () => {
    const binary = await loadGuest(UI_BINARY_GUEST, true);
    const json = await loadGuest(UI_GUEST, false);
    expect(binary.receiver.getComponentTree()).toEqual(json.receiver.getComponentTree());
  });

  it('degrades to JSON when the host leaves the gate off (and says so once)', async () => {
    const { receiver, logs } = await loadGuest(UI_BINARY_GUEST, false);
    expectUiGuestTree(receiver);
    // The diagnostic that makes an unwired gate loud instead of silent.
    expect(logs.filter((m) => m.includes(GATE_OFF_LOG))).toHaveLength(1);
  });

  it('skips the caps push for a pre-promotion guest without the export (JSON-only build)', async () => {
    // binaryOpBatch on (now the default), but ui-guest-json.wasm is built
    // --no-default-features so it has no keel_wire_caps export: the host must
    // skip the call and the JSON batch must still materialize (back-compat).
    const { receiver } = await loadGuest(UI_GUEST_JSON, true);
    expectUiGuestTree(receiver);
  });

  it('the promoted default guest (ui-guest) emits binary under the default host option', async () => {
    // ui-guest.wasm is now binary-capable by default; a host with binaryOpBatch
    // at its new default (true) negotiates binary and decodes the same tree.
    const { receiver, logs } = await loadGuest(UI_GUEST, true);
    expectUiGuestTree(receiver);
    expect(logs.find((m) => m.includes(GATE_OFF_LOG))).toBeUndefined();
    expect(logs.find((m) => m.includes(ENCODE_FAILED_LOG))).toBeUndefined();
  });
});

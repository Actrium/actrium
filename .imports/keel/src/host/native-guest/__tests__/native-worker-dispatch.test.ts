import { describe, expect, it } from 'bun:test';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { NativeWorkerDispatch } from '../native-worker-dispatch';
import type { NativeWorkerToMainMessage } from '../native-worker-protocol';

const EVENT_GUEST = readFileSync(join(import.meta.dir, 'fixtures/runaway-event.wasm'));

describe('NativeWorkerDispatch — guest run-state attribution', () => {
  it('attributes a later event to the event turn instead of the completed load turn', async () => {
    const messages: NativeWorkerToMainMessage[] = [];
    let notify: (() => void) | undefined;
    const dispatch = new NativeWorkerDispatch({
      post: (message) => {
        messages.push(message);
        notify?.();
        notify = undefined;
      },
    });
    const waitFor = async (
      predicate: (message: NativeWorkerToMainMessage) => boolean
    ): Promise<void> => {
      while (!messages.some(predicate)) {
        await new Promise<void>((resolve) => {
          notify = resolve;
        });
      }
    };

    dispatch.handle({ type: 'init', wasmBytes: EVENT_GUEST.slice().buffer });
    dispatch.handle({ type: 'load', turnId: 11 });
    await waitFor((message) => message.type === 'loaded' && message.turnId === 11);

    const eventMessageStart = messages.length;
    dispatch.handle({ type: 'event', turnId: 22, name: 'safe' });
    await waitFor((message) => message.type === 'turnDone' && message.turnId === 22);
    const eventMessages = messages.slice(eventMessageStart);

    expect(
      eventMessages
        .filter((message) => message.type === 'guestState' && message.turnId === 22)
        .map((message) => message.running)
    ).toEqual([true, false]);
    expect(
      eventMessages.filter((message) => message.type === 'guestState' && message.turnId === 11)
    ).toHaveLength(0);
  });
});

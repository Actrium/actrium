import { expect, test } from 'bun:test';
import { NativeServiceStreams } from './service-streams';

test('routes chunks only to the tab and registration that own the stream', async () => {
  const seen: string[] = [];
  let listener: ((event: unknown) => void) | undefined;
  let attempt = 0;
  const streams = new NativeServiceStreams('one', {
    subscribeServiceStream: async () => String(++attempt),
    unsubscribeServiceEvents: () => {},
  }, (handler) => {
    listener = handler;
    return () => { listener = undefined; };
  });
  const unsubscribe = streams.subscribe((chunk) => seen.push(chunk.sequence));
  const event = { tabId: 'one', streamId: 'reply', subscriptionId: '1', sequence: '1', payloadBase64: '' };
  await streams.register('reply');
  listener?.({ ...event, tabId: 'two' });
  listener?.(event);
  streams.unregister('reply');
  await streams.register('reply');
  listener?.(event);
  listener?.({ ...event, subscriptionId: '2', sequence: '2' });
  expect(seen).toEqual(['1', '2']);
  unsubscribe();
  streams.dispose();
});

test('unregisters a native subscription that resolves after the bridge closes', async () => {
  let resolve: ((id: string) => void) | undefined;
  const released: string[] = [];
  const streams = new NativeServiceStreams('one', {
    subscribeServiceStream: () => new Promise((done) => { resolve = done; }),
    unsubscribeServiceEvents: (_tabId, id) => { released.push(id); },
  }, () => () => {});
  const registration = streams.register('reply');
  streams.dispose();
  resolve?.('late');
  await expect(registration).rejects.toThrow('cancelled');
  expect(released).toEqual(['late']);
});

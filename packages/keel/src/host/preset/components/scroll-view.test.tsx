import { expect, test } from 'bun:test';
import React from 'react';
import { ScrollView as NativeScrollView, Text as NativeText, View as NativeView } from 'react-native';
import { act, create } from 'react-test-renderer';
import { ScrollView } from './scroll-view';
import { InitialScrollContent } from './initial-scroll-content';
import type { ScrollRequest } from '../../../shared/style-types';

const viewportLayout = { nativeEvent: { layout: { x: 0, y: 0, width: 600, height: 300 } } };

test('executes each valid request once and preserves the forwarded native ref', async () => {
  const calls: unknown[] = [];
  const nativeView = {
    scrollToEnd: (options: unknown) => { calls.push(['end', options]); },
    scrollTo: (options: unknown) => { calls.push(['offset', options]); },
  };
  const ref = React.createRef<React.ComponentRef<typeof ScrollView>>();
  let renderer: ReturnType<typeof create> | undefined;
  try {
    await act(async () => {
      renderer = create(<ScrollView ref={ref} scrollRequest={{ id: 1, target: 'end' }} />, {
        createNodeMock: () => nativeView,
      });
    });
    expect(ref.current).toBe(nativeView);
    await act(async () => {
      renderer!.update(<ScrollView ref={ref} scrollRequest={{ id: 1, target: 'end' }} />);
    });
    await act(async () => {
      renderer!.update(<ScrollView ref={ref} scrollRequest={{ id: 2, target: 120, animated: true }} />);
    });
    await act(async () => {
      renderer!.update(<ScrollView ref={ref} scrollRequest={{ id: 1, target: 'end' }} />);
    });
    await act(async () => {
      renderer!.update(<ScrollView ref={ref} scrollRequest={{ id: 3, target: Infinity }} />);
    });
    expect(calls).toEqual([
      ['end', { animated: false }],
      ['offset', { y: 120, animated: true }],
    ]);
  } finally {
    await act(async () => { renderer?.unmount(); });
  }
});

test.each(['object', 'callback'])('updates the %s ref when the native view is replaced', async (kind) => {
  const first = { scrollToEnd: () => {} };
  const replacementCalls: unknown[] = [];
  const replacement = { scrollToEnd: (options: unknown) => { replacementCalls.push(options); } };
  const ref = React.createRef<React.ComponentRef<typeof ScrollView>>();
  const outerRef = kind === 'object' ? ref : (view: React.ComponentRef<typeof ScrollView> | null) => {
    ref.current = view;
  };
  let renderer: ReturnType<typeof create> | undefined;
  try {
    await act(async () => {
      renderer = create(<ScrollView ref={outerRef} />, { createNodeMock: () => first });
    });
    expect(ref.current).toBe(first);
    const nativeRef = renderer!.root.findByType(NativeScrollView).props.ref;
    const attach = (view: typeof replacement | null) => {
      if (typeof nativeRef === 'function') nativeRef(view);
      else nativeRef.current = view;
    };
    await act(async () => { attach(null); });
    expect(ref.current).toBeNull();
    await act(async () => { attach(replacement); });
    expect(ref.current).toBe(replacement);
    await act(async () => {
      renderer!.update(<ScrollView ref={outerRef} scrollRequest={{ id: 1, target: 'end' }} />);
    });
    expect(replacementCalls).toEqual([{ animated: false }]);
  } finally {
    await act(async () => { renderer?.unmount(); });
  }
});

test('ignores numeric requests on a horizontal ScrollView and still supports the end target', async () => {
  const calls: unknown[] = [];
  const nativeView = {
    scrollTo: (options: unknown) => { calls.push(['offset', options]); },
    scrollToEnd: (options: unknown) => { calls.push(['end', options]); },
  };
  let renderer: ReturnType<typeof create> | undefined;
  try {
    await act(async () => {
      renderer = create(<ScrollView horizontal scrollRequest={{ id: 1, target: 120 }} />, {
        createNodeMock: () => nativeView,
      });
    });
    await act(async () => {
      renderer!.update(<ScrollView horizontal scrollRequest={{ id: 2, target: 'end' }} />);
    });
    expect(calls).toEqual([['end', { animated: false }]]);
  } finally {
    await act(async () => { renderer?.unmount(); });
  }
});

function mockAnimationFrames() {
  const originalRequest = globalThis.requestAnimationFrame;
  const originalCancel = globalThis.cancelAnimationFrame;
  const frames = new Map<number, FrameRequestCallback>();
  let frameId = 0;
  globalThis.requestAnimationFrame = (callback) => {
    frames.set(++frameId, callback);
    return frameId;
  };
  globalThis.cancelAnimationFrame = (id) => { frames.delete(id); };
  const flushFrame = () => {
    const queued = [...frames.values()];
    frames.clear();
    for (const callback of queued) callback(0);
  };
  return {
    flushFrame,
    restore() {
      globalThis.requestAnimationFrame = originalRequest;
      globalThis.cancelAnimationFrame = originalCancel;
    },
  };
}

test('the end offset is applied on the first visible commit, after delayed content lays out', async () => {
  const { flushFrame, restore } = mockAnimationFrames();
  const completed: unknown[] = [];
  const tree = (ready: boolean) => (
    <ScrollView initialScrollPosition="end" testID="conversation"
      onContentSizeChange={() => {}}
      onInitialScrollComplete={(offset) => completed.push(offset)}>
      <InitialScrollContent>{ready ? <NativeText>Long reply</NativeText> : null}</InitialScrollContent>
    </ScrollView>
  );
  let renderer: ReturnType<typeof create> | undefined;
  const nativeScroll = () => renderer!.root.findByType(NativeScrollView);
  try {
    await act(async () => { renderer = create(tree(false)); });
    await act(async () => {
      nativeScroll().props.onLayout(viewportLayout);
      nativeScroll().props.onContentSizeChange(600, 100);
      flushFrame();
    });
    expect(nativeScroll().props.style.at(-1)).toEqual({ opacity: 0 });
    expect(completed).toEqual([]);

    await act(async () => { renderer!.update(tree(true)); });
    await act(async () => {
      const content = renderer!.root.findAllByType(NativeView).find((node) => !node.props.testID && node.props.onLayout)!;
      content.props.onLayout(viewportLayout);
      nativeScroll().props.onContentSizeChange(600, 1000);
      flushFrame();
    });
    expect(nativeScroll().props.style).toBeUndefined();
    expect(nativeScroll().props.contentOffset).toEqual({ x: 0, y: 700 });
    expect(completed).toEqual([{ x: 0, y: 700 }]);

    await act(async () => {
      nativeScroll().props.onContentSizeChange(600, 1400);
      flushFrame();
    });
    expect(nativeScroll().props.contentOffset).toEqual({ x: 0, y: 700 });
    expect(completed).toHaveLength(1);
  } finally {
    await act(async () => { renderer?.unmount(); });
    restore();
  }
});

test('reveals the list using the measured layout when a tracked child stays pending', async () => {
  const { flushFrame, restore } = mockAnimationFrames();
  const completed: unknown[] = [];
  let renderer: ReturnType<typeof create> | undefined;
  try {
    await act(async () => {
      renderer = create(
        <ScrollView initialScrollPosition="end" onInitialScrollComplete={(offset) => completed.push(offset)}>
          <InitialScrollContent>{null}</InitialScrollContent>
        </ScrollView>
      );
    });
    const nativeScroll = () => renderer!.root.findByType(NativeScrollView);
    await act(async () => {
      nativeScroll().props.onLayout(viewportLayout);
      nativeScroll().props.onContentSizeChange(600, 1000);
      flushFrame();
    });
    expect(nativeScroll().props.pointerEvents).toBe('none');

    await act(async () => { await Bun.sleep(1100); });
    expect(nativeScroll().props.pointerEvents).toBeUndefined();
    expect(nativeScroll().props.contentOffset).toEqual({ x: 0, y: 700 });
    expect(completed).toEqual([{ x: 0, y: 700 }]);
  } finally {
    await act(async () => { renderer?.unmount(); });
    restore();
  }
});

test('discards requests received while the initial position is pending', async () => {
  const { flushFrame, restore } = mockAnimationFrames();
  const calls: unknown[] = [];
  const nativeView = { scrollTo: (options: unknown) => { calls.push(options); } };
  const tree = (scrollRequest: ScrollRequest) => (
    <ScrollView initialScrollPosition="end" scrollRequest={scrollRequest} />
  );
  let renderer: ReturnType<typeof create> | undefined;
  try {
    await act(async () => {
      renderer = create(tree({ id: 1, target: 0 }), { createNodeMock: () => nativeView });
    });
    await act(async () => { renderer!.update(tree({ id: 2, target: 120 })); });
    const nativeScroll = renderer!.root.findByType(NativeScrollView);
    await act(async () => {
      nativeScroll.props.onLayout(viewportLayout);
      nativeScroll.props.onContentSizeChange(600, 1000);
      flushFrame();
    });
    expect(nativeScroll.props.contentOffset).toEqual({ x: 0, y: 700 });
    await act(async () => { renderer!.update(tree({ id: 2, target: 240 })); });
    expect(calls).toEqual([]);
    await act(async () => { renderer!.update(tree({ id: 3, target: 300 })); });
    expect(calls).toEqual([{ y: 300, animated: false }]);
  } finally {
    await act(async () => { renderer?.unmount(); });
    restore();
  }
});

test('keeps following successive layouts without new guest requests and cancels a queued update when disabled', async () => {
  const { flushFrame, restore } = mockAnimationFrames();
  const calls: { animated: boolean }[] = [];
  const nativeView = { scrollToEnd: (options: { animated: boolean }) => calls.push(options) };
  const tree = (enabled: boolean) => (
    <ScrollView scrollToEndOnLayout={enabled} onContentSizeChange={() => {}} />
  );
  let renderer: ReturnType<typeof create> | undefined;
  const grow = (height: number) => renderer!.root.findByType(NativeScrollView).props.onContentSizeChange(600, height);
  try {
    await act(async () => { renderer = create(tree(true), { createNodeMock: () => nativeView }); });
    await act(async () => {
      grow(800);
      grow(820);
      flushFrame();
      grow(840);
      flushFrame();
    });
    expect(calls).toEqual([{ animated: false }, { animated: false }]);
    await act(async () => {
      grow(860);
      renderer!.update(tree(false));
    });
    await act(async () => {
      flushFrame();
      grow(880);
      flushFrame();
    });
    expect(calls).toHaveLength(2);
    await act(async () => { renderer!.update(tree(true)); });
    await act(async () => { flushFrame(); });
    expect(calls).toHaveLength(2);
    await act(async () => { grow(900); flushFrame(); });
    expect(calls).toHaveLength(3);
  } finally {
    await act(async () => { renderer?.unmount(); });
    restore();
  }
});

import React from 'react';
import { View } from 'react-native';

const INITIAL_SCROLL_TIMEOUT_MS = 1000;

type Size = { width: number; height: number };
type Offset = { x: number; y: number };
type TrackLayout = () => (() => void) | undefined;
type InitialLayout = {
  pending: boolean;
  waiting: number;
  viewport?: Size;
  content?: Size;
  frame?: number;
};

export const InitialScrollContentContext = React.createContext<TrackLayout | null>(null);

/**
 * Reports when asynchronous content first participates in native layout.
 * During initial end positioning this adds a View that remains mounted.
 * Put flex styles that need a direct scroll content child on an enclosing View.
 */
export function InitialScrollContent({ children }: { children: React.ReactNode }) {
  const context = React.useContext(InitialScrollContentContext);
  // Keep the wrapper mounted after the scroll view is revealed.
  const [track] = React.useState(() => context);
  const finish = React.useRef<(() => void) | undefined>(undefined);
  React.useLayoutEffect(() => {
    finish.current = track?.();
    return () => finish.current?.();
  }, [track]);
  const onLayout = React.useCallback(() => {
    finish.current?.();
    finish.current = undefined;
  }, []);
  return track && children != null ? <View onLayout={onLayout}>{children}</View> : children;
}

function measureEnd({ content, viewport }: InitialLayout, horizontal: boolean) {
  if (!content || !viewport) return undefined;
  const extent = horizontal ? viewport.width : viewport.height;
  if (extent <= 0) return undefined;
  return Math.max(0, (horizontal ? content.width : content.height) - extent);
}

function finishInitialLayout(
  layout: InitialLayout,
  horizontal: boolean,
  setOffset: (offset: Offset) => void,
  timedOut = false
) {
  if (!layout.pending || (!timedOut && layout.waiting)) return;
  const measuredEnd = measureEnd(layout, horizontal);
  if (measuredEnd === undefined && !timedOut) return;
  const endOffset = measuredEnd ?? 0;
  layout.pending = false;
  setOffset(horizontal ? { x: endOffset, y: 0 } : { x: 0, y: endOffset });
}

function useInitialOffset(layout: InitialLayout, horizontal: boolean) {
  const [offset, setOffset] = React.useState<Offset>();
  const schedule = React.useCallback(() => {
    if (!layout.pending || layout.frame !== undefined) return;
    layout.frame = requestAnimationFrame(() => {
      layout.frame = undefined;
      finishInitialLayout(layout, horizontal, setOffset);
    });
  }, [horizontal, layout]);

  React.useEffect(() => {
    if (offset || !layout.pending) return;
    schedule();
    const timeout = setTimeout(
      () => finishInitialLayout(layout, horizontal, setOffset, true),
      INITIAL_SCROLL_TIMEOUT_MS
    );
    return () => {
      clearTimeout(timeout);
      if (layout.frame !== undefined) cancelAnimationFrame(layout.frame);
      layout.frame = undefined;
    };
  }, [horizontal, layout, offset, schedule]);
  return { offset, schedule };
}

export function useInitialScroll(
  position: 'start' | 'end' | undefined,
  horizontal: boolean,
  onComplete?: (offset: Offset) => void
) {
  const [end] = React.useState(position === 'end');
  const layout = React.useRef<InitialLayout>({ pending: end, waiting: 0 }).current;
  const { offset, schedule } = useInitialOffset(layout, horizontal);
  const completed = React.useRef(onComplete);
  completed.current = onComplete;

  const trackLayout = React.useCallback(() => {
    if (!layout.pending) return undefined;
    layout.waiting++;
    let pending = true;
    return () => {
      if (!pending) return;
      pending = false;
      layout.waiting--;
      schedule();
    };
  }, [layout, schedule]);

  React.useLayoutEffect(() => {
    if (offset) completed.current?.(offset);
  }, [offset]);

  return {
    hidden: end && offset === undefined,
    offset,
    track: layout.pending ? trackLayout : null,
    setViewport(size: Size) {
      layout.viewport = size;
      schedule();
    },
    setContent(width: number, height: number) {
      layout.content = { width, height };
      schedule();
    },
  };
}

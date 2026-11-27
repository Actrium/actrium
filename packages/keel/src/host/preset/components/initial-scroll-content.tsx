import React from 'react';
import { View } from 'react-native';

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

/** Report when an asynchronous child first participates in native layout. */
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

function finishInitialLayout(
  layout: InitialLayout,
  horizontal: boolean,
  setOffset: (offset: Offset) => void
) {
  layout.frame = undefined;
  const { content, viewport, waiting } = layout;
  if (waiting || !content || !viewport) return;
  const extent = horizontal ? viewport.width : viewport.height;
  if (extent <= 0) return;
  const endOffset = Math.max(0, (horizontal ? content.width : content.height) - extent);
  layout.pending = false;
  setOffset(horizontal ? { x: endOffset, y: 0 } : { x: 0, y: endOffset });
}

export function useInitialScroll(
  position: 'start' | 'end' | undefined,
  horizontal: boolean,
  onComplete?: (offset: Offset) => void
) {
  const [end] = React.useState(position === 'end');
  const [offset, setOffset] = React.useState<Offset>();
  const layout = React.useRef<InitialLayout>({ pending: end, waiting: 0 }).current;
  const completed = React.useRef(onComplete);
  completed.current = onComplete;

  const schedule = React.useCallback(() => {
    if (!layout.pending || layout.frame !== undefined) return;
    layout.frame = requestAnimationFrame(() => finishInitialLayout(layout, horizontal, setOffset));
  }, [horizontal, layout]);

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
  React.useEffect(
    () => () => {
      if (layout.frame !== undefined) cancelAnimationFrame(layout.frame);
      layout.frame = undefined;
    },
    [layout]
  );

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

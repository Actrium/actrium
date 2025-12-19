/**
 * ScrollView Component (Web)
 *
 * Maps to div with overflow scroll
 */

import React from 'react';

export interface ScrollEvent {
  nativeEvent: {
    contentOffset: { x: number; y: number };
  };
}

export interface ScrollViewProps {
  style?: React.CSSProperties;
  children?: React.ReactNode;
  className?: string;
  horizontal?: boolean;
  onScroll?: (event: ScrollEvent) => void;
}

export function ScrollView({
  style,
  children,
  className,
  horizontal = false,
  onScroll,
}: ScrollViewProps): React.ReactElement {
  const baseStyle: React.CSSProperties = {
    display: 'flex',
    flexDirection: horizontal ? 'row' : 'column',
    overflow: 'auto',
    ...style,
  };

  const handleScroll = (e: React.UIEvent<HTMLDivElement>) => {
    if (onScroll) {
      onScroll({
        nativeEvent: {
          contentOffset: {
            x: e.currentTarget.scrollLeft,
            y: e.currentTarget.scrollTop,
          },
        },
      });
    }
  };

  return (
    <div style={baseStyle} className={className} onScroll={handleScroll}>
      {children}
    </div>
  );
}

export default ScrollView;

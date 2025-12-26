/**
 * Image Component (Web)
 *
 * Maps to img element
 */

import type React from 'react';

export interface ImageSource {
  uri: string;
  width?: number;
  height?: number;
}

export interface ImageProps {
  source: ImageSource | ImageSource[];
  style?: React.CSSProperties;
  className?: string;
  alt?: string;
  onLoad?: () => void;
  onError?: () => void;
}

export function Image({
  source,
  style,
  className,
  alt = '',
  onLoad,
  onError,
}: ImageProps): React.ReactElement {
  const src = Array.isArray(source) ? source[0]?.uri : source.uri;

  return (
    <img
      src={src}
      alt={alt}
      style={style}
      className={className}
      onLoad={onLoad}
      onError={onError}
    />
  );
}

export default Image;

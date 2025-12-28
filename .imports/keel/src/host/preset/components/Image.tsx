/**
 * Image Component
 *
 * Default Image component implementation, wrapping React Native Image
 */

import type React from 'react';
import {
  type ImageErrorEventData,
  type ImageSourcePropType,
  type ImageStyle,
  Image as RNImage,
  type NativeSyntheticEvent,
} from 'react-native';

export interface ImageSource {
  uri: string;
  width?: number;
  height?: number;
  headers?: Record<string, string>;
}

export interface ImageProps {
  source: ImageSource | ImageSourcePropType;
  style?: ImageStyle;
  resizeMode?: 'cover' | 'contain' | 'stretch' | 'repeat' | 'center';
  testID?: string;
  accessible?: boolean;
  accessibilityLabel?: string;
  fadeDuration?: number;
  blurRadius?: number;
  onLoad?: () => void;
  onLoadStart?: () => void;
  onLoadEnd?: () => void;
  onError?: (error: { nativeEvent: { error: string } }) => void;
}

export function Image({
  source,
  style,
  resizeMode = 'cover',
  testID,
  accessible,
  accessibilityLabel,
  fadeDuration,
  blurRadius,
  onLoad,
  onLoadStart,
  onLoadEnd,
  onError,
}: ImageProps): React.ReactElement {
  // Handle source format
  const imageSource: ImageSourcePropType =
    typeof source === 'object' && 'uri' in source
      ? { uri: source.uri, headers: source.headers }
      : source;

  // Sanitize error event to only pass serializable data
  const handleError = onError
    ? (event: NativeSyntheticEvent<ImageErrorEventData>) => {
        onError({
          nativeEvent: {
            error: event.nativeEvent.error,
          },
        });
      }
    : undefined;

  return (
    <RNImage
      source={imageSource}
      style={style}
      resizeMode={resizeMode}
      testID={testID}
      accessible={accessible}
      accessibilityLabel={accessibilityLabel}
      fadeDuration={fadeDuration}
      blurRadius={blurRadius}
      onLoad={onLoad ? () => onLoad() : undefined}
      onLoadStart={onLoadStart ? () => onLoadStart() : undefined}
      onLoadEnd={onLoadEnd ? () => onLoadEnd() : undefined}
      onError={handleError}
    />
  );
}

export default Image;

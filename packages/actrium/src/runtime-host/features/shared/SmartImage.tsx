import React, { useState } from 'react';
import { Image, ImageProps, ImageSourcePropType } from 'react-native';

interface SmartImageProps extends Omit<ImageProps, 'source'> {
  cdnUrl?: string;
  localUrl?: string;
}

/**
 * Smart Picture Component
 * Load Priority cdnUrl, if you fail, back to localUrl
 */
export const SmartImage: React.FC<SmartImageProps> = ({
  cdnUrl,
  localUrl,
  style,
  ...props
}) => {
  const hasLocalUrl = Boolean(localUrl);
  const [useLocal, setUseLocal] = useState(hasLocalUrl);

  // Encoding local paths to prevent characters like spaces from being loaded to fail
  const encodedLocalUrl = localUrl
    ? localUrl.startsWith('file://')
      ? encodeURI(localUrl)
      : `file://${encodeURI(localUrl)}`
    : undefined;

  // If not cdnUrl, or already marked as using locally, directly using locally
  // We'll only go there when local path is real, or we'll go straight. CDNavoid generation `{ uri: undefined }` The image is completely unloaded.
  const source: ImageSourcePropType = useLocal && encodedLocalUrl
    ? { uri: encodedLocalUrl }
    : { uri: cdnUrl };

  return (
    <Image
      {...props}
      source={source}
      style={style}
      onError={(e) => {
        props.onError?.(e);
        const errorMsg = e.nativeEvent.error;
        console.warn('[SmartImage] Failed to load image:', {
          source,
          error: errorMsg,
          useLocal,
          localUrl: encodedLocalUrl,
          cdnUrl,
        });

        if (!useLocal && hasLocalUrl) {
          console.log('[SmartImage] CDN load failed, falling back to local');
          setUseLocal(true);
        }
      }}
    />
  );
};

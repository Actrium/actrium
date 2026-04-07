import React, { useState } from 'react';
import { Image, ImageProps, ImageSourcePropType } from 'react-native';

interface SmartImageProps extends Omit<ImageProps, 'source'> {
  cdnUrl?: string;
  localUrl?: string;
}

/**
 * 智能图片组件
 * 优先加载 cdnUrl，如果失败则回退到 localUrl
 */
export const SmartImage: React.FC<SmartImageProps> = ({
  cdnUrl,
  localUrl,
  style,
  ...props
}) => {
  const [useLocal, setUseLocal] = useState(true);

  // 对本地路径进行编码处理，防止空格等字符导致加载失败
  const encodedLocalUrl = localUrl
    ? localUrl.startsWith('file://')
      ? encodeURI(localUrl)
      : `file://${encodeURI(localUrl)}`
    : undefined;

  // 如果没有 cdnUrl，或者已经标记为使用本地，则直接用本地
  const source: ImageSourcePropType =
    !cdnUrl || useLocal ? { uri: encodedLocalUrl } : { uri: cdnUrl };

  return (
    <Image
      {...props}
      source={source}
      style={style}
      onError={(e) => {
        const errorMsg = e.nativeEvent.error;
        console.warn('[SmartImage] Failed to load image:', {
          source,
          error: errorMsg,
          useLocal,
          localUrl: encodedLocalUrl,
          cdnUrl,
        });

        if (!useLocal && localUrl) {
          console.log('[SmartImage] CDN load failed, falling back to local');
          setUseLocal(true);
        }
      }}
    />
  );
};

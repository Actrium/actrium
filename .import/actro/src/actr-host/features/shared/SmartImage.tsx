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
  const hasLocalUrl = Boolean(localUrl);
  const [useLocal, setUseLocal] = useState(hasLocalUrl);

  // 对本地路径进行编码处理，防止空格等字符导致加载失败
  const encodedLocalUrl = localUrl
    ? localUrl.startsWith('file://')
      ? encodeURI(localUrl)
      : `file://${encodeURI(localUrl)}`
    : undefined;

  // 如果没有 cdnUrl，或者已经标记为使用本地，则直接用本地
  // 只有本地路径真实存在时才优先走本地，否则直接走 CDN，避免生成 `{ uri: undefined }` 导致图片完全不加载。
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

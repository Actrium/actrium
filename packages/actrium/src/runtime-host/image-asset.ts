import { decodeBase64 } from './base64';

const JPEG_PREFIX = 'data:image/jpeg;base64,';
const MAX_THUMBNAIL_BASE64_LENGTH = 16 * 1024;

export function isAllowedThumbnailBase64(value: unknown): value is string {
  if (typeof value !== 'string' || value.length < 12 ||
      value.length > MAX_THUMBNAIL_BASE64_LENGTH || value.length % 4 !== 0 ||
      !value.startsWith('/9j/') ||
      !/^(?:[A-Za-z0-9+/]{4})*(?:[A-Za-z0-9+/]{2}==|[A-Za-z0-9+/]{3}=)?$/.test(value)) {
    return false;
  }
  const tail = decodeBase64(value.slice(-8));
  return tail[tail.length - 2] === 0xff && tail[tail.length - 1] === 0xd9;
}

/** Render package assets and bounded, local raster thumbnails. */
export function isAllowedImageSource(source: unknown): boolean {
  if (typeof source === 'number') return true;
  if (!source || typeof source !== 'object' || !('uri' in source) ||
      Object.keys(source).length !== 1 || !Object.hasOwn(source, 'uri')) return false;
  const { uri } = source;
  return typeof uri === 'string' && uri.startsWith(JPEG_PREFIX) &&
    isAllowedThumbnailBase64(uri.slice(JPEG_PREFIX.length));
}

/**
 * Display-only props forwarded to the native image. Network-affecting props
 * such as `src`, `srcSet`, `crossOrigin`, `referrerPolicy`, `defaultSource`
 * and `loadingIndicatorSource` are deliberately excluded so a Guest cannot
 * bypass the `source` check above.
 */
export const ALLOWED_GUEST_IMAGE_PROPS = [
  'source',
  'style',
  'resizeMode',
  'testID',
  'accessible',
  'accessibilityLabel',
  'onLoad',
  'onError',
] as const;

export function pickGuestImageProps(
  props: Record<string, unknown>
): Record<string, unknown> {
  const picked: Record<string, unknown> = {};
  for (const key of ALLOWED_GUEST_IMAGE_PROPS) {
    if (props[key] !== undefined) {
      picked[key] = props[key];
    }
  }
  const source = props.source;
  if (source && typeof source === 'object' && 'uri' in source && isAllowedImageSource(source)) {
    picked.source = { uri: source.uri };
  }
  return picked;
}

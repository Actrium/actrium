/**
 * Only package-local asset ids can be rendered without a network capability.
 * Remote URIs would let a Guest bypass the host:net broker and attach
 * arbitrary headers, so they stay rejected until image loading is routed
 * through that broker.
 */
export function isAllowedImageSource(source: unknown): boolean {
  return typeof source === 'number';
}

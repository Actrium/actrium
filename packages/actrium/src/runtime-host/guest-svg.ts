import type { SvgPath, SvgProps } from 'keel/guest';
import { normalizeGuestSvgPath } from './guest-svg-path';

function size(value: unknown, max: number): value is number {
  return typeof value === 'number' && Number.isFinite(value) && value >= 0 && value <= max;
}

function paint(value: unknown): string | undefined {
  if (typeof value !== 'string') return undefined;
  return /^(?:#[\da-f]{3,4}|#[\da-f]{6}|#[\da-f]{8}|[a-z]{1,24})$/i.test(value) ? value : undefined;
}

function normalizePath(value: unknown, limit: number): { path: SvgPath; length: number } | undefined {
  if (!value || typeof value !== 'object') return undefined;
  const path = value as Partial<SvgPath>;
  if (typeof path.d !== 'string' || path.d.length > limit) return undefined;
  const d = normalizeGuestSvgPath(path.d);
  if (!d || d.length > limit) return undefined;
  return {
    length: Math.max(path.d.length, d.length),
    path: {
      d,
      fill: paint(path.fill),
      stroke: paint(path.stroke),
      fillRule: path.fillRule === 'evenodd' ? 'evenodd' : undefined,
      strokeWidth: size(path.strokeWidth, 4096) ? path.strokeWidth : undefined,
      strokeLinecap: path.strokeLinecap === 'round' || path.strokeLinecap === 'square' ? path.strokeLinecap : undefined,
      strokeLinejoin: path.strokeLinejoin === 'round' || path.strokeLinejoin === 'bevel' ? path.strokeLinejoin : undefined,
      opacity: size(path.opacity, 1) ? path.opacity : undefined,
    },
  };
}

export function normalizeGuestSvg(props: SvgProps): SvgProps | undefined {
  const { width, height, viewBox, paths } = props;
  if (!size(width, 4096) || width === 0 || !size(height, 4096) || height === 0 ||
      typeof viewBox !== 'string' || viewBox.length > 128 || !/^[\dEe.,+\s-]+$/.test(viewBox) ||
      !Array.isArray(paths) || paths.length === 0 || paths.length > 256) return undefined;
  const box = viewBox.trim().split(/[\s,]+/).map(Number);
  if (box.length !== 4 || box.some((n) => !Number.isFinite(n) || Math.abs(n) > 1_000_000) ||
      box[2]! <= 0 || box[3]! <= 0) return undefined;

  const safePaths: SvgPath[] = [];
  let remaining = 128 * 1024;
  for (const value of paths) {
    const result = normalizePath(value, Math.min(16_384, remaining));
    if (!result) return undefined;
    remaining -= result.length;
    safePaths.push(result.path);
  }
  return {
    width, height, viewBox: box.join(' '), paths: safePaths, color: paint(props.color),
    style: props.style, testID: typeof props.testID === 'string' ? props.testID : undefined,
    accessibilityLabel: typeof props.accessibilityLabel === 'string' ? props.accessibilityLabel : undefined,
  };
}

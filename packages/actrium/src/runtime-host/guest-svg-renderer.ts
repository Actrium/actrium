import type { ComponentType } from 'react';
import type { SvgProps } from 'keel/guest';

// Metro selects the RNSVG implementation in macOS builds that link the native renderer.
export const Svg: ComponentType<SvgProps> | undefined = undefined;

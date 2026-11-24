import React from 'react';
import NativeSvg, { Path } from 'react-native-svg';
import { normalizeGuestSvg } from './guest-svg';

export function Svg(input) {
  const svg = normalizeGuestSvg(input);
  if (!svg) return null;
  const { paths, accessibilityLabel, ...props } = svg;
  return (
    <NativeSvg {...props} accessible={!!accessibilityLabel} accessibilityRole="image"
      accessibilityLabel={accessibilityLabel} pointerEvents="none">
      {paths.map((path, index) => <Path key={index} {...path} />)}
    </NativeSvg>
  );
}

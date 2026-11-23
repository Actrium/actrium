import type { ComponentType } from 'react';

// Metro selects the Supramark implementation for macOS builds.
export const Supramark: ComponentType<Record<string, unknown>> | undefined =
  undefined as ComponentType<Record<string, unknown>> | undefined;

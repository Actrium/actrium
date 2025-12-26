/**
 * Guest Component Factory
 *
 * Creates type-safe Guest component identifiers for the rill reconciler.
 * In rill's model, Guest components are plain string identifiers that
 * the reconciler serializes and sends to the Host for rendering.
 */

import type React from 'react';

/**
 * Create a Guest component identifier with proper TypeScript typing.
 *
 * @example
 * ```typescript
 * import { createGuestComponent } from './guest-factory';
 * import type { ChatBubbleProps } from '../../types';
 *
 * export const ChatBubble = createGuestComponent<ChatBubbleProps>('ChatBubble');
 * ```
 *
 * @param name - The component name (must match Host component registration)
 * @returns A typed ElementType that can be used in JSX
 */
export function createGuestComponent<P>(name: string): React.ElementType<P> {
  return name as unknown as React.ElementType<P>;
}

/**
 * Batch create multiple Guest components
 *
 * @example
 * ```typescript
 * const components = createGuestComponents({
 *   ChatBubble: {} as ChatBubbleProps,
 *   StepList: {} as StepListProps,
 * });
 *
 * export const { ChatBubble, StepList } = components;
 * ```
 */
export function createGuestComponents<T extends Record<string, unknown>>(
  definitions: T
): { [K in keyof T]: React.ElementType<T[K]> } {
  const result = {} as { [K in keyof T]: React.ElementType<T[K]> };

  for (const name of Object.keys(definitions)) {
    result[name as keyof T] = createGuestComponent(name);
  }

  return result;
}

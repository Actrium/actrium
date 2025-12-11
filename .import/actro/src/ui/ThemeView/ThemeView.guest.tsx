/**
 * ThemeView - Guest Component (Type-Safe)
 *
 * Container component that applies theme-aware styling
 */

import type { ThemeViewProps } from '../../types';
import { createGuestComponent } from '../../core/component-helpers';

/**
 * ThemeView component factory
 *
 * @example
 * ```tsx
 * import { ThemeView } from 'askit';
 *
 * const view = ThemeView({
 *   variant: 'primary',
 *   padding: 'medium',
 *   children: [...],
 * });
 * ```
 */
export const ThemeView = createGuestComponent<ThemeViewProps>('ThemeView');

export default ThemeView;

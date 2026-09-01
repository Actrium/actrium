/**
 * ThemeView - Guest Component
 */

import type { ThemeViewProps } from '../../types';
import { createGuestComponent } from '../guest-factory';

export const ThemeView = createGuestComponent<ThemeViewProps>('ThemeView');

export default ThemeView;

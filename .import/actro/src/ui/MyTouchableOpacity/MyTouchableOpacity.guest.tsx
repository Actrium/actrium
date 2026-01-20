/**
 * MyTouchableOpacity - Guest Component
 */

import type { MyTouchableOpacityProps } from '../../types';
import { createGuestComponent } from '../guest-factory';

export const MyTouchableOpacity = createGuestComponent<MyTouchableOpacityProps>('MyTouchableOpacity');

export default MyTouchableOpacity;


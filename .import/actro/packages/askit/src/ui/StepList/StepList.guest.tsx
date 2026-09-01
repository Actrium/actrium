/**
 * StepList - Guest Component
 */

import type { StepListProps } from '../../types';
import { createGuestComponent } from '../guest-factory';

export const StepList = createGuestComponent<StepListProps>('StepList');

export default StepList;

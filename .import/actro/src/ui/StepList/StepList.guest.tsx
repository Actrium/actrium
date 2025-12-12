/**
 * StepList - Guest Component (Type-Safe)
 *
 * This is a type-safe guest component that validates props at runtime.
 * The actual rendering happens on the Host side.
 */

import { createGuestComponent, validators } from '../../core/component-helpers';
import type { StepListProps } from '../../types';

/**
 * StepList component factory
 *
 * @example
 * ```tsx
 * import { StepList } from 'askit';
 *
 * const stepList = StepList({
 *   items: [
 *     { id: '1', title: 'Step 1', status: 'completed' },
 *     { id: '2', title: 'Step 2', status: 'active' },
 *   ],
 * });
 * ```
 */
export const StepList = createGuestComponent<StepListProps>('StepList', [
  {
    field: 'items',
    type: 'array',
    required: true,
    validator: validators.nonEmptyArray,
    message: '[StepList] items must be a non-empty array',
  },
]);

export default StepList;

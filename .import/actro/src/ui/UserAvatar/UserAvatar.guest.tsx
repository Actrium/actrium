/**
 * UserAvatar - Guest Component (Type-Safe)
 *
 * Displays user avatar with optional online status indicator
 */

import type { UserAvatarProps } from '../../types';
import { createGuestComponent } from '../../core/component-helpers';

/**
 * UserAvatar component factory
 *
 * @example
 * ```tsx
 * import { UserAvatar } from 'askit';
 *
 * const avatar = UserAvatar({
 *   uri: 'https://example.com/avatar.jpg',
 *   name: 'John Doe',
 *   size: 'medium',
 *   showOnlineStatus: true,
 *   isOnline: true,
 * });
 * ```
 */
export const UserAvatar = createGuestComponent<UserAvatarProps>('UserAvatar');

export default UserAvatar;

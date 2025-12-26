/**
 * UserAvatar - Guest Component
 */

import type { UserAvatarProps } from '../../types';
import { createGuestComponent } from '../guest-factory';

export const UserAvatar = createGuestComponent<UserAvatarProps>('UserAvatar');

export default UserAvatar;

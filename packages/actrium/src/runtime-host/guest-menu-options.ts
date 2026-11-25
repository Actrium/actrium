import type { MenuOption } from 'keel/guest';

export function isGuestMenuOption(option: unknown): option is MenuOption {
  if (typeof option !== 'object' || option === null) return false;
  const { value, label, disabled } = option as Partial<MenuOption>;
  return typeof value === 'string' && typeof label === 'string' &&
    (disabled === undefined || typeof disabled === 'boolean');
}

export function guestMenuEntries(items: readonly unknown[]): MenuOption[] {
  return Array.from(items, (item) => {
    if (!isGuestMenuOption(item)) throw new TypeError('Invalid guest menu option.');
    const { value, label, disabled } = item;
    return { value, label, disabled };
  });
}

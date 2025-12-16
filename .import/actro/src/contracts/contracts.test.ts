import { validateGuestToHostPayload, validateHostToGuestPayload } from './generated';

describe('contracts payload validators', () => {
  it('validateHostToGuestPayload: HOST_VISIBILITY', () => {
    expect(validateHostToGuestPayload('HOST_VISIBILITY', { visible: true })).toBe(true);
    expect(validateHostToGuestPayload('HOST_VISIBILITY', { visible: true, tabId: 't1' })).toBe(
      true
    );
    expect(validateHostToGuestPayload('HOST_VISIBILITY', { tabId: 't1' })).toBe(false);
    expect(validateHostToGuestPayload('HOST_VISIBILITY', { visible: 'yes' })).toBe(false);
    expect(validateHostToGuestPayload('HOST_VISIBILITY', null)).toBe(false);
    expect(validateHostToGuestPayload('HOST_VISIBILITY', { visible: true, extra: 1 })).toBe(true);
  });

  it('validateGuestToHostPayload: GUEST_SLEEP_STATE', () => {
    expect(validateGuestToHostPayload('GUEST_SLEEP_STATE', { sleeping: true })).toBe(true);
    expect(
      validateGuestToHostPayload('GUEST_SLEEP_STATE', {
        sleeping: false,
        reason: 'hidden',
        tabId: 't1',
      })
    ).toBe(true);
    expect(validateGuestToHostPayload('GUEST_SLEEP_STATE', { sleeping: 'nope' })).toBe(false);
    expect(validateGuestToHostPayload('GUEST_SLEEP_STATE', {})).toBe(false);
    expect(validateGuestToHostPayload('GUEST_SLEEP_STATE', undefined)).toBe(false);
  });
});

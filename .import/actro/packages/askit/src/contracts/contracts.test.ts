import {
  isGuestToHostEventName,
  isHostToGuestEventName,
  validateGuestToHostPayload,
  validateHostToGuestPayload,
} from './generated';

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

  it('validateHostToGuestPayload: handles undefined optional fields', () => {
    // Test edge case where optional field is explicitly undefined
    expect(validateHostToGuestPayload('HOST_VISIBILITY', { visible: true, tabId: undefined })).toBe(
      true
    );
    expect(
      validateHostToGuestPayload('RECEIVER_BACKPRESSURE', {
        applied: 1,
        skipped: 0,
        total: 10,
        batchId: undefined,
      })
    ).toBe(true);
  });
});

describe('contracts type guards', () => {
  it('isHostToGuestEventName: should identify valid event names', () => {
    expect(isHostToGuestEventName('HOST_VISIBILITY')).toBe(true);
    expect(isHostToGuestEventName('RECEIVER_BACKPRESSURE')).toBe(true);
    expect(isHostToGuestEventName('INVALID_EVENT')).toBe(false);
    expect(isHostToGuestEventName('')).toBe(false);
  });

  it('isGuestToHostEventName: should identify valid event names', () => {
    expect(isGuestToHostEventName('GUEST_SLEEP_STATE')).toBe(true);
    expect(isGuestToHostEventName('INVALID_EVENT')).toBe(false);
    expect(isGuestToHostEventName('')).toBe(false);
  });
});

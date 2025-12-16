import { handleGuestMessage } from './bridge';

describe('Core Bridge - contracts', () => {
  let originalWarn: typeof console.warn;
  let originalError: typeof console.error;

  beforeEach(() => {
    originalWarn = console.warn;
    originalError = console.error;
  });

  afterEach(() => {
    console.warn = originalWarn;
    console.error = originalError;
  });

  it('should treat known contract events as non-unknown and call onContractEvent', () => {
    const events: Array<{ eventName: string; payload: unknown }> = [];
    const warnings: unknown[][] = [];
    const errors: unknown[][] = [];

    console.warn = (...args) => warnings.push(args);
    console.error = (...args) => errors.push(args);

    handleGuestMessage(
      { event: 'GUEST_SLEEP_STATE', payload: { sleeping: true, tabId: 't1' } },
      {
        onContractEvent: (eventName, payload) => events.push({ eventName, payload }),
      }
    );

    expect(events).toEqual([{ eventName: 'GUEST_SLEEP_STATE', payload: { sleeping: true, tabId: 't1' } }]);
    expect(warnings).toHaveLength(0);
    expect(errors).toHaveLength(0);
  });

  it('should report invalid contract payload', () => {
    const violations: unknown[] = [];
    const errors: unknown[][] = [];
    console.error = (...args) => errors.push(args);

    handleGuestMessage(
      { event: 'GUEST_SLEEP_STATE', payload: { sleeping: 'nope' } },
      {
        onContractViolation: (v) => violations.push(v),
      }
    );

    expect(violations).toHaveLength(1);
    expect(errors.length).toBeGreaterThan(0);
  });

  it('should report unknown events as violations', () => {
    const violations: unknown[] = [];
    const warnings: unknown[][] = [];
    console.warn = (...args) => warnings.push(args);

    handleGuestMessage(
      { event: 'SOME_RANDOM_EVENT', payload: { a: 1 } },
      {
        onContractViolation: (v) => violations.push(v),
      }
    );

    expect(violations).toHaveLength(1);
    expect(warnings.length).toBeGreaterThan(0);
  });
});


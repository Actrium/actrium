import { handleGuestMessage } from './bridge';
import { clearToastHandler, configureToast } from './registry.modules';

describe('Core Bridge - permissions', () => {
  afterEach(() => {
    clearToastHandler();
  });

  it('should deny module call when missing permission in deny mode', () => {
    const calls: string[] = [];
    const violations: Array<{ kind?: string }> = [];

    const originalWarn = console.warn;
    console.warn = () => {};

    configureToast((message) => calls.push(message));

    handleGuestMessage(
      { event: 'askit:toast:show', payload: 'hi' },
      {
        permissions: [],
        permissionMode: 'deny',
        onContractViolation: (v) => violations.push(v),
      }
    );

    expect(calls).toEqual([]);
    expect(violations.some((v) => v.kind === 'missing_permission')).toBe(true);

    console.warn = originalWarn;
  });

  it('should warn but allow module call when missing permission in warn mode', () => {
    const calls: string[] = [];
    const violations: Array<{ kind?: string }> = [];

    const originalWarn = console.warn;
    console.warn = () => {};

    configureToast((message) => calls.push(message));

    handleGuestMessage(
      { event: 'askit:toast:show', payload: 'hi' },
      {
        permissions: [],
        permissionMode: 'warn',
        onContractViolation: (v) => violations.push(v),
      }
    );

    expect(calls).toEqual(['hi']);
    expect(violations.some((v) => v.kind === 'missing_permission')).toBe(true);

    console.warn = originalWarn;
  });

  it('should bypass permission checks in allow mode', () => {
    const calls: string[] = [];
    const violations: Array<{ kind?: string }> = [];

    configureToast((message) => calls.push(message));

    handleGuestMessage(
      { event: 'askit:toast:show', payload: 'hi' },
      {
        permissions: [],
        permissionMode: 'allow',
        onContractViolation: (v) => violations.push(v),
      }
    );

    expect(calls).toEqual(['hi']);
    expect(violations).toHaveLength(0);
  });
});


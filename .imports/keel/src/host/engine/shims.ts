/**
 * Guest Sandbox Shims
 *
 * NOTE: The React/JSX shims (ALL_SHIMS) have been removed.
 * Guest now uses real React bundled via src/guest/bundle.ts.
 *
 * Only DEVTOOLS_SHIM remains for DevTools error capturing.
 */

/**
 * DevTools Guest shim
 * Provides error capturing and reporting to host DevTools
 */
export const DEVTOOLS_SHIM = `
if (typeof globalThis.__sendEventToHost === 'function') {
  globalThis.__rill_devtools_guest = {
    captureError: function(error, context) {
      globalThis.__sendEventToHost('devtools:error', {
        message: error?.message || String(error),
        stack: error?.stack,
        context: context
      });
    }
  };
}
`;

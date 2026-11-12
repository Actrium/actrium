const LEGACY_ECHO_APP_ID = 'actrium/echo-ui';

/**
 * The legacy `ACTR_CALL` bridge is confined to the echo demo tab. Any other
 * service-capable package must use the per-tab `host:service` path.
 */
export function isLegacyEchoTab(appId: string | undefined): boolean {
  if (typeof appId !== 'string') return false;
  return (
    appId === LEGACY_ECHO_APP_ID ||
    appId.startsWith(`${LEGACY_ECHO_APP_ID}@`)
  );
}

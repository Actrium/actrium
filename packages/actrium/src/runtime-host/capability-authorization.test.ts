import { describe, expect, test } from 'bun:test';

import {
  capabilitiesFromSnapshotJSON,
  CapabilityAccessError,
  CapabilityAuthorizer,
  effectiveCapabilities,
  evaluateCapabilityAccess,
  type CapabilityGrantStatus,
  type NativeCapabilitySnapshot,
} from './capability-authorization';
import {
  FRONT_CAPABILITY_DEFINITIONS,
  type FrontCapabilityDefinitions,
} from './capability-model';

function snapshot(
  overrides: Partial<NativeCapabilitySnapshot> = {}
): NativeCapabilitySnapshot {
  return {
    schemaVersion: 1,
    revision: 1,
    subject: {
      publisher: 'actrium',
      program: 'counter',
      majorVersion: 1,
      packageDigest: 'sha256:test',
    },
    audience: 'third_party',
    declaredCapabilities: ['service', 'host:clipboard.write'],
    grants: { 'host:clipboard.write': 'prompt' },
    platform: {},
    session: {
      generation: 1,
      state: 'active',
      allowedCapabilities: ['service', 'host:clipboard.write'],
    },
    ...overrides,
  };
}

function withGrant(
  status: CapabilityGrantStatus
): NativeCapabilitySnapshot {
  return snapshot({ grants: { 'host:clipboard.write': status } });
}

const cameraDefinitions: FrontCapabilityDefinitions = {
  ...FRONT_CAPABILITY_DEFINITIONS,
  'host:camera.capturePhoto': {
    ...FRONT_CAPABILITY_DEFINITIONS['host:camera.capturePhoto'],
    implemented: true,
  },
};

describe('capability authorization snapshot', () => {
  test('effective set excludes prompt capabilities and includes no-grant methods', () => {
    expect(effectiveCapabilities(snapshot())).toEqual(['service.call']);
  });

  test('effective set accepts a granted method-level capability', () => {
    expect(effectiveCapabilities(withGrant('granted'))).toEqual([
      'service.call',
      'host:clipboard.write',
    ]);
  });

  test('module grants do not cover future methods', () => {
    const result = evaluateCapabilityAccess(
      'host:clipboard.write',
      snapshot({ grants: { 'host:clipboard': 'granted' } })
    );
    expect(result).toEqual({
      allowed: false,
      capability: 'host:clipboard.write',
      code: 'permission_required',
    });
  });

  test('evaluate returns prompt, denied and restricted grant reasons', () => {
    expect(evaluateCapabilityAccess('host:clipboard.write', withGrant('prompt'))).toEqual({
      allowed: false,
      capability: 'host:clipboard.write',
      code: 'permission_required',
    });
    expect(evaluateCapabilityAccess('host:clipboard.write', withGrant('denied'))).toEqual({
      allowed: false,
      capability: 'host:clipboard.write',
      code: 'permission_denied',
    });
    expect(
      evaluateCapabilityAccess('host:clipboard.write', withGrant('restricted'))
    ).toEqual({
      allowed: false,
      capability: 'host:clipboard.write',
      code: 'permission_restricted',
    });
  });

  test('OS permissions are checked only for OS-backed capabilities', () => {
    const cameraSnapshot = snapshot({
      declaredCapabilities: ['host:camera.capturePhoto'],
      grants: { 'host:camera.capturePhoto': 'granted' },
      platform: { camera: 'prompt' },
      session: {
        generation: 1,
        state: 'active',
        allowedCapabilities: ['host:camera.capturePhoto'],
      },
    });
    const options = {
      definitions: cameraDefinitions,
      hostSupported: new Set(['host:camera.capturePhoto']),
    };

    expect(evaluateCapabilityAccess('host:camera.capturePhoto', cameraSnapshot, options)).toEqual({
      allowed: false,
      capability: 'host:camera.capturePhoto',
      code: 'platform_permission_required',
    });
    expect(
      evaluateCapabilityAccess(
        'host:camera.capturePhoto',
        { ...cameraSnapshot, platform: { camera: 'denied' } },
        options
      )
    ).toEqual({
      allowed: false,
      capability: 'host:camera.capturePhoto',
      code: 'platform_permission_denied',
    });
    expect(
      evaluateCapabilityAccess(
        'host:camera.capturePhoto',
        { ...cameraSnapshot, platform: { camera: 'granted' } },
        options
      )
    ).toEqual({ allowed: true, capability: 'host:camera.capturePhoto' });

    // clipboard has no OS permission and stays unaffected by an empty map
    expect(evaluateCapabilityAccess('host:clipboard.write', withGrant('granted'))).toEqual({
      allowed: true,
      capability: 'host:clipboard.write',
    });
  });

  test('session state and allowed set gate protected methods', () => {
    expect(
      evaluateCapabilityAccess(
        'host:clipboard.write',
        snapshot({
          grants: { 'host:clipboard.write': 'granted' },
          session: {
            generation: 1,
            state: 'detached',
            allowedCapabilities: ['host:clipboard.write'],
          },
        })
      )
    ).toEqual({
      allowed: false,
      capability: 'host:clipboard.write',
      code: 'session_detached',
    });

    expect(
      evaluateCapabilityAccess(
        'host:clipboard.write',
        snapshot({
          grants: { 'host:clipboard.write': 'granted' },
          session: {
            generation: 1,
            state: 'active',
            allowedCapabilities: ['service'],
          },
        })
      )
    ).toEqual({
      allowed: false,
      capability: 'host:clipboard.write',
      code: 'session_capability_denied',
    });
  });

  test('system capabilities require system audience', () => {
    const definitions: FrontCapabilityDefinitions = {
      ...FRONT_CAPABILITY_DEFINITIONS,
      'system:diagnostics.view': {
        ...FRONT_CAPABILITY_DEFINITIONS['system:diagnostics.view'],
        implemented: true,
      },
    };
    const systemSnapshot = snapshot({
      declaredCapabilities: ['system:diagnostics.view'],
      grants: { 'system:diagnostics.view': 'granted' },
      session: {
        generation: 1,
        state: 'active',
        allowedCapabilities: ['system:diagnostics.view'],
      },
    });
    expect(
      evaluateCapabilityAccess('system:diagnostics.view', systemSnapshot, {
        definitions,
        hostSupported: new Set(['system:diagnostics.view']),
      })
    ).toEqual({
      allowed: false,
      capability: 'system:diagnostics.view',
      code: 'capability_audience_denied',
    });
  });

  test('parses only valid snapshots', () => {
    expect(capabilitiesFromSnapshotJSON(JSON.stringify(snapshot()))).toEqual(
      snapshot()
    );
    expect(capabilitiesFromSnapshotJSON('{')).toBeNull();
    expect(
      capabilitiesFromSnapshotJSON(
        JSON.stringify({ ...snapshot(), schemaVersion: 2 })
      )
    ).toBeNull();
  });
});

describe('CapabilityAuthorizer', () => {
  test('authorizes immediately when the snapshot already allows the method', async () => {
    let requested = false;
    const authorizer = new CapabilityAuthorizer(withGrant('granted'), () => {
      requested = true;
    });
    await authorizer.authorize('host:clipboard.write');
    expect(requested).toBe(false);
  });

  test('requests a grant and resolves after a newer snapshot', async () => {
    const requested: string[] = [];
    const authorizer = new CapabilityAuthorizer(withGrant('prompt'), (capability) => {
      requested.push(capability);
    });
    const pending = authorizer.authorize('host:clipboard.write');
    expect(requested).toEqual(['host:clipboard.write']);
    authorizer.update({ ...withGrant('granted'), revision: 2 });
    await expect(pending).resolves.toBeUndefined();
  });

  test('rejects after a newer denied snapshot', async () => {
    const authorizer = new CapabilityAuthorizer(withGrant('prompt'), () => {});
    const pending = authorizer.authorize('host:clipboard.write');
    authorizer.update({ ...withGrant('denied'), revision: 2 });
    await expect(pending).rejects.toMatchObject({
      code: 'permission_denied',
    } satisfies Partial<CapabilityAccessError>);
  });

  test('dispose rejects pending grants', async () => {
    const authorizer = new CapabilityAuthorizer(withGrant('prompt'), () => {});
    const pending = authorizer.authorize('host:clipboard.write');
    authorizer.dispose();
    await expect(pending).rejects.toMatchObject({
      code: 'session_detached',
    } satisfies Partial<CapabilityAccessError>);
  });

  test('rejects new grants after dispose', async () => {
    const authorizer = new CapabilityAuthorizer(withGrant('prompt'), () => {});
    authorizer.dispose();
    await expect(authorizer.authorize('host:clipboard.write')).rejects.toMatchObject({
      code: 'session_detached',
    } satisfies Partial<CapabilityAccessError>);
  });

  test('ignores snapshots that arrive after dispose', async () => {
    const authorizer = new CapabilityAuthorizer(withGrant('prompt'), () => {});
    const pending = authorizer.authorize('host:clipboard.write');
    authorizer.dispose();
    authorizer.update({ ...withGrant('granted'), revision: 2 });
    await expect(pending).rejects.toMatchObject({
      code: 'session_detached',
    } satisfies Partial<CapabilityAccessError>);
  });

  test('resolves every waiter on the same capability at once', async () => {
    const authorizer = new CapabilityAuthorizer(withGrant('prompt'), () => {});
    const first = authorizer.authorize('host:clipboard.write');
    const second = authorizer.authorize('host:clipboard.write');
    authorizer.update({ ...withGrant('granted'), revision: 2 });
    await expect(Promise.all([first, second])).resolves.toEqual([
      undefined,
      undefined,
    ]);
  });

  test('ignores revisions that are not newer', async () => {
    const authorizer = new CapabilityAuthorizer(withGrant('prompt'), () => {});
    let settled = false;
    const pending = authorizer
      .authorize('host:clipboard.write')
      .then(() => {
        settled = true;
      });
    authorizer.update({ ...withGrant('granted'), revision: 1 });
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(settled).toBe(false);

    authorizer.update({ ...withGrant('granted'), revision: 2 });
    await pending;
    expect(settled).toBe(true);
  });

  test('keeps waiters pending while the capability stays unresolved', async () => {
    const authorizer = new CapabilityAuthorizer(withGrant('prompt'), () => {});
    let settled = false;
    const pending = authorizer
      .authorize('host:clipboard.write')
      .then(() => {
        settled = true;
      });
    authorizer.update({ ...withGrant('prompt'), revision: 2 });
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(settled).toBe(false);

    authorizer.update({ ...withGrant('granted'), revision: 3 });
    await pending;
    expect(settled).toBe(true);
  });

  test('rejects stale grants when the subject changes at the same revision', async () => {
    const authorizer = new CapabilityAuthorizer(withGrant('granted'), () => {});
    authorizer.update({
      ...withGrant('denied'),
      revision: 1,
      subject: {
        publisher: 'actrium',
        program: 'other-program',
        majorVersion: 1,
        packageDigest: 'sha256:other',
      },
    });

    await expect(
      authorizer.authorize('host:clipboard.write')
    ).rejects.toMatchObject({
      code: 'permission_denied',
    } satisfies Partial<CapabilityAccessError>);
  });

  test('rejects pending waiters when the subject changes', async () => {
    const authorizer = new CapabilityAuthorizer(withGrant('prompt'), () => {});
    const pending = authorizer.authorize('host:clipboard.write');
    authorizer.update({
      ...withGrant('prompt'),
      subject: {
        publisher: 'actrium',
        program: 'other-program',
        majorVersion: 1,
        packageDigest: 'sha256:other',
      },
    });

    await expect(pending).rejects.toMatchObject({
      code: 'session_detached',
    } satisfies Partial<CapabilityAccessError>);
  });
});

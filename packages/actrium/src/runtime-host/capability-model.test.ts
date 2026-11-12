import { describe, expect, test } from 'bun:test';
import { FRONT_CAPABILITIES, type FrontCapability } from './capabilities';
import {
  computeFrontCapabilities,
  computeServiceCapabilities,
  FRONT_CAPABILITY_DEFINITIONS,
  HOST_SUPPORTED_CAPABILITIES,
  PLATFORM_PERMISSION_MAP,
  type CapabilityDefinition,
  type FrontCapabilityDefinitions,
  type FrontCapabilityInput,
} from './capability-model';

const all = new Set<string>(FRONT_CAPABILITIES);

const base: FrontCapabilityInput = {
  declared: ['service', 'host:assets'],
  hostSupported: all,
  guestKind: 'js',
  grants: new Set<string>(),
  platform: all,
  session: all,
  audience: 'third_party',
};

function withDefinitions(
  patches: Partial<Record<FrontCapability, Partial<CapabilityDefinition>>>
): FrontCapabilityDefinitions {
  const definitions = { ...FRONT_CAPABILITY_DEFINITIONS };
  for (const [capability, patch] of Object.entries(patches)) {
    const key = capability as FrontCapability;
    definitions[key] = { ...definitions[key], ...patch };
  }
  return definitions;
}

/**
 * Policy tests need capabilities that are actually implemented; the Host's
 * real table only marks `service.call` and `host:clipboard.write` so. Override
 * a copy instead of mutating the production definitions.
 */
function withImplemented(
  ...capabilities: FrontCapability[]
): FrontCapabilityDefinitions {
  return withDefinitions(
    Object.fromEntries(
      capabilities.map((capability) => [capability, { implemented: true }])
    )
  );
}

describe('computeFrontCapabilities', () => {
  test('expands module shorthands to method-level capabilities', () => {
    expect(
      computeFrontCapabilities(base, withImplemented('host:assets.read'))
    ).toEqual(['service.call', 'host:assets.read']);
  });

  test('drops capabilities that require a grant until granted', () => {
    const definitions = withImplemented('host:clipboard.write');
    expect(
      computeFrontCapabilities(
        { ...base, declared: ['host:clipboard.write'] },
        definitions
      )
    ).toEqual([]);
    expect(
      computeFrontCapabilities(
        {
          ...base,
          declared: ['host:clipboard.write'],
          grants: new Set(['host:clipboard.write']),
        },
        definitions
      )
    ).toEqual(['host:clipboard.write']);
    // A module grant covers its methods.
    expect(
      computeFrontCapabilities(
        {
          ...base,
          declared: ['host:clipboard'],
          grants: new Set(['host:clipboard']),
        },
        definitions
      )
    ).toEqual(['host:clipboard.write']);
  });

  test('accepts a method grant for a module shorthand', () => {
    expect(
      computeFrontCapabilities({
        ...base,
        declared: ['host:clipboard'],
        grants: new Set(['host:clipboard.write']),
      })
    ).toEqual(['host:clipboard.write']);
  });

  test('a grant for one method does not authorize another method', () => {
    const definitions = withDefinitions({
      'service.tell': {
        implemented: true,
        requiresGrant: 'user',
        defaultEnabled: false,
      },
    });

    expect(
      computeFrontCapabilities(
        {
          ...base,
          declared: ['service'],
          grants: new Set(['service.call']),
        },
        definitions
      )
    ).toEqual(['service.call']);

    expect(
      computeFrontCapabilities(
        {
          ...base,
          declared: ['service'],
          grants: new Set(['service.tell']),
        },
        definitions
      )
    ).toEqual(['service.call', 'service.tell']);

    // A module grant covers every method.
    expect(
      computeFrontCapabilities(
        {
          ...base,
          declared: ['service'],
          grants: new Set(['service']),
        },
        definitions
      )
    ).toEqual(['service.call', 'service.tell']);
  });

  test('drops capabilities the host does not support', () => {
    expect(
      computeFrontCapabilities({
        ...base,
        declared: ['service'],
        hostSupported: new Set(['host:clipboard']),
      })
    ).toEqual([]);
  });

  test('drops capabilities the platform or session does not allow', () => {
    expect(
      computeFrontCapabilities({
        ...base,
        declared: ['service'],
        platform: new Set(['host:assets']),
      })
    ).toEqual([]);
    expect(
      computeFrontCapabilities({
        ...base,
        declared: ['service'],
        session: new Set(['host:assets']),
      })
    ).toEqual([]);
  });

  test('maps the legacy actor.call alias to service.call', () => {
    expect(
      computeFrontCapabilities({ ...base, declared: ['actor.call'] })
    ).toEqual(['service.call']);
  });

  test('deduplicates a capability declared through aliases', () => {
    expect(
      computeFrontCapabilities({ ...base, declared: ['service', 'actor.call'] })
    ).toEqual(['service.call']);
  });

  test('accepts contract module ids in hostSupported', () => {
    expect(
      computeFrontCapabilities({
        ...base,
        declared: ['service'],
        hostSupported: new Set(['host:service']),
      })
    ).toEqual(['service.call']);
  });

  test('silently drops unknown capabilities', () => {
    expect(
      computeFrontCapabilities({ ...base, declared: ['unknown:thing'] })
    ).toEqual([]);
  });

  test('keeps third-party capabilities for the system audience', () => {
    expect(
      computeFrontCapabilities({
        ...base,
        declared: ['service'],
        audience: 'system',
      })
    ).toEqual(['service.call']);
  });
});

describe('capability model data', () => {
  test('every front capability has a definition', () => {
    for (const capability of FRONT_CAPABILITIES) {
      expect(FRONT_CAPABILITY_DEFINITIONS[capability]?.name).toBe(capability);
    }
  });

  test('host:net.request is default-off and broker-mediated', () => {
    expect(
      FRONT_CAPABILITY_DEFINITIONS['host:net.request'].defaultEnabled
    ).toBe(false);
    expect(FRONT_CAPABILITY_DEFINITIONS['host:net.request'].requiresBroker).toBe(
      true
    );
  });

  test('clipboard has a platform permission mapping', () => {
    expect(PLATFORM_PERMISSION_MAP['host:clipboard']).toBe('os:clipboard');
  });

  test('host:navigate.navigate is available without a grant', () => {
    expect(
      FRONT_CAPABILITY_DEFINITIONS['host:navigate.navigate'].defaultEnabled
    ).toBe(true);
    expect(
      computeFrontCapabilities(
        { ...base, declared: ['host:navigate.navigate'] },
        withImplemented('host:navigate.navigate')
      )
    ).toEqual(['host:navigate.navigate']);
  });

  test('host:external-link.open requires a user grant', () => {
    expect(
      FRONT_CAPABILITY_DEFINITIONS['host:external-link.open'].requiresGrant
    ).toBe('user');
    const definitions = withImplemented('host:external-link.open');
    expect(
      computeFrontCapabilities(
        { ...base, declared: ['host:external-link.open'] },
        definitions
      )
    ).toEqual([]);
    expect(
      computeFrontCapabilities(
        {
          ...base,
          declared: ['host:external-link.open'],
          grants: new Set(['host:external-link.open']),
        },
        definitions
      )
    ).toEqual(['host:external-link.open']);
  });

  test('only implemented capabilities count as host-supported', () => {
    expect(HOST_SUPPORTED_CAPABILITIES.size).toBe(2);
    expect(HOST_SUPPORTED_CAPABILITIES.has('service.call')).toBe(true);
    expect(HOST_SUPPORTED_CAPABILITIES.has('host:clipboard.write')).toBe(true);
    for (const capability of FRONT_CAPABILITIES) {
      if (HOST_SUPPORTED_CAPABILITIES.has(capability)) continue;
      expect(FRONT_CAPABILITY_DEFINITIONS[capability].implemented).toBe(false);
    }
  });

  test('placeholder capabilities never enter the effective set', () => {
    const placeholders = FRONT_CAPABILITIES.filter(
      (capability) => !FRONT_CAPABILITY_DEFINITIONS[capability].implemented
    );

    // A caller that reports everything as host-supported still cannot make a
    // placeholder callable; `implemented` is enforced by the function itself.
    expect(
      computeFrontCapabilities({
        ...base,
        declared: [...placeholders, 'host:clipboard.write'],
        hostSupported: all,
        grants: new Set([...placeholders, 'host:clipboard.write']),
      })
    ).toEqual(['host:clipboard.write']);
  });

  test('third-party effective sets never include system capabilities', () => {
    const definitions = withImplemented('system:diagnostics.view');

    expect(
      computeFrontCapabilities(
        {
          ...base,
          declared: ['system:diagnostics.view', 'system:diagnostics'],
          grants: new Set(['system:diagnostics.view']),
        },
        definitions
      )
    ).toEqual([]);

    expect(
      computeFrontCapabilities(
        {
          ...base,
          declared: ['system:diagnostics.view'],
          grants: new Set(['system:diagnostics.view']),
          audience: 'system',
        },
        definitions
      )
    ).toEqual(['system:diagnostics.view']);
  });
});

describe('computeServiceCapabilities', () => {
  const declared = [
    'workspace:read-write',
    'actor:call:notes.example:index-worker:1',
    'shell:bookmarks',
  ];

  test('intersects declared, host-supported, policy and session', () => {
    expect(
      computeServiceCapabilities({
        declared,
        hostSupported: new Set(declared),
        policy: new Set(declared),
        session: new Set(declared),
      })
    ).toEqual(declared);
  });

  test('drops malformed service capabilities', () => {
    const malformed = [
      'workspace:write',
      'net:http://insecure.example',
      'actor:call:notes.example:index-worker',
      'shell:unknown',
    ];
    expect(
      computeServiceCapabilities({
        declared: malformed,
        hostSupported: new Set(malformed),
        policy: new Set(malformed),
        session: new Set(malformed),
      })
    ).toEqual([]);
  });

  test('drops capabilities missing from policy or session', () => {
    expect(
      computeServiceCapabilities({
        declared,
        hostSupported: new Set(declared),
        policy: new Set(['workspace:read-write']),
        session: new Set(declared),
      })
    ).toEqual(['workspace:read-write']);
  });
});

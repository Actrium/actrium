/**
 * Front capability names and their Keel host-module mapping.
 *
 * Capabilities are method-level (v1 12 §4):
 *   `service.call`, `host:clipboard.write`, `host:net.request`, ...
 *
 * Two compatibility forms are accepted wherever a capability name is read:
 * - A module name (`service`, `host:clipboard`) means every method of that
 *   module; this keeps the v1 examples `["service", "host:notifications"]`
 *   working.
 * - A contract id (`host:service`, `host:clipboard.writeText`) maps back to
 *   the method-level capability; note `host:clipboard.write` is the capability
 *   name while `writeText` is the Keel export name.
 *
 * `ui.render` is intentionally absent: it is a component/rendering concern,
 * not a callable Front capability.
 */

export type CapabilitySpec = {
  /** Capability module, e.g. `service`, `host:clipboard`. */
  module: string;
  /** Keel host module id, e.g. `host:service`. */
  hostModule: string;
  /** Keel export name, e.g. `call`, `writeText`. */
  hostMethod: string;
};

export const FRONT_CAPABILITY_SPECS = {
  'service.call': {
    module: 'service',
    hostModule: 'host:service',
    hostMethod: 'call',
  },
  'service.tell': {
    module: 'service',
    hostModule: 'host:service',
    hostMethod: 'tell',
  },
  'service.on': {
    module: 'service',
    hostModule: 'host:service',
    hostMethod: 'on',
  },
  'host:assets.read': {
    module: 'host:assets',
    hostModule: 'host:assets',
    hostMethod: 'read',
  },
  'host:clipboard.write': {
    module: 'host:clipboard',
    hostModule: 'host:clipboard',
    hostMethod: 'writeText',
  },
  'host:notifications.show': {
    module: 'host:notifications',
    hostModule: 'host:notifications',
    hostMethod: 'show',
  },
  'host:camera.capturePhoto': {
    module: 'host:camera',
    hostModule: 'host:camera',
    hostMethod: 'capturePhoto',
  },
  'host:microphone.record': {
    module: 'host:microphone',
    hostModule: 'host:microphone',
    hostMethod: 'record',
  },
  'host:location.getCurrentPosition': {
    module: 'host:location',
    hostModule: 'host:location',
    hostMethod: 'getCurrentPosition',
  },
  'host:external-link.open': {
    module: 'host:external-link',
    hostModule: 'host:external-link',
    hostMethod: 'open',
  },
  'host:navigate.navigate': {
    module: 'host:navigate',
    hostModule: 'host:navigate',
    hostMethod: 'navigate',
  },
  'host:file-import.pick': {
    module: 'host:file-import',
    hostModule: 'host:file-import',
    hostMethod: 'pick',
  },
  'host:file-export.save': {
    module: 'host:file-export',
    hostModule: 'host:file-export',
    hostMethod: 'save',
  },
  'host:share.share': {
    module: 'host:share',
    hostModule: 'host:share',
    hostMethod: 'share',
  },
  'host:net.request': {
    module: 'host:net',
    hostModule: 'host:net',
    hostMethod: 'request',
  },
  'system:diagnostics.view': {
    module: 'system:diagnostics',
    hostModule: 'host:system-diagnostics',
    hostMethod: 'view',
  },
} as const satisfies Record<string, CapabilitySpec>;

export type FrontCapability = keyof typeof FRONT_CAPABILITY_SPECS;

export type FrontCapabilityModule =
  (typeof FRONT_CAPABILITY_SPECS)[FrontCapability]['module'];

export const FRONT_CAPABILITIES = Object.keys(
  FRONT_CAPABILITY_SPECS
) as readonly FrontCapability[];

export function capabilityModule(
  capability: FrontCapability
): FrontCapabilityModule {
  return FRONT_CAPABILITY_SPECS[capability].module;
}

export const FRONT_CAPABILITY_MODULES: readonly FrontCapabilityModule[] =
  Array.from(
    new Set(FRONT_CAPABILITIES.map((capability) => capabilityModule(capability)))
  );

export const CAPABILITY_TO_HOST_MODULE: Record<FrontCapability, string> =
  Object.fromEntries(
    FRONT_CAPABILITIES.map((capability) => [
      capability,
      FRONT_CAPABILITY_SPECS[capability].hostModule,
    ])
  ) as Record<FrontCapability, string>;

export const CAPABILITY_TO_HOST_METHOD: Record<FrontCapability, string> =
  Object.fromEntries(
    FRONT_CAPABILITIES.map((capability) => [
      capability,
      FRONT_CAPABILITY_SPECS[capability].hostMethod,
    ])
  ) as Record<FrontCapability, string>;

function buildCapabilitiesByModule(): Record<
  FrontCapabilityModule,
  readonly FrontCapability[]
> {
  const result = {} as Record<
    FrontCapabilityModule,
    readonly FrontCapability[]
  >;
  for (const module of FRONT_CAPABILITY_MODULES) {
    result[module] = FRONT_CAPABILITIES.filter(
      (capability) => FRONT_CAPABILITY_SPECS[capability].module === module
    );
  }
  return result;
}

export const CAPABILITIES_BY_MODULE = buildCapabilitiesByModule();

const CONTRACT_MODULE_TO_CAPABILITY_MODULE = new Map<
  string,
  FrontCapabilityModule
>();
const CONTRACT_CAPABILITY_TO_CAPABILITY = new Map<string, FrontCapability>();

for (const capability of FRONT_CAPABILITIES) {
  const spec = FRONT_CAPABILITY_SPECS[capability];
  CONTRACT_MODULE_TO_CAPABILITY_MODULE.set(spec.hostModule, spec.module);
  CONTRACT_CAPABILITY_TO_CAPABILITY.set(
    `${spec.hostModule}.${spec.hostMethod}`,
    capability
  );
}

/**
 * Legacy package names kept readable during the prototype migration.
 * `actor.call` was the pre-v1 name for the Service call capability.
 */
export const LEGACY_CAPABILITY_ALIASES: Record<string, string> = {
  'actor.call': 'service.call',
};

function hasOwn(value: object, key: string): boolean {
  return Object.prototype.hasOwnProperty.call(value, key);
}

/**
 * Expand any accepted spelling into the method-level capabilities it covers:
 * an exact capability, a module alias (all methods), a contract module id, or
 * a contract `module.method` id. Unknown names expand to an empty list.
 */
export function expandCapabilityName(name: string): FrontCapability[] {
  const aliased = LEGACY_CAPABILITY_ALIASES[name] ?? name;

  if (hasOwn(FRONT_CAPABILITY_SPECS, aliased)) {
    return [aliased as FrontCapability];
  }

  if (hasOwn(CAPABILITIES_BY_MODULE, aliased)) {
    return [...CAPABILITIES_BY_MODULE[aliased as FrontCapabilityModule]];
  }

  const capability = CONTRACT_CAPABILITY_TO_CAPABILITY.get(aliased);
  if (capability) {
    return [capability];
  }

  const module = CONTRACT_MODULE_TO_CAPABILITY_MODULE.get(aliased);
  if (module) {
    return [...CAPABILITIES_BY_MODULE[module]];
  }

  return [];
}

export function expandCapabilitySet(
  values: Iterable<string>
): Set<FrontCapability> {
  const result = new Set<FrontCapability>();
  for (const value of values) {
    for (const capability of expandCapabilityName(value)) {
      result.add(capability);
    }
  }
  return result;
}

export function hasDeclaredCapability(
  declared: readonly string[],
  capability: string
): boolean {
  const requested = expandCapabilityName(capability);
  if (requested.length === 0) return false;
  const declaredSet = expandCapabilitySet(declared);
  return requested.some((name) => declaredSet.has(name));
}

import { V1_COMPONENTS } from 'keel/v1';
import {
  expandCapabilityName,
  expandCapabilitySet,
  FRONT_CAPABILITIES,
  type FrontCapability,
} from './capabilities';
import { validateServiceCapabilities } from './service-capabilities';

/**
 * Capability layering and effective-set computation.
 *
 * The four layers follow v1 05 §7 / 06 §5.3:
 *   1. Front capability     — callable Host methods (`service.call`, `host:*`)
 *   2. Component whitelist  — Keel components a Guest may render
 *   3. Platform permission  — OS-level grants (camera/mic/location/notifications)
 *   4. Service capability   — workspace/network/actor/shell capabilities
 *
 * The attribute set follows `capability-tiers.zh.md` §4 (audience, grant,
 * defaults, broker, sandbox). Capability data is intentionally data-only so
 * the system-app audience can reuse the same structures later.
 */

export type CapabilityAudience = 'third_party' | 'system';
export type CapabilityGrant = 'none' | 'user' | 'admin' | 'elevated';
export type OsPermission = 'camera' | 'microphone' | 'location' | 'notifications';

export type CapabilityDefinition = {
  name: string;
  audience: CapabilityAudience;
  requiresGrant: CapabilityGrant;
  /** OS permission this capability needs, or undefined for non-OS capabilities. */
  osPermission?: OsPermission;
  /** Consumed at call time by the broker; not part of the effective set. */
  requiresBroker: boolean;
  /** Consumed by the sandbox at call time; not part of the effective set. */
  requiresSandbox: boolean;
  /**
   * True only once the Host has a working implementation. A capability can be
   * declared in the contract (builds accept its import) while still being a
   * throwing placeholder; such capabilities never enter the effective set.
   */
  implemented: boolean;
};

export const FRONT_CAPABILITY_DEFINITIONS: Record<
  FrontCapability,
  CapabilityDefinition
> = {
  'service.call': {
    name: 'service.call',
    audience: 'third_party',
    requiresGrant: 'none',
    requiresBroker: false,
    requiresSandbox: true,
    implemented: true,
  },
  'service.tell': {
    name: 'service.tell',
    audience: 'third_party',
    requiresGrant: 'none',
    requiresBroker: false,
    requiresSandbox: true,
    implemented: false,
  },
  'service.on': {
    name: 'service.on',
    audience: 'third_party',
    requiresGrant: 'none',
    requiresBroker: false,
    requiresSandbox: true,
    implemented: false,
  },
  'host:assets.read': {
    name: 'host:assets.read',
    audience: 'third_party',
    requiresGrant: 'none',
    requiresBroker: false,
    requiresSandbox: true,
    implemented: false,
  },
  'host:clipboard.write': {
    name: 'host:clipboard.write',
    audience: 'third_party',
    requiresGrant: 'user',
    requiresBroker: false,
    requiresSandbox: true,
    implemented: true,
  },
  'host:notifications.show': {
    name: 'host:notifications.show',
    audience: 'third_party',
    requiresGrant: 'user',
    osPermission: 'notifications',
    requiresBroker: false,
    requiresSandbox: true,
    implemented: false,
  },
  'host:camera.capturePhoto': {
    name: 'host:camera.capturePhoto',
    audience: 'third_party',
    requiresGrant: 'user',
    osPermission: 'camera',
    requiresBroker: false,
    requiresSandbox: true,
    implemented: false,
  },
  'host:microphone.record': {
    name: 'host:microphone.record',
    audience: 'third_party',
    requiresGrant: 'user',
    osPermission: 'microphone',
    requiresBroker: false,
    requiresSandbox: true,
    implemented: false,
  },
  'host:location.getCurrentPosition': {
    name: 'host:location.getCurrentPosition',
    audience: 'third_party',
    requiresGrant: 'user',
    osPermission: 'location',
    requiresBroker: false,
    requiresSandbox: true,
    implemented: false,
  },
  'host:external-link.open': {
    name: 'host:external-link.open',
    audience: 'third_party',
    requiresGrant: 'user',
    requiresBroker: false,
    requiresSandbox: true,
    implemented: false,
  },
  'host:navigate.navigate': {
    name: 'host:navigate.navigate',
    audience: 'third_party',
    requiresGrant: 'none',
    requiresBroker: false,
    requiresSandbox: true,
    implemented: false,
  },
  'host:file-import.pick': {
    name: 'host:file-import.pick',
    audience: 'third_party',
    requiresGrant: 'user',
    requiresBroker: false,
    requiresSandbox: true,
    implemented: false,
  },
  'host:file-export.save': {
    name: 'host:file-export.save',
    audience: 'third_party',
    requiresGrant: 'user',
    requiresBroker: false,
    requiresSandbox: true,
    implemented: false,
  },
  'host:share.share': {
    name: 'host:share.share',
    audience: 'third_party',
    requiresGrant: 'user',
    requiresBroker: false,
    requiresSandbox: true,
    implemented: false,
  },
  'host:net.request': {
    name: 'host:net.request',
    audience: 'third_party',
    requiresGrant: 'user',
    requiresBroker: true,
    requiresSandbox: true,
    implemented: false,
  },
  'system:diagnostics.view': {
    name: 'system:diagnostics.view',
    audience: 'system',
    requiresGrant: 'admin',
    requiresBroker: false,
    requiresSandbox: true,
    implemented: false,
  },
};

/**
 * Capabilities the Host can actually serve. Placeholder capabilities are
 * declared in the contract so builds accept their imports, but they stay out
 * of the effective set until implemented (see `implemented` above).
 */
export const HOST_SUPPORTED_CAPABILITIES: ReadonlySet<FrontCapability> =
  new Set(
    FRONT_CAPABILITIES.filter(
      (capability) => FRONT_CAPABILITY_DEFINITIONS[capability].implemented
    )
  );

// Keep the Host registry policy and the Product v1 compiler on the same list.
export const COMPONENT_WHITELIST = V1_COMPONENTS;

export type AllowedComponent = (typeof COMPONENT_WHITELIST)[number];

export type FrontCapabilityInput = {
  declared: readonly string[];
  /**
   * Method-level names (`service.call`, `host:clipboard.write`), module
   * shorthands (`service`, `host:clipboard`) and contract ids are accepted.
   * Placeholder capabilities are filtered by `implemented` even when they
   * appear here; production callers should still pass a real support set such
   * as `HOST_SUPPORTED_CAPABILITIES`.
   */
  hostSupported: ReadonlySet<string>;
  /** Reserved for native Hosts; v1 currently opens JS Guests only. */
  guestKind: 'js' | 'native';
  /** Method-level capabilities whose required grant has been satisfied. */
  grants: ReadonlySet<string>;
  /** OS permissions currently granted by the platform. */
  platform: ReadonlySet<OsPermission>;
  session: {
    state: 'active' | 'suspended' | 'expired' | 'detached';
    allowedCapabilities: ReadonlySet<string>;
  };
  audience: CapabilityAudience;
};

export type FrontCapabilityDefinitions = Record<
  FrontCapability,
  CapabilityDefinition
>;

/**
 * effective = declared ∩ host-supported ∩ audience ∩ grant ∩ platform ∩ session.
 * Parsing route names stays opaque; the result is the callable, method-level set.
 *
 * A throwing placeholder capability never enters the set: `definition.implemented`
 * is enforced here rather than trusting the caller to pass a narrow
 * `hostSupported`. `definitions` exists so policy tests can use a fixture;
 * production callers must leave it at the Host's real capability table.
 */
export function computeFrontCapabilities(
  input: FrontCapabilityInput,
  definitions: FrontCapabilityDefinitions = FRONT_CAPABILITY_DEFINITIONS
): FrontCapability[] {
  const supported = expandCapabilitySet(input.hostSupported);
  const session = expandCapabilitySet(input.session.allowedCapabilities);
  const activeSession = input.session.state === 'active';

  const result: FrontCapability[] = [];
  const seen = new Set<FrontCapability>();

  for (const declared of input.declared) {
    for (const capability of expandCapabilityName(declared)) {
      if (seen.has(capability)) continue;
      const definition = definitions[capability];
      if (!definition) continue;
      if (!definition.implemented) continue;
      if (!supported.has(capability)) continue;
      if (definition.audience === 'system' && input.audience !== 'system') continue;
      // Grants are persisted per canonical method. Never expand a module alias
      // here, otherwise a future method could inherit an older module grant.
      if (definition.requiresGrant !== 'none' && !input.grants.has(capability)) {
        continue;
      }
      if (definition.osPermission && !input.platform.has(definition.osPermission)) {
        continue;
      }
      if (!activeSession || !session.has(capability)) continue;
      seen.add(capability);
      result.push(capability);
    }
  }

  return result;
}

export type ServiceCapabilityInput = {
  declared: readonly string[];
  hostSupported: ReadonlySet<string>;
  policy: ReadonlySet<string>;
  session: ReadonlySet<string>;
};

/**
 * Service capabilities are per-instance. v1 currently opens JS Guests only, so
 * the result is the third-party set; system instances will add their own
 * audience rule when that path exists.
 */
export function computeServiceCapabilities(
  input: ServiceCapabilityInput
): string[] {
  return validateServiceCapabilities(input.declared).valid.filter(
    (capability) =>
      input.hostSupported.has(capability) &&
      input.policy.has(capability) &&
      input.session.has(capability)
  );
}

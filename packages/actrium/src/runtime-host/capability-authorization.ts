import {
  expandCapabilitySet,
  type FrontCapability,
} from './capabilities';
import {
  computeFrontCapabilities,
  FRONT_CAPABILITY_DEFINITIONS,
  HOST_SUPPORTED_CAPABILITIES,
  type CapabilityAudience,
  type FrontCapabilityDefinitions,
  type OsPermission,
} from './capability-model';

export type CapabilityGrantStatus =
  | 'not_required'
  | 'prompt'
  | 'granted'
  | 'denied'
  | 'restricted';

export type PlatformPermissionStatus =
  | 'not_applicable'
  | 'prompt'
  | 'granted'
  | 'denied'
  | 'restricted'
  | 'unavailable';

export type CapabilitySessionState =
  | 'active'
  | 'suspended'
  | 'expired'
  | 'detached';

export type ProgramSubject = {
  publisher: string;
  program: string;
  majorVersion: number;
  packageDigest: string;
};

/**
 * Snapshot supplied by the trusted Native Front Host. Keel consumes this as
 * data and never writes the authoritative permission store.
 */
export type NativeCapabilitySnapshot = {
  schemaVersion: 1;
  revision: number;
  subject: ProgramSubject;
  audience: CapabilityAudience;
  declaredCapabilities: string[];
  grants: Record<string, CapabilityGrantStatus>;
  platform: Partial<Record<OsPermission, PlatformPermissionStatus>>;
  session: {
    generation: number;
    state: CapabilitySessionState;
    allowedCapabilities: string[];
  };
};

export type CapabilityDenialCode =
  | 'capability_not_declared'
  | 'capability_not_implemented'
  | 'capability_not_supported'
  | 'capability_audience_denied'
  | 'permission_required'
  | 'permission_denied'
  | 'permission_restricted'
  | 'platform_permission_required'
  | 'platform_permission_denied'
  | 'platform_permission_restricted'
  | 'platform_unavailable'
  | 'session_capability_denied'
  | 'session_expired'
  | 'session_detached';

export type CapabilityAccessDecision =
  | { allowed: true; capability: FrontCapability }
  | {
      allowed: false;
      capability: FrontCapability;
      code: CapabilityDenialCode;
    };

export function grantStatusFor(
  capability: FrontCapability,
  snapshot: NativeCapabilitySnapshot,
  definitions: FrontCapabilityDefinitions = FRONT_CAPABILITY_DEFINITIONS
): CapabilityGrantStatus {
  const definition = definitions[capability];
  if (!definition || definition.requiresGrant === 'none') {
    return 'not_required';
  }
  return snapshot.grants[capability] ?? 'prompt';
}

function grantedCapabilities(
  snapshot: NativeCapabilitySnapshot,
  definitions: FrontCapabilityDefinitions
): ReadonlySet<string> {
  const result = new Set<string>();
  for (const capability of Object.keys(definitions) as FrontCapability[]) {
    const status = grantStatusFor(capability, snapshot, definitions);
    if (status === 'not_required' || status === 'granted') {
      result.add(capability);
    }
  }
  return result;
}

function grantedOsPermissions(
  snapshot: NativeCapabilitySnapshot
): ReadonlySet<OsPermission> {
  return new Set(
    Object.entries(snapshot.platform)
      .filter(([, status]) => status === 'granted')
      .map(([permission]) => permission as OsPermission)
  );
}

function computeEffective(
  snapshot: NativeCapabilitySnapshot,
  definitions: FrontCapabilityDefinitions,
  hostSupported: ReadonlySet<string>
): FrontCapability[] {
  return computeFrontCapabilities(
    {
      declared: snapshot.declaredCapabilities,
      hostSupported,
      guestKind: 'js',
      grants: grantedCapabilities(snapshot, definitions),
      platform: grantedOsPermissions(snapshot),
      session: {
        state: snapshot.session.state,
        allowedCapabilities: new Set(snapshot.session.allowedCapabilities),
      },
      audience: snapshot.audience,
    },
    definitions
  );
}

export function effectiveCapabilities(
  snapshot: NativeCapabilitySnapshot,
  options: {
    hostSupported?: ReadonlySet<string>;
    definitions?: FrontCapabilityDefinitions;
  } = {}
): FrontCapability[] {
  return computeEffective(
    snapshot,
    options.definitions ?? FRONT_CAPABILITY_DEFINITIONS,
    options.hostSupported ?? HOST_SUPPORTED_CAPABILITIES
  );
}

/**
 * Per-call authorization. The effective set is only a projection; callers must
 * evaluate every protected Host method against the current snapshot.
 */
export function evaluateCapabilityAccess(
  capability: FrontCapability,
  snapshot: NativeCapabilitySnapshot,
  options: {
    hostSupported?: ReadonlySet<string>;
    definitions?: FrontCapabilityDefinitions;
  } = {}
): CapabilityAccessDecision {
  const definitions = options.definitions ?? FRONT_CAPABILITY_DEFINITIONS;
  const hostSupported =
    options.hostSupported ?? HOST_SUPPORTED_CAPABILITIES;
  const definition = definitions[capability];
  if (!definition) {
    return { allowed: false, capability, code: 'capability_not_supported' };
  }

  const declared = expandCapabilitySet(snapshot.declaredCapabilities);
  if (!declared.has(capability)) {
    return { allowed: false, capability, code: 'capability_not_declared' };
  }
  if (!definition.implemented || !hostSupported.has(capability)) {
    return { allowed: false, capability, code: 'capability_not_implemented' };
  }
  if (definition.audience === 'system' && snapshot.audience !== 'system') {
    return { allowed: false, capability, code: 'capability_audience_denied' };
  }

  const grant = grantStatusFor(capability, snapshot, definitions);
  if (grant === 'prompt') {
    return { allowed: false, capability, code: 'permission_required' };
  }
  if (grant === 'denied') {
    return { allowed: false, capability, code: 'permission_denied' };
  }
  if (grant === 'restricted') {
    return { allowed: false, capability, code: 'permission_restricted' };
  }

  const osPermission = definition.osPermission;
  if (osPermission) {
    const platform = snapshot.platform[osPermission] ?? 'prompt';
    if (platform === 'prompt') {
      return {
        allowed: false,
        capability,
        code: 'platform_permission_required',
      };
    }
    if (platform === 'denied') {
      return {
        allowed: false,
        capability,
        code: 'platform_permission_denied',
      };
    }
    if (platform === 'restricted') {
      return {
        allowed: false,
        capability,
        code: 'platform_permission_restricted',
      };
    }
    if (platform === 'unavailable') {
      return { allowed: false, capability, code: 'platform_unavailable' };
    }
  }

  if (snapshot.session.state === 'expired') {
    return { allowed: false, capability, code: 'session_expired' };
  }
  if (snapshot.session.state === 'detached') {
    return { allowed: false, capability, code: 'session_detached' };
  }
  if (snapshot.session.state !== 'active') {
    return { allowed: false, capability, code: 'session_capability_denied' };
  }
  const sessionAllowed = expandCapabilitySet(
    snapshot.session.allowedCapabilities
  );
  if (!sessionAllowed.has(capability)) {
    return { allowed: false, capability, code: 'session_capability_denied' };
  }
  if (!effectiveCapabilities(snapshot, options).includes(capability)) {
    return { allowed: false, capability, code: 'capability_not_supported' };
  }

  return { allowed: true, capability };
}

export function capabilitiesFromSnapshotJSON(
  json: string
): NativeCapabilitySnapshot | null {
  try {
    const value: unknown = JSON.parse(json);
    if (!value || typeof value !== 'object') return null;
    const snapshot = value as Partial<NativeCapabilitySnapshot>;
    if (
      snapshot.schemaVersion !== 1 ||
      typeof snapshot.revision !== 'number' ||
      !snapshot.subject ||
      !Array.isArray(snapshot.declaredCapabilities) ||
      !snapshot.grants ||
      !snapshot.platform ||
      !snapshot.session ||
      !Array.isArray(snapshot.session.allowedCapabilities)
    ) {
      return null;
    }
    return snapshot as NativeCapabilitySnapshot;
  } catch {
    return null;
  }
}

export class CapabilityAccessError extends Error {
  readonly code: CapabilityDenialCode;
  readonly capability: FrontCapability;

  constructor(capability: FrontCapability, code: CapabilityDenialCode) {
    super(`${capability}: ${code}`);
    this.name = 'CapabilityAccessError';
    this.capability = capability;
    this.code = code;
  }
}

type GrantWaiter = {
  capability: FrontCapability;
  resolve: () => void;
  reject: (error: Error) => void;
  timer: ReturnType<typeof setTimeout>;
};

function sameSubject(a: ProgramSubject, b: ProgramSubject): boolean {
  return (
    a.publisher === b.publisher &&
    a.program === b.program &&
    a.majorVersion === b.majorVersion &&
    a.packageDigest === b.packageDigest
  );
}

/**
 * Per-Tab authorization coordinator. It keeps the latest Native snapshot and
 * waits for a new revision only when the Native Shell needs user interaction.
 */
export class CapabilityAuthorizer {
  private snapshot: NativeCapabilitySnapshot;
  private readonly requestGrant: (capability: FrontCapability) => void;
  private readonly waiters = new Map<FrontCapability, GrantWaiter[]>();
  private disposed = false;

  constructor(
    snapshot: NativeCapabilitySnapshot,
    requestGrant: (capability: FrontCapability) => void
  ) {
    this.snapshot = snapshot;
    this.requestGrant = requestGrant;
  }

  update(snapshot: NativeCapabilitySnapshot): void {
    if (this.disposed) return;
    const subjectChanged = !sameSubject(
      snapshot.subject,
      this.snapshot.subject
    );
    if (!subjectChanged && snapshot.revision <= this.snapshot.revision) return;
    this.snapshot = snapshot;
    for (const [capability, waiters] of [...this.waiters]) {
      if (subjectChanged) {
        this.waiters.delete(capability);
        for (const waiter of waiters) {
          clearTimeout(waiter.timer);
          waiter.reject(
            new CapabilityAccessError(capability, 'session_detached')
          );
        }
        continue;
      }
      const decision = evaluateCapabilityAccess(capability, snapshot);
      if (decision.allowed || decision.code !== 'permission_required') {
        this.waiters.delete(capability);
        for (const waiter of waiters) {
          clearTimeout(waiter.timer);
          if (decision.allowed) {
            waiter.resolve();
          } else {
            waiter.reject(new CapabilityAccessError(capability, decision.code));
          }
        }
      }
    }
  }

  async authorize(capability: FrontCapability): Promise<void> {
    if (this.disposed) {
      throw new CapabilityAccessError(capability, 'session_detached');
    }
    const decision = evaluateCapabilityAccess(capability, this.snapshot);
    if (decision.allowed) return;
    if (decision.code !== 'permission_required') {
      throw new CapabilityAccessError(capability, decision.code);
    }

    await this.waitForGrant(capability);
    const resolved = evaluateCapabilityAccess(capability, this.snapshot);
    if (!resolved.allowed) {
      throw new CapabilityAccessError(capability, resolved.code ?? 'permission_denied');
    }
  }

  dispose(): void {
    if (this.disposed) return;
    this.disposed = true;
    for (const waiters of this.waiters.values()) {
      for (const waiter of waiters) {
        clearTimeout(waiter.timer);
        waiter.reject(
          new CapabilityAccessError(waiter.capability, 'session_detached')
        );
      }
    }
    this.waiters.clear();
  }

  private waitForGrant(capability: FrontCapability): Promise<void> {
    return new Promise((resolve, reject) => {
      const waiter: GrantWaiter = {
        capability,
        resolve,
        reject,
        timer: setTimeout(() => {
          this.removeWaiter(capability, waiter);
          reject(new CapabilityAccessError(capability, 'permission_required'));
        }, 30_000),
      };
      this.waiters.set(capability, [
        ...(this.waiters.get(capability) ?? []),
        waiter,
      ]);
      try {
        this.requestGrant(capability);
      } catch (cause) {
        clearTimeout(waiter.timer);
        this.removeWaiter(capability, waiter);
        reject(cause instanceof Error ? cause : new Error(String(cause)));
      }
    });
  }

  private removeWaiter(capability: FrontCapability, waiter: GrantWaiter): void {
    const remaining = (this.waiters.get(capability) ?? []).filter(
      (candidate) => candidate !== waiter
    );
    if (remaining.length > 0) {
      this.waiters.set(capability, remaining);
    } else {
      this.waiters.delete(capability);
    }
  }
}

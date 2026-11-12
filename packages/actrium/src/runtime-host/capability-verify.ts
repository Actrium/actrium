import { resolveDeclaredCapability } from './capabilities';
import { COMPONENT_WHITELIST } from './capability-model';

/**
 * Compare the capability manifest emitted at build time with what the Host
 * allows. Inputs are already-unpacked manifest data or keel build artifacts;
 * signature, archive and hash verification stay in the Rust side.
 */

export type CapabilityManifestV1 = {
  schemaVersion: number;
  keelApi: string;
  guestEntry: string;
  imports: string[];
  hostModules: Record<string, { rpc?: string[]; events?: string[] }>;
  components: string[];
};

export type CapabilityVerificationCode =
  | 'capability_mismatch'
  | 'unknown_capability'
  | 'component_not_allowed';

export type CapabilityVerificationResult = {
  ok: boolean;
  code?: CapabilityVerificationCode;
  details: string[];
};

export function verifyCapabilityManifest(
  manifest: CapabilityManifestV1,
  options: {
    allowedComponents?: readonly string[];
    /** Per-app `[actrium.components].allow` from the package manifest. */
    appAllowComponents?: readonly string[];
    allowedImports?: ReadonlySet<string>;
    hostSupportedModules?: ReadonlySet<string>;
    contractKinds?: Record<string, Record<string, 'rpc' | 'subscription'>>;
    /** Allowed business routes/events for `host:service`. */
    serviceCapabilities?: {
      rpc?: ReadonlySet<string>;
      events?: ReadonlySet<string>;
    };
    declaredCapabilities?: readonly string[];
  } = {}
): CapabilityVerificationResult {
  const details: string[] = [];
  const fail = (
    code: CapabilityVerificationCode,
    detail: string
  ): CapabilityVerificationResult => {
    details.push(detail);
    return { ok: false, code, details };
  };

  if (manifest.schemaVersion !== 1) {
    return fail(
      'capability_mismatch',
      `Unsupported capability manifest schemaVersion ${manifest.schemaVersion}.`
    );
  }

  const hostAllow = new Set(options.allowedComponents ?? COMPONENT_WHITELIST);
  if (options.appAllowComponents) {
    for (const component of options.appAllowComponents) {
      if (!hostAllow.has(component)) {
        return fail(
          'component_not_allowed',
          `Component "${component}" is not in the host allowlist.`
        );
      }
    }
  }

  // usage ⊆ app allow ⊆ host allow (v1 12 §4).
  const appAllow = new Set(options.appAllowComponents ?? hostAllow);
  const allowlistLabel = options.appAllowComponents
    ? 'app allowlist'
    : 'host allowlist';
  for (const component of manifest.components) {
    if (!appAllow.has(component)) {
      return fail(
        'component_not_allowed',
        `Component "${component}" is not in the ${allowlistLabel}.`
      );
    }
  }

  if (options.hostSupportedModules) {
    for (const importName of manifest.imports) {
      if (
        importName.startsWith('host:') &&
        !options.hostSupportedModules.has(importName)
      ) {
        return fail(
          'unknown_capability',
          `Host module "${importName}" is not supported.`
        );
      }
    }
  }

  if (options.allowedImports) {
    for (const importName of manifest.imports) {
      if (importName.startsWith('host:')) continue;
      if (!options.allowedImports.has(importName)) {
        return fail(
          'unknown_capability',
          `Import "${importName}" is not allowed.`
        );
      }
    }
  }

  // v1 keys hostModules by capability name (`service`); older manifests may
  // still use the contract module id (`host:service`).
  const toModuleId = (key: string) =>
    key.startsWith('host:') ? key : `host:${key}`;

  const importedHostModules = new Set(
    manifest.imports.filter((name) => name.startsWith('host:'))
  );
  for (const rawKey of Object.keys(manifest.hostModules)) {
    const moduleId = toModuleId(rawKey);
    if (!importedHostModules.has(moduleId)) {
      return fail(
        'capability_mismatch',
        `Host module "${moduleId}" is used but not imported.`
      );
    }
  }

  if (options.contractKinds) {
    for (const [rawKey, entry] of Object.entries(manifest.hostModules)) {
      const moduleId = toModuleId(rawKey);
      if (moduleId === 'host:service') {
        // `host:service` entries are business routes/events, not transport
        // exports (`call`/`tell`/`on`). Validate them against the service
        // route policy when one is supplied.
        if (options.serviceCapabilities) {
          for (const route of entry.rpc ?? []) {
            if (
              options.serviceCapabilities.rpc &&
              !options.serviceCapabilities.rpc.has(route)
            ) {
              return fail(
                'capability_mismatch',
                `Service route "${route}" is not allowed.`
              );
            }
          }
          for (const event of entry.events ?? []) {
            if (
              options.serviceCapabilities.events &&
              !options.serviceCapabilities.events.has(event)
            ) {
              return fail(
                'capability_mismatch',
                `Service event "${event}" is not allowed.`
              );
            }
          }
        }
        continue;
      }
      const kinds = options.contractKinds[moduleId];
      if (!kinds) {
        return fail(
          'capability_mismatch',
          `Host module "${moduleId}" is not in the contract.`
        );
      }
      for (const rpcName of entry.rpc ?? []) {
        if (kinds[rpcName] !== 'rpc') {
          return fail(
            'capability_mismatch',
            `"${moduleId}.${rpcName}" is not an rpc in the contract.`
          );
        }
      }
      for (const eventName of entry.events ?? []) {
        if (kinds[eventName] !== 'subscription') {
          return fail(
            'capability_mismatch',
            `"${moduleId}.${eventName}" is not a subscription in the contract.`
          );
        }
      }
    }
  }

  if (options.declaredCapabilities) {
    const declared = new Set(
      options.declaredCapabilities
        .map(resolveDeclaredCapability)
        .filter((capability): capability is NonNullable<typeof capability> =>
          capability !== undefined
        )
    );
    for (const rawKey of Object.keys(manifest.hostModules)) {
      const moduleId = toModuleId(rawKey);
      const capability = moduleId === 'host:service' ? 'service' : moduleId;
      const resolved = resolveDeclaredCapability(capability);
      if (!resolved || !declared.has(resolved)) {
        return fail(
          'capability_mismatch',
          `Capability "${capability}" is used but not declared by the package.`
        );
      }
    }
    // Importing a host module is itself a capability acquisition, even when
    // the manifest records no calls or events for it.
    for (const importName of manifest.imports) {
      if (!importName.startsWith('host:')) continue;
      const capability =
        importName === 'host:service' ? 'service' : importName;
      const resolved = resolveDeclaredCapability(capability);
      if (!resolved || !declared.has(resolved)) {
        return fail(
          'capability_mismatch',
          `Capability "${capability}" is imported but not declared by the package.`
        );
      }
    }
  }

  return { ok: true, details };
}

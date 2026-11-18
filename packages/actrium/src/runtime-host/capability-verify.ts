import { capabilityModule, expandCapabilityName } from './capabilities';
import { COMPONENT_WHITELIST } from './capability-model';

/**
 * Fixed `host:service` exports and the capability each one requires. Any other
 * named export in that module is a business route or event and is checked
 * against the service route ACL instead. Keep this explicit: inferring
 * transports from `expandCapabilityName` would make module shorthands
 * indistinguishable from real transport names.
 */
const SERVICE_TRANSPORT_CAPABILITIES: ReadonlyMap<string, string> = new Map([
  ['call', 'service.call'],
  ['tell', 'service.tell'],
  ['on', 'service.on'],
  ['uploadAttachment', 'service.attachmentUpload'],
]);

/**
 * Compare the capability manifest emitted at build time with what the Host
 * allows. Inputs are already-unpacked manifest data or keel build artifacts;
 * signature, archive and hash verification stay in the Rust side.
 *
 * This is the verifier half of the capability-manifest pipeline. The native
 * package loader does not call it yet: runtime enforcement lands with the
 * Attach/session work, and until then the loader only applies the declared
 * `permissions` check.
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
    hostSupportedMethods?: Record<string, ReadonlySet<string>>;
    contractKinds?: Record<string, Record<string, 'rpc' | 'subscription'>>;
    /** Allowed business routes/events for `host:service`. */
    serviceCapabilities?: {
      rpc: ReadonlySet<string>;
      events: ReadonlySet<string>;
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

  if (options.contractKinds || options.serviceCapabilities) {
    for (const [rawKey, entry] of Object.entries(manifest.hostModules)) {
      const moduleId = toModuleId(rawKey);
      if (moduleId === 'host:service') {
        // `host:service` entries are business routes/events or fixed
        // transport exports (`call`/`tell`/`on`/`uploadAttachment`).
        // Transport exports never enter the business route ACL; business
        // routes are validated against the service policy whenever one is
        // supplied, independently of `contractKinds`.
        if (options.serviceCapabilities) {
          for (const route of entry.rpc ?? []) {
            if (SERVICE_TRANSPORT_CAPABILITIES.has(route)) continue;
            if (!options.serviceCapabilities.rpc.has(route)) {
              return fail(
                'capability_mismatch',
                `Service route "${route}" is not allowed.`
              );
            }
          }
          for (const event of entry.events ?? []) {
            if (SERVICE_TRANSPORT_CAPABILITIES.has(event)) continue;
            if (!options.serviceCapabilities.events.has(event)) {
              return fail(
                'capability_mismatch',
                `Service event "${event}" is not allowed.`
              );
            }
          }
        }
        continue;
      }
      if (!options.contractKinds) continue;
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

  if (options.hostSupportedMethods) {
    for (const [rawKey, entry] of Object.entries(manifest.hostModules)) {
      const moduleId = toModuleId(rawKey);
      // `host:service` entries are business routes/events or fixed transport
      // exports (`call`/`tell`/`on`/`uploadAttachment`). Business routes are
      // deferred to `serviceCapabilities`; fixed transports must actually be
      // implemented by the Host.
      if (moduleId === 'host:service') {
        const supported = options.hostSupportedMethods['host:service'];
        for (const transportName of [
          ...(entry.rpc ?? []),
          ...(entry.events ?? []),
        ]) {
          if (!SERVICE_TRANSPORT_CAPABILITIES.has(transportName)) continue;
          if (!supported?.has(transportName)) {
            return fail(
              'unknown_capability',
              `Host method "host:service.${transportName}" is not implemented.`
            );
          }
        }
        if ((entry.events?.length ?? 0) > 0 && !supported?.has('on')) {
          return fail(
            'unknown_capability',
            'Host method "host:service.on" is not implemented.'
          );
        }
        continue;
      }
      const supportedMethods = options.hostSupportedMethods[moduleId];
      if (!supportedMethods) {
        return fail(
          'unknown_capability',
          `Host module "${moduleId}" is not supported.`
        );
      }
      for (const methodName of [
        ...(entry.rpc ?? []),
        ...(entry.events ?? []),
      ]) {
        if (!supportedMethods.has(methodName)) {
          return fail(
            'unknown_capability',
            `Host method "${moduleId}.${methodName}" is not implemented.`
          );
        }
      }
    }
  }

  if (options.declaredCapabilities) {
    const declared = new Set<string>();
    const declaredModules = new Set<string>();
    for (const capability of options.declaredCapabilities) {
      for (const methodCapability of expandCapabilityName(capability)) {
        declared.add(methodCapability);
        declaredModules.add(capabilityModule(methodCapability));
      }
    }

    for (const [rawKey, entry] of Object.entries(manifest.hostModules)) {
      const moduleId = toModuleId(rawKey);
      if (moduleId === 'host:service') {
        // Fixed transport exports (`call` / `tell` / `on` /
        // `uploadAttachment`) expand from `host:service.<name>` and need
        // their exact capability; business routes do not expand and instead
        // require the transport method capability explicitly.
        const transportNames = new Set<string>();
        for (const transportName of [
          ...(entry.rpc ?? []),
          ...(entry.events ?? []),
        ]) {
          const required = SERVICE_TRANSPORT_CAPABILITIES.get(transportName);
          if (!required) continue;
          transportNames.add(transportName);
          if (!declared.has(required)) {
            return fail(
              'capability_mismatch',
              `Capability "${required}" is used but not declared by the package.`
            );
          }
        }
        const businessRoutes = (entry.rpc ?? []).filter(
          (route) => !transportNames.has(route)
        );
        const businessEvents = (entry.events ?? []).filter(
          (event) => !transportNames.has(event)
        );
        if (businessRoutes.length > 0 && !declared.has('service.call')) {
          return fail(
            'capability_mismatch',
            'Capability "service.call" is used but not declared by the package.'
          );
        }
        if (businessEvents.length > 0 && !declared.has('service.on')) {
          return fail(
            'capability_mismatch',
            'Capability "service.on" is used but not declared by the package.'
          );
        }
        continue;
      }
      if (!declaredModules.has(moduleId)) {
        return fail(
          'capability_mismatch',
          `Capability "${moduleId}" is used but not declared by the package.`
        );
      }

      // Method-level check when the manifest names a known contract export.
      for (const methodName of [
        ...(entry.rpc ?? []),
        ...(entry.events ?? []),
      ]) {
        const capabilities = expandCapabilityName(`${moduleId}.${methodName}`);
        if (capabilities.length === 0) continue;
        if (!capabilities.some((capability) => declared.has(capability))) {
          return fail(
            'capability_mismatch',
            `Capability "${moduleId}.${methodName}" is used but not declared by the package.`
          );
        }
      }
    }
    // Importing a host module is itself a capability acquisition, even when
    // the manifest records no calls or events for it. `host:service` is
    // satisfied by any capability of the service module: upload-only Guests
    // still import the module while declaring `service.attachmentUpload`
    // explicitly instead of the `service` shorthand.
    for (const importName of manifest.imports) {
      if (!importName.startsWith('host:')) continue;
      const capability =
        importName === 'host:service' ? 'service' : importName;
      const resolved = expandCapabilityName(capability);
      const satisfied =
        resolved.some((name) => declared.has(name)) ||
        (resolved.length > 0 &&
          declaredModules.has(capabilityModule(resolved[0])));
      if (resolved.length === 0 || !satisfied) {
        return fail(
          'capability_mismatch',
          `Capability "${capability}" is imported but not declared by the package.`
        );
      }
    }
  }

  return { ok: true, details };
}

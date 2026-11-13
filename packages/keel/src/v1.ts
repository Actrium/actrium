import { isHostModuleId, type KeelContractShape } from './contract';

/** Components exposed by the Actrium Product v1 profile. */
export const V1_COMPONENTS = [
  'View',
  'Text',
  'ScrollView',
  'TextInput',
  'Button',
  'FlatList',
  'ImageAsset',
  'Switch',
] as const;

/** Runtime modules with no Actrium capability of their own. */
export const V1_RUNTIME_IMPORTS = ['react', 'react/jsx-runtime', 'react/jsx-dev-runtime'] as const;

/** Named Guest SDK exports available without adding a component capability. */
const V1_SDK_UTILITIES = new Set([
  'StyleSheet',
  'createUtf8Decoder',
  'createUtf8Encoder',
  'decodeBase64',
  'encodeBase64',
  'nowMs',
  'useConfig',
]);

/** Marker for the inert first line of the versioned v1 bundle. */
const HEADER = '// keel:v1-linkage ';

/** Source module prefix for a fixed business route or event. */
export const V1_SERVICE_PREFIX = 'host:service/';

/** Dotted, bounded business names that can be encoded as Keel module IDs. */
const SERVICE_NAME = /^[A-Za-z_][A-Za-z0-9_-]*(\.[A-Za-z_][A-Za-z0-9_-]*)+$/;

/** One external module or one named export from a capability-bearing module. */
export type V1Link = [moduleId: string] | [moduleId: string, exportName: string];

/** The linkage inputs that both the builder and the consumer verify. */
export type V1Linkage = { guestEntry: string; links: V1Link[] };

/** Public capability manifest defined by Actrium Product v1. */
export type V1CapabilityManifest = {
  schemaVersion: 1;
  keelApi: '1.0';
  guestEntry: string;
  imports: string[];
  hostModules: Record<string, { rpc?: string[]; events?: string[] }>;
  components: string[];
};

/** Return the protocol name for a Guest SDK component export. */
export function v1ComponentName(exportName: string): string | undefined {
  // The public SDK currently emits Image while the v1 manifest names ImageAsset.
  if (exportName === 'Image') return 'ImageAsset';
  return V1_COMPONENTS.find((name) => name === exportName && name !== 'ImageAsset');
}

/** Parse a source-level service link into its fixed transport operation and name. */
export function v1ServiceLink(moduleId: string, exportName: string) {
  const name = moduleId.slice(V1_SERVICE_PREFIX.length);
  // Restrict route names and operations before building a Host dispatch key.
  if (
    !moduleId.startsWith(V1_SERVICE_PREFIX) ||
    name.length > 256 ||
    !SERVICE_NAME.test(name) ||
    (exportName !== 'call' && exportName !== 'tell' && exportName !== 'on')
  ) {
    throw new Error(`capability_mismatch: invalid fixed route or event ${moduleId}.${exportName}`);
  }
  return { name, operation: exportName };
}

/** Encode the case-sensitive business name in the existing Keel module ID grammar. */
export function v1ServiceDispatchId(moduleId: string, exportName: string): `host:${string}` {
  const { name } = v1ServiceLink(moduleId, exportName);
  const encoded = Array.from(name, (character) =>
    character.charCodeAt(0).toString(16).padStart(2, '0')
  ).join('');
  return `host:service/${encoded}`;
}

/** Reject path spellings that cannot denote an entry in the signed package. */
function checkGuestEntry(entry: string): void {
  // Require a relative, slash-separated path with no traversal or line terminators.
  if (
    /[\\:\u007f\u2028\u2029]/u.test(entry) ||
    [...entry].some((character) => character < ' ') ||
    entry.split('/').some((segment) => !segment || segment === '.' || segment === '..')
  ) {
    throw new Error('capability_mismatch: guestEntry must be a normalized package-relative path');
  }
}

/** Convert a validated linkage into the public manifest using the Host contract. */
export function createV1CapabilityManifest(
  linkage: V1Linkage,
  contract: KeelContractShape
): V1CapabilityManifest {
  checkGuestEntry(linkage.guestEntry);
  const imports = new Set<string>();
  const components = new Set<string>();
  const modules = new Map<string, { rpc: Set<string>; events: Set<string> }>();
  for (const [moduleId, exportName] of linkage.links) {
    // React imports do not confer an Actrium Host capability.
    if (V1_RUNTIME_IMPORTS.some((id) => id === moduleId) && exportName === undefined) {
      imports.add(moduleId);
      continue;
    }
    // Record the SDK component at import time, independently of how it is rendered.
    if (moduleId === 'keel/guest' && exportName !== undefined) {
      const component = v1ComponentName(exportName);
      // An unrecognized SDK export may expose a component outside the v1 registry.
      if (!component && !V1_SDK_UTILITIES.has(exportName)) {
        throw new Error(
          `component_not_allowed: keel/guest export "${exportName}" is unavailable in v1`
        );
      }
      // SDK helpers and components share one public module ID.
      if (component) components.add(component);
      imports.add(moduleId);
      continue;
    }
    // Raw service methods take an arbitrary route, so they require a fixed route import.
    if (moduleId === 'host:service') {
      throw new Error(
        'capability_mismatch: use a fixed route or event import from host:service/<name>'
      );
    }
    // Host modules may only be acquired by named export.
    if (exportName === undefined) {
      throw new Error(`unknown_capability: invalid v1 import "${moduleId}"`);
    }
    const service = moduleId.startsWith(V1_SERVICE_PREFIX)
      ? v1ServiceLink(moduleId, exportName)
      : undefined;
    const contractModule = service ? 'host:service' : moduleId;
    const specifier = service?.operation ?? exportName;
    // Validate the original Host descriptor, even when the wire uses a fixed-route alias.
    if (!isHostModuleId(contractModule)) {
      throw new Error(`unknown_capability: invalid v1 module "${contractModule}"`);
    }
    const spec = contract.hostModules[contractModule];
    // Check own exports to prevent inherited object names becoming capabilities.
    if (!spec || !Object.hasOwn(spec, specifier)) {
      throw new Error(
        `unknown_capability: "${contractModule}.${specifier}" is absent from the contract`
      );
    }
    imports.add(contractModule);
    const key = contractModule.slice('host:'.length);
    const module = modules.get(key) ?? { rpc: new Set<string>(), events: new Set<string>() };
    const kind = spec[specifier]!.kind === 'subscription' ? 'events' : 'rpc';
    module[kind].add(service?.name ?? exportName);
    modules.set(key, module);
  }

  return {
    schemaVersion: 1,
    keelApi: '1.0',
    guestEntry: linkage.guestEntry,
    imports: [...imports].sort(),
    hostModules: Object.fromEntries(
      [...modules.keys()].sort().map((name) => {
        const { rpc, events } = modules.get(name)!;
        return [
          name,
          {
            ...(rpc.size ? { rpc: [...rpc].sort() } : {}),
            ...(events.size ? { events: [...events].sort() } : {}),
          },
        ];
      })
    ),
    components: [...components].sort(),
  };
}

/** Serialize inert linkage data ahead of the final CommonJS guest. */
export function writeV1Bundle(linkage: V1Linkage, code: string): string {
  return `${HEADER}${JSON.stringify(linkage)}\n${code}`;
}

/** Read and validate the first line without evaluating Guest code. */
export function readV1Linkage(bundle: string): V1Linkage {
  const endOfHeader = bundle.indexOf('\n');
  // The loader recognizes exactly one versioned header at the beginning of the bundle.
  if (endOfHeader < 0 || !bundle.startsWith(HEADER)) {
    throw new Error('capability_mismatch: missing v1 bundle linkage');
  }
  const { guestEntry, links } = JSON.parse(bundle.slice(HEADER.length, endOfHeader)) as {
    // Reason: Guest-authored header fields require runtime validation.
    guestEntry?: unknown;
    // Reason: Guest-authored header fields require runtime validation.
    links?: unknown;
  };
  // Require a flat array of unique module/export tuples before projecting the manifest.
  if (
    typeof guestEntry !== 'string' ||
    !Array.isArray(links) ||
    !links.every(
      (link) =>
        Array.isArray(link) &&
        (link.length === 1 || link.length === 2) &&
        link.every((value) => typeof value === 'string' && value.length > 0)
    ) ||
    new Set(links.map((link) => JSON.stringify(link))).size !== links.length
  ) {
    throw new Error('capability_mismatch: invalid v1 bundle linkage');
  }
  checkGuestEntry(guestEntry);
  return { guestEntry, links };
}

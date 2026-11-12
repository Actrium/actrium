import { type HostModuleImplementationMap, type KeelContractShape, isHostModuleId, rpc } from 'keel/contract';
import {
  type V1CapabilityManifest,
  type V1Linkage,
  V1_SERVICE_PREFIX,
  createV1CapabilityManifest,
  readV1Linkage,
  v1ServiceDispatchId,
  v1ServiceLink,
} from 'keel/v1';
import type { CapabilityVerificationCode } from './capability-verify';
import { type ServiceCallInput, type ServiceCallOutput, contract } from './contract';
import { HOST_SUPPORTED_METHODS, type HostModules } from './host-modules';
import { verifyRuntimeCapabilityManifestJSON } from './runtime-capability-verify';

/** Return a comparison key that treats manifest arrays as unordered sets of entries. */
// Reason: The signed JSON must be checked before it is treated as a typed manifest.
function manifestKey(value: unknown): string {
  // Sort arrays and object entries without discarding duplicates or unknown fields.
  if (Array.isArray(value)) return JSON.stringify(value.map(manifestKey).sort());
  if (value !== null && typeof value === 'object') {
    return JSON.stringify(Object.keys(value).sort().map((key) => [
      key,
      manifestKey((value as Record<string, unknown>)[key]),
    ]));
  }
  return JSON.stringify(value);
}

/** Check the exact Guest and manifest selected by the package loader before evaluation. */
export function verifyV1RuntimeGuest(
  script: string,
  manifestJSON: string,
  declarations: readonly string[]
):
  | { ok: true; linkage: V1Linkage; manifest: V1CapabilityManifest }
  | { ok: false; code: CapabilityVerificationCode; details: string[] } {
  try {
    const linkage = readV1Linkage(script);
    const manifest = createV1CapabilityManifest(linkage, contract);
    // The public service RPC list combines call/tell, so check the exact linked operation.
    for (const [moduleId, exportName] of linkage.links) {
      // SDK and React links have no Host implementation to validate.
      if (!exportName || !moduleId.startsWith('host:')) continue;
      const isService = moduleId.startsWith(V1_SERVICE_PREFIX);
      const hostId = isService ? 'host:service' : moduleId;
      // A fixed Service link also needs a transport adapter in this loader.
      if ((isService && exportName !== 'call') || !HOST_SUPPORTED_METHODS[hostId]?.has(exportName)) {
        return {
          ok: false,
          code: 'unknown_capability',
          details: [`Host method "${hostId}.${exportName}" is not implemented.`],
        };
      }
    }
    // Compare independently reconstructed usage with the package's signed profile.
    if (manifestKey(JSON.parse(manifestJSON)) !== manifestKey(manifest)) {
      return {
        ok: false,
        code: 'capability_mismatch',
        details: ['The signed capability manifest differs from the Guest linkage.'],
      };
    }
    const verified = verifyRuntimeCapabilityManifestJSON(manifestJSON, declarations);
    // Preserve existing Host allowlists and declaration checks.
    if (!verified.ok) {
      return { ok: false, code: verified.code ?? 'capability_mismatch', details: verified.details };
    }
    return { ok: true, linkage, manifest };
  } catch (cause) {
    return {
      ok: false,
      code: 'capability_mismatch',
      details: [cause instanceof Error ? cause.message : String(cause)],
    };
  }
}

/** Construct a Keel contract and implementation containing only verified v1 links. */
export function bindV1HostModules(
  linkage: V1Linkage,
  implementations: HostModules,
  authorizeCall: (capability: 'service.call') => Promise<void>
): { contract: KeelContractShape; hostModules: HostModuleImplementationMap } {
  const modules: KeelContractShape['hostModules'] = {};
  const hostModules: HostModuleImplementationMap = {};
  const available: HostModuleImplementationMap = implementations;
  const descriptors: KeelContractShape['hostModules'] = contract.hostModules;
  for (const [moduleId, exportName] of linkage.links) {
    // Only the implemented service call has a fixed-route adapter today.
    if (moduleId.startsWith(V1_SERVICE_PREFIX) && exportName === 'call') {
      const { name } = v1ServiceLink(moduleId, exportName);
      const id = v1ServiceDispatchId(moduleId, exportName);
      const original = contract.hostModules['host:service'].call;
      modules[id] = {
        call: rpc<ServiceCallInput, ServiceCallOutput>({
          schema: {
            ...original.schema,
            parseInput: (value) => {
              // Validate the wire payload and overwrite any Guest-supplied route.
              const data = value !== null && typeof value === 'object' && !Array.isArray(value)
                ? { ...value, route: name }
                : value;
              return original.schema!.parseInput!(data);
            },
          },
        }),
      };
      hostModules[id] = {
        call: async (input: ServiceCallInput) => {
          await authorizeCall('service.call');
          return implementations['host:service'].call(input);
        },
      };
    } else if (exportName && isHostModuleId(moduleId) && HOST_SUPPORTED_METHODS[moduleId]?.has(exportName)) {
      // Reuse the schema and implementation for each linked non-Service method.
      modules[moduleId] ??= {};
      hostModules[moduleId] ??= {};
      modules[moduleId][exportName] = descriptors[moduleId]![exportName]!;
      hostModules[moduleId][exportName] = available[moduleId]![exportName]!;
    }
  }
  return { contract: { ...contract, hostModules: modules }, hostModules };
}

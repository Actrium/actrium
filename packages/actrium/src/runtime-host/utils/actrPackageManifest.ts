import parseToml from '@iarna/toml/parse-string.js';

import {
  normalizeSha256Hex,
  SUPPORTED_CONTRACT,
  SUPPORTED_INTEGRITY_ALGORITHM,
} from '../coreContract';

export const ACTR_PACKAGE_FORMAT = 'actr-package/1' as const;

export interface ActorDescriptor {
  manufacturer: string;
  name: string;
  version: string;
}

export interface KeelGuestDescriptor {
  runtime: string;
  guestApi: string;
  capabilities?: string;
  contract?: string;
}

export interface WorkloadDescriptor {
  profile: string;
  entry: string;
  keel?: KeelGuestDescriptor;
}

export interface HostRequirements {
  profile: string;
  api: string;
  requiredCapabilities: string[];
  optionalCapabilities: string[];
}

export interface ActorDependency {
  actor: string;
  version: string;
  contract?: string;
  required: boolean;
}

export interface ActriumPackageExtension {
  title?: string;
  description?: string;
  icon?: string;
  themeColor?: string;
  leftPanelDefaultVisible: boolean;
  rightPanelDefaultVisible: boolean;
}

export interface PackageFileEntry {
  path: string;
  role: string;
  mediaType: string;
  size: number;
  sha256: string;
}

export interface ActrPackageManifest {
  format: typeof ACTR_PACKAGE_FORMAT;
  actor: ActorDescriptor;
  workload: WorkloadDescriptor;
  host: HostRequirements;
  dependencies: Record<string, ActorDependency>;
  extensions: { actrium?: ActriumPackageExtension };
  signature: { algorithm: 'ed25519'; keyId: string };
  files: PackageFileEntry[];
}

export interface PackageIntegrityPlan {
  algorithm: typeof SUPPORTED_INTEGRITY_ALGORITHM;
  files: Array<{ path: string; expectedHash: string }>;
}

export function parseActrPackageManifest(source: string): ActrPackageManifest {
  let root: Record<string, unknown>;
  try {
    const parsed = parseToml(source);
    root = requiredRecord(parsed, 'manifest');
  } catch (error) {
    throw new Error(`manifest TOML is invalid: ${errorMessage(error)}`);
  }

  const format = requiredString(root, 'format');
  if (format !== ACTR_PACKAGE_FORMAT) {
    throw new Error(
      `unsupported package format \`${format}\`, expected \`${ACTR_PACKAGE_FORMAT}\``,
    );
  }

  const actorRaw = requiredRecord(root.actor, 'actor');
  const workloadRaw = requiredRecord(root.workload, 'workload');
  const hostRaw = requiredRecord(root.host, 'host');
  const signatureRaw = requiredRecord(root.signature, 'signature');
  const workloadProfile = requiredString(workloadRaw, 'profile');
  const supportedProfile = `${SUPPORTED_CONTRACT.name}/${SUPPORTED_CONTRACT.version}`;
  if (workloadProfile !== supportedProfile) {
    throw new Error(
      `unsupported workload profile \`${workloadProfile}\`, expected \`${supportedProfile}\``,
    );
  }

  const keelRaw = requiredRecord(workloadRaw.keel, 'workload.keel');
  const guestApi = requiredString(keelRaw, 'guest-api');
  if (guestApi !== supportedProfile) {
    throw new Error(
      `workload.keel.guest-api must be \`${supportedProfile}\``,
    );
  }

  const signatureAlgorithm = requiredString(signatureRaw, 'algorithm');
  if (signatureAlgorithm !== 'ed25519') {
    throw new Error('signature.algorithm must be `ed25519`');
  }

  const filesRaw = root.files;
  if (!Array.isArray(filesRaw) || filesRaw.length === 0) {
    throw new Error('manifest must declare at least one `files` entry');
  }
  const files = filesRaw.map((value, index) => parseFile(value, index));
  const paths = new Set<string>();
  for (const file of files) {
    if (paths.has(file.path)) {
      throw new Error(`manifest declares duplicate file \`${file.path}\``);
    }
    paths.add(file.path);
  }

  const workloadEntry = requiredString(workloadRaw, 'entry');
  const workloadFile = files.find((file) => file.path === workloadEntry);
  if (!workloadFile || workloadFile.role !== 'workload') {
    throw new Error(
      `workload entry \`${workloadEntry}\` must be declared with role \`workload\``,
    );
  }

  for (const profileFile of [
    optionalString(keelRaw, 'capabilities'),
    optionalString(keelRaw, 'contract'),
  ]) {
    if (profileFile && !paths.has(profileFile)) {
      throw new Error(`profile file \`${profileFile}\` is not declared in files`);
    }
  }

  return {
    format: ACTR_PACKAGE_FORMAT,
    actor: {
      manufacturer: requiredString(actorRaw, 'manufacturer'),
      name: requiredString(actorRaw, 'name'),
      version: requiredString(actorRaw, 'version'),
    },
    workload: {
      profile: workloadProfile,
      entry: workloadEntry,
      keel: {
        runtime: requiredString(keelRaw, 'runtime'),
        guestApi,
        capabilities: optionalString(keelRaw, 'capabilities'),
        contract: optionalString(keelRaw, 'contract'),
      },
    },
    host: {
      profile: requiredString(hostRaw, 'profile'),
      api: requiredString(hostRaw, 'api'),
      requiredCapabilities: optionalStringArray(hostRaw, 'required-capabilities') ?? [],
      optionalCapabilities: optionalStringArray(hostRaw, 'optional-capabilities') ?? [],
    },
    dependencies: parseDependencies(root.dependencies),
    extensions: parseExtensions(root.extensions),
    signature: {
      algorithm: 'ed25519',
      keyId: requiredString(signatureRaw, 'key-id'),
    },
    files,
  };
}

export function validateActrPackageManifest(source: string): void {
  parseActrPackageManifest(source);
}

export function createPackageIntegrityPlan(
  manifest: ActrPackageManifest,
): PackageIntegrityPlan {
  return {
    algorithm: SUPPORTED_INTEGRITY_ALGORITHM,
    files: manifest.files.map((file) => ({
      path: file.path,
      expectedHash: file.sha256,
    })),
  };
}

function parseFile(value: unknown, index: number): PackageFileEntry {
  const record = requiredRecord(value, `files[${index}]`);
  const path = requiredString(record, 'path');
  validatePackagePath(path);
  const sha256 = normalizeSha256Hex(requiredString(record, 'sha256'));
  if (!sha256) {
    throw new Error(`files[${index}].sha256 must be a SHA-256 hex digest`);
  }
  const size = record.size;
  if (typeof size !== 'number' || !Number.isSafeInteger(size) || size < 0) {
    throw new Error(`files[${index}].size must be a non-negative safe integer`);
  }
  return {
    path,
    role: requiredString(record, 'role'),
    mediaType: requiredString(record, 'media-type'),
    size,
    sha256,
  };
}

function parseDependencies(value: unknown): Record<string, ActorDependency> {
  if (value === undefined) return {};
  const dependencies = requiredRecord(value, 'dependencies');
  return Object.fromEntries(
    Object.entries(dependencies).map(([name, raw]) => {
      const dependency = requiredRecord(raw, `dependencies.${name}`);
      const required = dependency.required;
      if (required !== undefined && typeof required !== 'boolean') {
        throw new Error(`dependencies.${name}.required must be a boolean`);
      }
      return [name, {
        actor: requiredString(dependency, 'actor'),
        version: requiredString(dependency, 'version'),
        contract: optionalString(dependency, 'contract'),
        required: required ?? true,
      }];
    }),
  );
}

function parseExtensions(value: unknown): ActrPackageManifest['extensions'] {
  if (value === undefined) return {};
  const extensions = requiredRecord(value, 'extensions');
  if (extensions.actrium === undefined) return {};
  const actrium = requiredRecord(extensions.actrium, 'extensions.actrium');
  return {
    actrium: {
      title: optionalString(actrium, 'title'),
      description: optionalString(actrium, 'description'),
      icon: optionalString(actrium, 'icon'),
      themeColor: optionalString(actrium, 'theme-color'),
      leftPanelDefaultVisible:
        optionalBoolean(actrium, 'left-panel-default-visible') ?? false,
      rightPanelDefaultVisible:
        optionalBoolean(actrium, 'right-panel-default-visible') ?? false,
    },
  };
}

function validatePackagePath(path: string): void {
  if (
    path.startsWith('/') ||
    path.startsWith('\\') ||
    path.includes('\\') ||
    path.split('/').some((part) => part.length === 0 || part === '.' || part === '..') ||
    path === 'manifest.toml' ||
    path === 'manifest.sig'
  ) {
    throw new Error(`invalid package path \`${path}\``);
  }
}

function requiredRecord(value: unknown, field: string): Record<string, unknown> {
  if (typeof value !== 'object' || value === null || Array.isArray(value)) {
    throw new Error(`${field} must be a table`);
  }
  return value as Record<string, unknown>;
}

function requiredString(record: Record<string, unknown>, field: string): string {
  const value = record[field];
  if (typeof value !== 'string' || value.trim().length === 0) {
    throw new Error(`${field} must be a non-empty string`);
  }
  return value;
}

function optionalString(
  record: Record<string, unknown>,
  field: string,
): string | undefined {
  const value = record[field];
  if (value === undefined) return undefined;
  if (typeof value !== 'string') {
    throw new Error(`${field} must be a string`);
  }
  return value;
}

function optionalBoolean(
  record: Record<string, unknown>,
  field: string,
): boolean | undefined {
  const value = record[field];
  if (value === undefined) return undefined;
  if (typeof value !== 'boolean') {
    throw new Error(`${field} must be a boolean`);
  }
  return value;
}

function optionalStringArray(
  record: Record<string, unknown>,
  field: string,
): string[] | undefined {
  const value = record[field];
  if (value === undefined) return undefined;
  if (!Array.isArray(value) || value.some((item) => typeof item !== 'string')) {
    throw new Error(`${field} must be an array of strings`);
  }
  return value as string[];
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

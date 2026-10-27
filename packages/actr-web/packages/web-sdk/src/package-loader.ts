/**
 * Unverified .actr package inspection helpers.
 *
 * These helpers only parse the ZIP and manifest. They DO NOT verify
 * manifest.sig, signing trust, or any manifest-declared content hash. Every
 * returned field is attacker-controlled when the input package is untrusted.
 * Never eval the returned JavaScript or instantiate the returned WASM.
 *
 * Executable workload loading belongs to actor.sw.js, which delegates package
 * verification to the Rust Web host before extracting binary or glue content.
 *
 * .actr ZIP structure (all entries STORE / no compression):
 *   manifest.toml             - package manifest (TOML)
 *   manifest.sig              - Ed25519 signature (64 bytes)
 *   bin/actor.wasm        - WASM binary
 *   resources/glue.js     - wasm-bindgen JS glue
 *   resources/actor.sw.js - Service Worker entry
 */

/**
 * Parsed manifest.toml manifest
 */
export interface UnverifiedActrManifest {
  publisher: string;
  name: string;
  version: string;
  signature_algorithm: string;
  binary: {
    path: string;
    target: string;
    hash: string;
    size: number;
  };
  resources: Array<{
    path: string;
    hash: string;
  }>;
  metadata: {
    description: string;
    license: string;
  };
}

/**
 * Result of parsing a .actr package
 */
export interface UnverifiedLoadedActrPackage {
  /** Unverified manifest text parsed from manifest.toml. */
  manifest: UnverifiedActrManifest;
  /** Unverified raw WASM bytes. Never instantiate these bytes. */
  wasmBytes: Uint8Array;
  /** Unverified JS source text. Never evaluate this text. */
  jsGlue: string | null;
  /** Unverified Service Worker source text. Never register or evaluate it. */
  actorSwJs: string | null;
  /** Unverified ZIP entries: filename → bytes. */
  files: Map<string, Uint8Array>;
}

/** @deprecated Use UnverifiedActrManifest; this parser does not verify packages. */
export type ActrManifest = UnverifiedActrManifest;

/**
 * @deprecated Use UnverifiedLoadedActrPackage; none of its contents are
 * authenticated and executable fields must not be executed.
 */
export type LoadedActrPackage = UnverifiedLoadedActrPackage;

// ── ZIP parser (STORE-only) ──

/**
 * Parse a ZIP file with STORE compression (no deflate).
 * .actr packages always use CompressionMethod::Stored.
 */
function parseStoreZip(buffer: ArrayBuffer): Map<string, Uint8Array> {
  const view = new DataView(buffer);
  const bytes = new Uint8Array(buffer);
  const entries = new Map<string, Uint8Array>();
  let offset = 0;

  while (offset + 30 <= buffer.byteLength) {
    const sig = view.getUint32(offset, true);
    if (sig !== 0x04034b50) break; // not a Local File Header

    const compressedSize = view.getUint32(offset + 18, true);
    const filenameLen = view.getUint16(offset + 26, true);
    const extraLen = view.getUint16(offset + 28, true);

    const filenameBytes = bytes.subarray(offset + 30, offset + 30 + filenameLen);
    const filename = new TextDecoder().decode(filenameBytes);

    const dataStart = offset + 30 + filenameLen + extraLen;
    const dataEnd = dataStart + compressedSize;

    if (dataEnd > buffer.byteLength) break;

    entries.set(filename, bytes.slice(dataStart, dataEnd));
    offset = dataEnd;
  }

  return entries;
}

// ── Minimal TOML parser for manifest.toml ──

/**
 * Parse a minimal subset of TOML used in manifest.toml.
 * Handles top-level keys, [section] headers, [[array]] items.
 */
function parseActrToml(text: string): UnverifiedActrManifest {
  const manifest: UnverifiedActrManifest = {
    publisher: '',
    name: '',
    version: '',
    signature_algorithm: '',
    binary: { path: '', target: '', hash: '', size: 0 },
    resources: [],
    metadata: { description: '', license: '' },
  };

  let currentSection = '';
  let currentResource: { path: string; hash: string } | null = null;

  for (const rawLine of text.split('\n')) {
    const line = rawLine.trim();
    if (!line || line.startsWith('#')) continue;

    // [[resources]] array
    if (line === '[[resources]]') {
      if (currentResource) manifest.resources.push(currentResource);
      currentResource = { path: '', hash: '' };
      currentSection = 'resources';
      continue;
    }

    // [section] header
    const sectionMatch = line.match(/^\[(\w+)\]$/);
    if (sectionMatch) {
      if (currentResource) {
        manifest.resources.push(currentResource);
        currentResource = null;
      }
      currentSection = sectionMatch[1];
      continue;
    }

    // key = value
    const kvMatch = line.match(/^(\w+)\s*=\s*(.+)$/);
    if (!kvMatch) continue;

    const key = kvMatch[1];
    let val = kvMatch[2].trim();
    // Strip quotes
    if ((val.startsWith('"') && val.endsWith('"')) || (val.startsWith("'") && val.endsWith("'"))) {
      val = val.slice(1, -1);
    }

    if (currentSection === '' || currentSection === 'package') {
      if (key === 'publisher') manifest.publisher = val;
      else if (key === 'name') manifest.name = val;
      else if (key === 'version') manifest.version = val;
      else if (key === 'signature_algorithm') manifest.signature_algorithm = val;
    } else if (currentSection === 'binary') {
      if (key === 'path') manifest.binary.path = val;
      else if (key === 'target') manifest.binary.target = val;
      else if (key === 'hash') manifest.binary.hash = val;
      else if (key === 'size') manifest.binary.size = parseInt(val, 10);
    } else if (currentSection === 'resources' && currentResource) {
      if (key === 'path') currentResource.path = val;
      else if (key === 'hash') currentResource.hash = val;
    } else if (currentSection === 'metadata') {
      if (key === 'description') manifest.metadata.description = val;
      else if (key === 'license') manifest.metadata.license = val;
    }
  }

  if (currentResource) manifest.resources.push(currentResource);

  return manifest;
}

// ── Public API ──

/**
 * Fetch and inspect a .actr package from a URL without verification.
 *
 * This is suitable only for non-security-sensitive display and diagnostics.
 * It does not perform pre-validation. Do not execute any returned content.
 *
 * @param url - URL of the .actr package
 * @returns Unverified package contents
 */
export async function loadUnverifiedActrPackage(
  url: string
): Promise<UnverifiedLoadedActrPackage> {
  const resp = await fetch(url);
  if (!resp.ok) {
    throw new Error(`Failed to fetch .actr package: ${resp.status} ${resp.statusText} (${url})`);
  }
  const buffer = await resp.arrayBuffer();
  return parseUnverifiedActrPackage(buffer);
}

/**
 * Inspect raw .actr package bytes without verification.
 *
 * The result has not passed signature or hash verification. It must never be
 * used as an executable workload or as trusted package metadata.
 *
 * @param buffer - Raw .actr ZIP bytes
 * @returns Unverified package contents
 */
export function parseUnverifiedActrPackage(buffer: ArrayBuffer): UnverifiedLoadedActrPackage {
  const files = parseStoreZip(buffer);

  // Parse manifest
  const tomlBytes = files.get('manifest.toml');
  if (!tomlBytes) {
    throw new Error('Invalid .actr package: missing manifest.toml');
  }
  const manifest = parseActrToml(new TextDecoder().decode(tomlBytes));

  // Find WASM binary
  let wasmBytes: Uint8Array | null = null;
  for (const [name, data] of files) {
    if (name.startsWith('bin/') && name.endsWith('.wasm')) {
      wasmBytes = data;
      break;
    }
  }
  if (!wasmBytes) {
    throw new Error('Invalid .actr package: no WASM binary found');
  }

  // Find JS glue (resources/*.js, not actor.sw.js)
  let jsGlue: string | null = null;
  for (const [name, data] of files) {
    if (name.startsWith('resources/') && name.endsWith('.js') && !name.endsWith('actor.sw.js')) {
      jsGlue = new TextDecoder().decode(data);
      break;
    }
  }

  // Find actor.sw.js
  let actorSwJs: string | null = null;
  const swBytes = files.get('resources/actor.sw.js');
  if (swBytes) {
    actorSwJs = new TextDecoder().decode(swBytes);
  }

  return { manifest, wasmBytes, jsGlue, actorSwJs, files };
}

/**
 * @deprecated Use loadUnverifiedActrPackage. Despite its historical name,
 * this function does not verify signatures or hashes. Never execute its output.
 */
export async function loadActrPackage(url: string): Promise<LoadedActrPackage> {
  return loadUnverifiedActrPackage(url);
}

/**
 * @deprecated Use parseUnverifiedActrPackage. Despite its historical name,
 * this function does not verify signatures or hashes. Never execute its output.
 */
export function parseActrPackage(buffer: ArrayBuffer): LoadedActrPackage {
  return parseUnverifiedActrPackage(buffer);
}

/**
 * .actr File Loader Type Definition
 */

import type { ActrManifest } from '../coreContract';

export type { ActrManifest } from '../coreContract';
export { ASSET_EXTENSIONS } from '../coreContract';

/**
 * After parsing .actr Package Data
 */
export interface ActrPackage {
  manifest: ActrManifest;
  unifiedScript: string; // Code contents of entry files
  sourceUrl: string; // Original .actr File URL
  archiveHash?: string; // Downloadable. .actr zip SHA-256(if caller requests verification)
}

/**
 * Single Load Options
 */
export interface ActrLoadOptions {
  expectedSha256?: string | null; // Expectations .actr zip SHA-256
}

/**
 * Load Options
 */
export interface ActrLoaderOptions {
  enableCache?: boolean; // Whether Cache Enabled
  cacheExpiry?: number; // Cache Expire Time (ms)
  timeout?: number; // Download timeout (ms)
  verifyIntegrity?: boolean; // Validation of document integrity
}

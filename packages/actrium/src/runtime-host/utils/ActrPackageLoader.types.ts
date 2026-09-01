/**
 * .actr File Loader Type Definition
 */

import type { ActrPackageManifest } from './actrPackageManifest';

export type { ActrPackageManifest } from './actrPackageManifest';
export { ASSET_EXTENSIONS } from '../coreContract';

/**
 * After parsing .actr Package Data
 */
export interface ActrPackage {
  manifest: ActrPackageManifest;
  unifiedScript: string; // Code contents of entry files
  sourceUrl: string; // Original .actr File URL
  archiveHash?: string; // Downloadable. .actr zip SHA-256(if caller requests verification)
}

/**
 * Single Load Options
 */
export interface ActrPackageLoadOptions {
  expectedSha256?: string | null; // Expectations .actr zip SHA-256
}

/**
 * Load Options
 */
export interface ActrPackageLoaderOptions {
  enableCache?: boolean; // Whether Cache Enabled
  cacheExpiry?: number; // Cache Expire Time (ms)
  timeout?: number; // Download timeout (ms)
}

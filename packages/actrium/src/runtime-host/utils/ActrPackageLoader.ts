/**
 * .actr File Loader
 *
 * Function:
 * 1. From URL Download .actr Documentationzip (Packages)
 * 2. Unpressure zip Package
 * 3. Parse the versioned manifest.toml
 * 4. Read the declared Keel Guest workload
 * 5. Copy picture resource to Durable Directory
 * 6. Replaces the relative path in the script as absolute file:// URI
 * 7. Return can be passed directly to RuntimeTabView Data
 */

import RNFS from 'react-native-fs';
import { unzip } from 'react-native-zip-archive';
import {
  isAssetPath,
  normalizePath,
  normalizeSha256Hex,
  relativePathFrom,
  SUPPORTED_INTEGRITY_ALGORITHM,
} from '../coreContract';
import {
  createPackageIntegrityPlan,
  parseActrPackageManifest,
} from './actrPackageManifest';
import type {
  ActrPackageLoadOptions,
  ActrPackageManifest,
  ActrPackage,
  ActrPackageLoaderOptions,
} from './ActrPackageLoader.types';

/** Root directory of the resource for sustainable storage */
const ASSETS_ROOT_DIR = normalizePath(`${RNFS.DocumentDirectoryPath}/actr-assets`);

export class ActrPackageLoader {
  private cache = new Map<
    string,
    { package: ActrPackage; timestamp: number }
  >();
  private options: Required<ActrPackageLoaderOptions>;

  constructor(options: ActrPackageLoaderOptions = {}) {
    this.options = {
      enableCache: options.enableCache ?? true,
      cacheExpiry: options.cacheExpiry ?? 10 * 60 * 1000, // 10min
      timeout: options.timeout ?? 30000, // 30sec
    };
  }

  /**
   * From URL Load .actr Documentation
   * @param url .actr File URL Address
   * @param appId Apply ID, for the Resource Storage Directory Naming
   * @returns Parsed package data
   */
  async load(
    url: string,
    appId: string,
    loadOptions: ActrPackageLoadOptions = {},
  ): Promise<ActrPackage> {
    console.log(`[ActrPackageLoader] Start loading: ${url}`);
    const expectedSha256 = this.normalizeExpectedSha256(
      loadOptions.expectedSha256,
    );

    // Check Cache
    if (this.options.enableCache) {
      const cached = this.cache.get(url);
      if (cached && Date.now() - cached.timestamp < this.options.cacheExpiry) {
        if (!expectedSha256 || cached.package.archiveHash === expectedSha256) {
          console.log(`[ActrPackageLoader] Use Cache: ${url}`);
          return cached.package;
        }

        console.log(`[ActrPackageLoader] Cache hash Unsatisfactory expectations. Redownload.: ${url}`);
      }
    }

    let zipPath: string | null = null;
    let rootDir: string | null = null;

    try {
      // 1. Download .actr Documentation
      zipPath = await this.downloadFile(url);
      console.log(`[ActrPackageLoader] Download completed`);

      // 2. Verify the same download .actr zip(if called by the hash)
      const archiveHash = await this.verifyDownloadedArchive(
        zipPath,
        expectedSha256,
      );

      // 3. Unpressure zip Documentation
      const extracted = await this.extractZip(zipPath);
      const { workDir } = extracted;
      rootDir = extracted.rootDir;
      console.log(`[ActrPackageLoader] Depression complete.`);

      // 4. Parse manifest.toml.
      const manifest = await this.parseManifest(workDir);
      console.log(
        `[ActrPackageLoader] Parsed manifest: ${manifest.actor.publisher}/${manifest.actor.name}@${manifest.actor.version}`,
      );

      // 5. Every declared payload is authenticated before any workload bytes
      // are copied or executed. This is a package invariant, not an option.
      await this.verifyPackageIntegrity(workDir, manifest);

      // 6. Read access files
      let unifiedScript = await this.readEntryFile(workDir, manifest.workload.entry);
      console.log(
        `[ActrPackageLoader] Read workload: ${manifest.workload.entry} (${unifiedScript.length} bytes)`,
      );

      // 7. Copy picture resources to a persistent directory and replace relative paths in scripts
      const assetDir = `${ASSETS_ROOT_DIR}/${appId}`;
      const assetFileNames = await this.copyAssets(workDir, assetDir);

      if (assetFileNames.length > 0) {
        unifiedScript = this.rewriteAssetPaths(unifiedScript, assetDir, assetFileNames);
        console.log(
          `[ActrPackageLoader] Replaced ${assetFileNames.length} Resource Path`,
        );
      }

      const result: ActrPackage = {
        manifest,
        unifiedScript,
        sourceUrl: url,
        archiveHash,
      };

      // Cache Results
      if (this.options.enableCache) {
        this.cache.set(url, { package: result, timestamp: Date.now() });
      }

      return result;
    } catch (error) {
      console.error(`[ActrPackageLoader] Loading failed: ${url}`, error);
      throw error;
    } finally {
      await this.cleanup(zipPath, rootDir);
    }
  }

  private normalizeExpectedSha256(expectedSha256?: string | null): string | null {
    if (!expectedSha256) {
      return null;
    }

    const normalized = normalizeSha256Hex(expectedSha256);
    if (!normalized) {
      throw new Error(`[ActrPackageLoader] expected SHA-256 Invalid: ${expectedSha256}`);
    }

    return normalized;
  }

  private async verifyDownloadedArchive(
    zipPath: string,
    expectedSha256: string | null,
  ): Promise<string | undefined> {
    if (!expectedSha256) {
      return undefined;
    }

    const actualHash = normalizeSha256Hex(
      await RNFS.hash(zipPath, SUPPORTED_INTEGRITY_ALGORITHM),
    );

    if (actualHash !== expectedSha256) {
      throw new Error(
        `[ActrPackageLoader] .actr SHA-256 Verify Failed: expected ${expectedSha256}, got ${actualHash ?? 'invalid hash'}`,
      );
    }

    return actualHash;
  }

  /**
   * Download files to temporary directory
   */
  private async downloadFile(url: string): Promise<string> {
    const fileName = `actr_${Date.now()}.zip`;
    const downloadPath = normalizePath(`${RNFS.TemporaryDirectoryPath}/${fileName}`);

    console.log(`[ActrPackageLoader] Start Download: ${url} -> ${downloadPath}`);

    const downloadResult = await RNFS.downloadFile({
      fromUrl: url,
      toFile: downloadPath,
      connectionTimeout: this.options.timeout,
      readTimeout: this.options.timeout,
    }).promise;

    if (downloadResult.statusCode !== 200) {
      throw new Error(`Download failed: HTTP ${downloadResult.statusCode}`);
    }

    return downloadPath;
  }

  /**
   * Unpressure zip Documentation
   * @returns { workDir: directory containing manifest.toml, rootDir: extraction directory for cleanup }
   */
  private async extractZip(
    zipPath: string,
  ): Promise<{ workDir: string; rootDir: string }> {
    const extractDir = normalizePath(`${RNFS.TemporaryDirectoryPath}/actr_${Date.now()}`);

    console.log(`[ActrPackageLoader] Start depressure.:`);

    // Inspection zip Existence of file
    const zipExists = await RNFS.exists(zipPath);

    if (!zipExists) {
      throw new Error(`Zip File does not exist: ${zipPath}`);
    }

    // Access zip File Size
    try {
      const stat = await RNFS.stat(zipPath);
    } catch (error) {
      console.warn(`  - Unable to access zip File Information:`, error);
    }

    // Create a depressive directory
    await RNFS.mkdir(extractDir);

    // Use react-native-zip-archive Unpressure
    try {
      await unzip(zipPath, extractDir);

      // Verify the depression result
      const extractedItems = await RNFS.readDir(extractDir);
      console.log(`[ActrPackageLoader] Number of files depressed: ${extractedItems.length}`);
      extractedItems.forEach((item) => {
        console.log(
          `[ActrPackageLoader] ${item.name} (${item.isDirectory() ? 'Contents' : 'Documentation'})`,
        );
      });

      // If only one directory is depressed, enter it as a work directory
      if (extractedItems.length === 1 && extractedItems[0].isDirectory()) {
        const subDir = extractedItems[0].path;
        return { workDir: subDir, rootDir: extractDir };
      }

      // Otherwise, you can use the depressor directory directly.
      return { workDir: extractDir, rootDir: extractDir };
    } catch (error) {
      console.error(`[ActrPackageLoader] Unpressure failed:`, error);
      throw new Error(
        `[ActrPackageLoader] Unpressure failed: ${error instanceof Error ? error.message : String(error)}`,
      );
    }
  }

  /**
   * Parse the package's versioned TOML manifest.
   */
  private async parseManifest(extractDir: string): Promise<ActrPackageManifest> {
    const manifestPath = `${extractDir}/manifest.toml`;
    const exists = await RNFS.exists(manifestPath);

    if (!exists) {
      // Lists the contents of the directory to debug
      try {
        const items = await RNFS.readDir(extractDir);
        console.error('[ActrPackageLoader] manifest.toml was not found. Extracted entries:');
        items.forEach((item) => {
          console.error(
            `[ActrPackageLoader] ${item.name} (${item.isDirectory() ? 'Contents' : 'Documentation'})`,
          );
        });
      } catch (error) {
        console.error(`[ActrPackageLoader] Unable to read depresso directory:`, error);
      }
      throw new Error(`[ActrPackageLoader] manifest.toml was not found: ${manifestPath}`);
    }

    // Read File Contents
    const content = await RNFS.readFile(manifestPath, 'utf8');
    console.log(`[ActrPackageLoader] manifest.toml size: ${content.length} bytes`);

    try {
      const manifest = parseActrPackageManifest(content);

      console.log(
        `[ActrPackageLoader] manifest parsed: ${manifest.actor.name} v${manifest.actor.version}, entry: ${manifest.workload.entry}`,
      );
      return manifest;
    } catch (error) {
      throw new Error(
        `[ActrPackageLoader] manifest.toml parsing failed: ${error instanceof Error ? error.message : String(error)}`,
      );
    }
  }

  /**
   * Press Rust-owned manifest integrity Planned to verify the package document.
   */
  private async verifyPackageIntegrity(
    workDir: string,
    manifest: ActrPackageManifest,
  ): Promise<void> {
    const plan = createPackageIntegrityPlan(manifest);
    if (!plan) {
      return;
    }

    for (const file of plan.files) {
      const filePath = normalizePath(`${workDir}/${file.path}`);
      const exists = await RNFS.exists(filePath);
      if (!exists) {
        throw new Error(`[ActrPackageLoader] integrity File does not exist: ${file.path}`);
      }

      const actualHash = (await RNFS.hash(filePath, plan.algorithm)).toLowerCase();
      if (actualHash !== file.expectedHash) {
        throw new Error(
          `[ActrPackageLoader] integrity Verify Failed: ${file.path}, expected ${file.expectedHash}, got ${actualHash}`,
        );
      }
    }
  }

  /**
   * Read entry file contents
   */
  private async readEntryFile(
    extractDir: string,
    entryPath: string,
  ): Promise<string> {
    // Support multiple possible path formats
    const possiblePaths = [
      `${extractDir}/${entryPath}`,
      `${extractDir}/dist/${entryPath}`,
      `${extractDir}/${entryPath.replace(/^dist\//, '')}`,
    ];

    console.log(`[ActrPackageLoader] Find entry files: ${entryPath}`);

    for (const path of possiblePaths) {
      const exists = await RNFS.exists(path);
      if (exists) {
        const content = await RNFS.readFile(path, 'utf8');
        return content;
      }
    }

    throw new Error(`[ActrPackageLoader] The entry file does not exist: ${entryPath}`);
  }

  /**
   * Recursive scan directory to collect relative paths for all photographic resource files
   */
  private async scanAssetFiles(dir: string, baseDir: string): Promise<string[]> {
    const results: string[] = [];
    let items: RNFS.ReadDirItem[];

    try {
      items = await RNFS.readDir(dir);
    } catch {
      return results;
    }

    for (const item of items) {
      if (item.isDirectory()) {
        const subResults = await this.scanAssetFiles(item.path, baseDir);
        results.push(...subResults);
      } else if (item.isFile()) {
        if (isAssetPath(item.name)) {
          // Return relative to baseDir Path to
          const relativePath = relativePathFrom(item.path, baseDir);
          results.push(relativePath);
        }
      }
    }

    return results;
  }

  /**
   * Copy picture resource to Durable Directory
   * If the target directory already exists, empty it and then copy it.
   * @returns List of file names successfully copied (excluding directory prefix)
   */
  private async copyAssets(
    workDir: string,
    assetDir: string,
  ): Promise<string[]> {
    // Scan all photographic resources in the depressure directory
    const relativePaths = await this.scanAssetFiles(workDir, workDir);

    if (relativePaths.length === 0) {
      return [];
    }

    console.log(
      `[ActrPackageLoader] Found ${relativePaths.length} A Photo Resource`,
    );

    // If the target directory already exists, clear it first.
    const dirExists = await RNFS.exists(assetDir);
    if (dirExists) {
      await RNFS.unlink(assetDir);
      console.log(`[ActrPackageLoader] Emptyed old resource directory: ${assetDir}`);
    }

    // Create destination directory
    await RNFS.mkdir(assetDir);

    // Copy Resource Files Individually
    const copiedFileNames: string[] = [];

    for (const relativePath of relativePaths) {
      const srcPath = `${workDir}/${relativePath}`;
      const fileName = relativePath.split('/').pop()!;
      const destPath = `${assetDir}/${fileName}`;

      try {
        // Ensure Source File Exists
        const srcExists = await RNFS.exists(srcPath);
        if (!srcExists) {
          console.warn(`[ActrPackageLoader] Resource file does not exist, Skip: ${srcPath}`);
          continue;
        }

        await RNFS.copyFile(srcPath, destPath);
        copiedFileNames.push(fileName);
      } catch (error) {
        console.warn(
          `[ActrPackageLoader] Copying resource failed: ${srcPath} -> ${destPath}`,
          error,
        );
      }
    }

    console.log(
      `[ActrPackageLoader] Copying Resource Complete: ${copiedFileNames.length}/${relativePaths.length}`,
    );

    return copiedFileNames;
  }

  /**
   * Replace the relative path in the script with file:// URI
   *
   * Bun build Format in Product:
   *   module2.exports = "./clear-653n4c11.png";
   * Replace With:
   *   module2.exports = {uri:"file:///path/to/actr-assets/{appId}/clear-653n4c11.png"};
   *
   * Photo File Namebelt content hashThe risk of error is extremely low.
   */
  private rewriteAssetPaths(
    script: string,
    assetDir: string,
    assetFileNames: string[],
  ): string {
    let result = script;

    for (const fileName of assetFileNames) {
      // Match "./{fileName}" or "../{fileName}" The relative path string
      const pattern = new RegExp(
        `"((?:\\.\\.?/)*)${escapeRegExp(fileName)}"`,
        'g',
      );

      const absoluteUri = `file://${assetDir}/${fileName}`;
      const replacement = `{uri:"${absoluteUri}"}`;

      result = result.replace(pattern, replacement);
    }

    return result;
  }

  /**
   * Clear temporary files
   */
  private async cleanup(
    zipPath: string | null,
    extractDir: string | null,
  ): Promise<void> {
    try {
      // Delete zip Documentation
      if (zipPath) {
        const zipExists = await RNFS.exists(zipPath);
        if (zipExists) {
          await RNFS.unlink(zipPath);
          console.log(`[ActrPackageLoader] temporary file deleted`);
        }
      }

      // Remove the depressure directory
      if (extractDir) {
        const dirExists = await RNFS.exists(extractDir);
        if (dirExists) {
          await RNFS.unlink(extractDir);
          // console.log(`[ActrPackageLoader] Removed temporary directory: ${extractDir}`);
        }
      }
    } catch (error) {
      console.log(`[ActrPackageLoader] Failed to clean temporary files:`, error);
      // Cleanup failure does not affect the main process
    }
  }

  /**
   * Clear Cache
   */
  clearCache(url?: string): void {
    if (url) {
      this.cache.delete(url);
      console.log(`[ActrPackageLoader] Cleared Cache: ${url}`);
    } else {
      this.cache.clear();
      console.log(`[ActrPackageLoader] Cleared all caches`);
    }
  }
}

/**
 * Special characters for transposition regular expression
 */
function escapeRegExp(str: string): string {
  return str.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

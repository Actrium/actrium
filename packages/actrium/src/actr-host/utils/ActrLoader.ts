/**
 * .actr File Loader
 *
 * Function:
 * 1. From URL Download .actr Documentationzip (Packages)
 * 2. Unpressure zip Package
 * 3. Parsing manifest.json
 * 4. Read entry files (unified-app.js)
 * 5. Copy picture resource to Durable Directory
 * 6. Replaces the relative path in the script as absolute file:// URI
 * 7. Return can be passed directly to ActrTabView Data
 */

import RNFS from 'react-native-fs';
import { unzip } from 'react-native-zip-archive';
import {
  createActrIntegrityPlan,
  isAssetPath,
  normalizePath,
  normalizeSha256Hex,
  parseActrManifest,
  relativePathFrom,
  SUPPORTED_INTEGRITY_ALGORITHM,
} from '../coreContract';
import type {
  ActrLoadOptions,
  ActrManifest,
  ActrPackage,
  ActrLoaderOptions,
} from './ActrLoader.types';

/** Root directory of the resource for sustainable storage */
const ASSETS_ROOT_DIR = normalizePath(`${RNFS.DocumentDirectoryPath}/actr-assets`);

export class ActrLoader {
  private cache = new Map<
    string,
    { package: ActrPackage; timestamp: number }
  >();
  private options: Required<ActrLoaderOptions>;

  constructor(options: ActrLoaderOptions = {}) {
    this.options = {
      enableCache: options.enableCache ?? true,
      cacheExpiry: options.cacheExpiry ?? 10 * 60 * 1000, // 10min
      timeout: options.timeout ?? 30000, // 30sec
      verifyIntegrity: options.verifyIntegrity ?? false,
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
    loadOptions: ActrLoadOptions = {},
  ): Promise<ActrPackage> {
    console.log(`[ActrLoader] Start loading: ${url}`);
    const expectedSha256 = this.normalizeExpectedSha256(
      loadOptions.expectedSha256,
    );

    // Check Cache
    if (this.options.enableCache) {
      const cached = this.cache.get(url);
      if (cached && Date.now() - cached.timestamp < this.options.cacheExpiry) {
        if (!expectedSha256 || cached.package.archiveHash === expectedSha256) {
          console.log(`[ActrLoader] Use Cache: ${url}`);
          return cached.package;
        }

        console.log(`[ActrLoader] Cache hash Unsatisfactory expectations. Redownload.: ${url}`);
      }
    }

    let zipPath: string | null = null;
    let rootDir: string | null = null;

    try {
      // 1. Download .actr Documentation
      zipPath = await this.downloadFile(url);
      console.log(`[ActrLoader] Download completed`);

      // 2. Verify the same download .actr zip(if called by the hash)
      const archiveHash = await this.verifyDownloadedArchive(
        zipPath,
        expectedSha256,
      );

      // 3. Unpressure zip Documentation
      const extracted = await this.extractZip(zipPath);
      const { workDir } = extracted;
      rootDir = extracted.rootDir;
      console.log(`[ActrLoader] Depression complete.`);

      // 4. Parsing manifest.json
      const manifest = await this.parseManifest(workDir);
      console.log(
        `[ActrLoader] Parsing manifest: ${manifest.name} v${manifest.version}`,
      );

      // 5. Press manifest integrity Schedule verification files (optional)
      if (this.options.verifyIntegrity) {
        await this.verifyPackageIntegrity(workDir, manifest);
      }

      // 6. Read access files
      let unifiedScript = await this.readEntryFile(workDir, manifest.entry);
      console.log(
        `[ActrLoader] Read access files: ${manifest.entry} (${unifiedScript.length} bytes)`,
      );

      // 7. Copy picture resources to a persistent directory and replace relative paths in scripts
      const assetDir = `${ASSETS_ROOT_DIR}/${appId}`;
      const assetFileNames = await this.copyAssets(workDir, assetDir);

      if (assetFileNames.length > 0) {
        unifiedScript = this.rewriteAssetPaths(unifiedScript, assetDir, assetFileNames);
        console.log(
          `[ActrLoader] Replaced ${assetFileNames.length} Resource Path`,
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
      console.error(`[ActrLoader] Loading failed: ${url}`, error);
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
      throw new Error(`[ActrLoader] expected SHA-256 Invalid: ${expectedSha256}`);
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
        `[ActrLoader] .actr SHA-256 Verify Failed: expected ${expectedSha256}, got ${actualHash ?? 'invalid hash'}`,
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

    console.log(`[ActrLoader] Start Download: ${url} -> ${downloadPath}`);

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
   * @returns { workDir: Work Directory (includes) manifest.json ), rootDir: Root depressure directory (for cleanup)}
   */
  private async extractZip(
    zipPath: string,
  ): Promise<{ workDir: string; rootDir: string }> {
    const extractDir = normalizePath(`${RNFS.TemporaryDirectoryPath}/actr_${Date.now()}`);

    console.log(`[ActrLoader] Start depressure.:`);

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
      console.log(`[ActrLoader] Number of files depressed: ${extractedItems.length}`);
      extractedItems.forEach((item) => {
        console.log(
          `[ActrLoader] ${item.name} (${item.isDirectory() ? 'Contents' : 'Documentation'})`,
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
      console.error(`[ActrLoader] Unpressure failed:`, error);
      throw new Error(
        `[ActrLoader] Unpressure failed: ${error instanceof Error ? error.message : String(error)}`,
      );
    }
  }

  /**
   * Parsing manifest.json
   */
  private async parseManifest(extractDir: string): Promise<ActrManifest> {
    // Try to find manifest.json
    const manifestPath = `${extractDir}/manifest.json`;
    const exists = await RNFS.exists(manifestPath);

    if (!exists) {
      // Lists the contents of the directory to debug
      try {
        const items = await RNFS.readDir(extractDir);
        console.error(`[ActrLoader] manifest.json Cannot initialise Evolution's mail component.:`);
        items.forEach((item) => {
          console.error(
            `[ActrLoader] ${item.name} (${item.isDirectory() ? 'Contents' : 'Documentation'})`,
          );
        });
      } catch (error) {
        console.error(`[ActrLoader] Unable to read depresso directory:`, error);
      }
      throw new Error(`[ActrLoader] manifest.json Other Organiser: ${manifestPath}`);
    }

    // Read File Contents
    const content = await RNFS.readFile(manifestPath, 'utf8');
    console.log(`[ActrLoader] manifest.json Content Length: ${content.length}`);

    // Parsing JSON
    try {
      const manifest = parseActrManifest(content);

      console.log(
        `[ActrLoader] manifest Parsing successful: ${manifest.name} v${manifest.version}, entry: ${manifest.entry}`,
      );
      return manifest;
    } catch (error) {
      throw new Error(
        `[ActrLoader] manifest.json Parsing failed: ${error instanceof Error ? error.message : String(error)}`,
      );
    }
  }

  /**
   * Press Rust-owned manifest integrity Planned to verify the package document.
   */
  private async verifyPackageIntegrity(
    workDir: string,
    manifest: ActrManifest,
  ): Promise<void> {
    const plan = createActrIntegrityPlan(manifest);
    if (!plan) {
      return;
    }

    for (const file of plan.files) {
      const filePath = normalizePath(`${workDir}/${file.path}`);
      const exists = await RNFS.exists(filePath);
      if (!exists) {
        throw new Error(`[ActrLoader] integrity File does not exist: ${file.path}`);
      }

      const actualHash = (await RNFS.hash(filePath, plan.algorithm)).toLowerCase();
      if (actualHash !== file.expectedHash) {
        throw new Error(
          `[ActrLoader] integrity Verify Failed: ${file.path}, expected ${file.expectedHash}, got ${actualHash}`,
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

    console.log(`[ActrLoader] Find entry files: ${entryPath}`);

    for (const path of possiblePaths) {
      const exists = await RNFS.exists(path);
      if (exists) {
        const content = await RNFS.readFile(path, 'utf8');
        return content;
      }
    }

    throw new Error(`[ActrLoader] The entry file does not exist: ${entryPath}`);
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
      `[ActrLoader] Found ${relativePaths.length} A Photo Resource`,
    );

    // If the target directory already exists, clear it first.
    const dirExists = await RNFS.exists(assetDir);
    if (dirExists) {
      await RNFS.unlink(assetDir);
      console.log(`[ActrLoader] Emptyed old resource directory: ${assetDir}`);
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
          console.warn(`[ActrLoader] Resource file does not exist, Skip: ${srcPath}`);
          continue;
        }

        await RNFS.copyFile(srcPath, destPath);
        copiedFileNames.push(fileName);
      } catch (error) {
        console.warn(
          `[ActrLoader] Copying resource failed: ${srcPath} -> ${destPath}`,
          error,
        );
      }
    }

    console.log(
      `[ActrLoader] Copying Resource Complete: ${copiedFileNames.length}/${relativePaths.length}`,
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
          console.log(`[ActrLoader] temporary file deleted`);
        }
      }

      // Remove the depressure directory
      if (extractDir) {
        const dirExists = await RNFS.exists(extractDir);
        if (dirExists) {
          await RNFS.unlink(extractDir);
          // console.log(`[ActrLoader] Removed temporary directory: ${extractDir}`);
        }
      }
    } catch (error) {
      console.log(`[ActrLoader] Failed to clean temporary files:`, error);
      // Cleanup failure does not affect the main process
    }
  }

  /**
   * Clear Cache
   */
  clearCache(url?: string): void {
    if (url) {
      this.cache.delete(url);
      console.log(`[ActrLoader] Cleared Cache: ${url}`);
    } else {
      this.cache.clear();
      console.log(`[ActrLoader] Cleared all caches`);
    }
  }
}

/**
 * Special characters for transposition regular expression
 */
function escapeRegExp(str: string): string {
  return str.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

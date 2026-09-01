/**
 * .actr 文件加载器
 *
 * 功能：
 * 1. 从 URL 下载 .actr 文件（zip 包）
 * 2. 解压 zip 包
 * 3. 解析 manifest.json
 * 4. 读取入口文件（unified-app.js）
 * 5. 复制图片资源到持久化目录
 * 6. 替换脚本中的相对路径为绝对 file:// URI
 * 7. 返回可以直接传给 ActrTabView 的数据
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

/** 资源持久化存储的根目录 */
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
      cacheExpiry: options.cacheExpiry ?? 10 * 60 * 1000, // 10分钟
      timeout: options.timeout ?? 30000, // 30秒
      verifyIntegrity: options.verifyIntegrity ?? false,
    };
  }

  /**
   * 从 URL 加载 .actr 文件
   * @param url .actr 文件的 URL 地址
   * @param appId 应用 ID，用于资源存储目录命名
   * @returns 解析后的包数据
   */
  async load(
    url: string,
    appId: string,
    loadOptions: ActrLoadOptions = {},
  ): Promise<ActrPackage> {
    console.log(`[ActrLoader] 开始加载: ${url}`);
    const expectedSha256 = this.normalizeExpectedSha256(
      loadOptions.expectedSha256,
    );

    // 检查缓存
    if (this.options.enableCache) {
      const cached = this.cache.get(url);
      if (cached && Date.now() - cached.timestamp < this.options.cacheExpiry) {
        if (!expectedSha256 || cached.package.archiveHash === expectedSha256) {
          console.log(`[ActrLoader] 使用缓存: ${url}`);
          return cached.package;
        }

        console.log(`[ActrLoader] 缓存 hash 与期望不一致，重新下载: ${url}`);
      }
    }

    let zipPath: string | null = null;
    let rootDir: string | null = null;

    try {
      // 1. 下载 .actr 文件
      zipPath = await this.downloadFile(url);
      console.log(`[ActrLoader] 下载完成`);

      // 2. 校验下载得到的同一份 .actr zip（如果调用方提供 hash）
      const archiveHash = await this.verifyDownloadedArchive(
        zipPath,
        expectedSha256,
      );

      // 3. 解压 zip 文件
      const extracted = await this.extractZip(zipPath);
      const { workDir } = extracted;
      rootDir = extracted.rootDir;
      console.log(`[ActrLoader] 解压完成`);

      // 4. 解析 manifest.json
      const manifest = await this.parseManifest(workDir);
      console.log(
        `[ActrLoader] 解析 manifest: ${manifest.name} v${manifest.version}`,
      );

      // 5. 按 manifest integrity 计划校验文件（可选）
      if (this.options.verifyIntegrity) {
        await this.verifyPackageIntegrity(workDir, manifest);
      }

      // 6. 读取入口文件
      let unifiedScript = await this.readEntryFile(workDir, manifest.entry);
      console.log(
        `[ActrLoader] 读取入口文件: ${manifest.entry} (${unifiedScript.length} bytes)`,
      );

      // 7. 复制图片资源到持久化目录，并替换脚本中的相对路径
      const assetDir = `${ASSETS_ROOT_DIR}/${appId}`;
      const assetFileNames = await this.copyAssets(workDir, assetDir);

      if (assetFileNames.length > 0) {
        unifiedScript = this.rewriteAssetPaths(unifiedScript, assetDir, assetFileNames);
        console.log(
          `[ActrLoader] 已替换 ${assetFileNames.length} 个资源路径`,
        );
      }

      const result: ActrPackage = {
        manifest,
        unifiedScript,
        sourceUrl: url,
        archiveHash,
      };

      // 缓存结果
      if (this.options.enableCache) {
        this.cache.set(url, { package: result, timestamp: Date.now() });
      }

      return result;
    } catch (error) {
      console.error(`[ActrLoader] 加载失败: ${url}`, error);
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
      throw new Error(`[ActrLoader] expected SHA-256 无效: ${expectedSha256}`);
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
        `[ActrLoader] .actr SHA-256 校验失败: expected ${expectedSha256}, got ${actualHash ?? 'invalid hash'}`,
      );
    }

    return actualHash;
  }

  /**
   * 下载文件到临时目录
   */
  private async downloadFile(url: string): Promise<string> {
    const fileName = `actr_${Date.now()}.zip`;
    const downloadPath = normalizePath(`${RNFS.TemporaryDirectoryPath}/${fileName}`);

    console.log(`[ActrLoader] 开始下载: ${url} -> ${downloadPath}`);

    const downloadResult = await RNFS.downloadFile({
      fromUrl: url,
      toFile: downloadPath,
      connectionTimeout: this.options.timeout,
      readTimeout: this.options.timeout,
    }).promise;

    if (downloadResult.statusCode !== 200) {
      throw new Error(`下载失败: HTTP ${downloadResult.statusCode}`);
    }

    return downloadPath;
  }

  /**
   * 解压 zip 文件
   * @returns { workDir: 工作目录（包含 manifest.json 的目录）, rootDir: 根解压目录（用于清理）}
   */
  private async extractZip(
    zipPath: string,
  ): Promise<{ workDir: string; rootDir: string }> {
    const extractDir = normalizePath(`${RNFS.TemporaryDirectoryPath}/actr_${Date.now()}`);

    console.log(`[ActrLoader] 开始解压:`);

    // 检查 zip 文件是否存在
    const zipExists = await RNFS.exists(zipPath);

    if (!zipExists) {
      throw new Error(`Zip 文件不存在: ${zipPath}`);
    }

    // 获取 zip 文件大小
    try {
      const stat = await RNFS.stat(zipPath);
    } catch (error) {
      console.warn(`  - 无法获取 zip 文件信息:`, error);
    }

    // 创建解压目录
    await RNFS.mkdir(extractDir);

    // 使用 react-native-zip-archive 解压
    try {
      await unzip(zipPath, extractDir);

      // 验证解压结果
      const extractedItems = await RNFS.readDir(extractDir);
      console.log(`[ActrLoader] 解压后文件数量: ${extractedItems.length}`);
      extractedItems.forEach((item) => {
        console.log(
          `[ActrLoader] ${item.name} (${item.isDirectory() ? '目录' : '文件'})`,
        );
      });

      // 如果解压后只有一个目录，进入该目录作为工作目录
      if (extractedItems.length === 1 && extractedItems[0].isDirectory()) {
        const subDir = extractedItems[0].path;
        return { workDir: subDir, rootDir: extractDir };
      }

      // 否则直接使用解压目录
      return { workDir: extractDir, rootDir: extractDir };
    } catch (error) {
      console.error(`[ActrLoader] 解压失败:`, error);
      throw new Error(
        `[ActrLoader] 解压失败: ${error instanceof Error ? error.message : String(error)}`,
      );
    }
  }

  /**
   * 解析 manifest.json
   */
  private async parseManifest(extractDir: string): Promise<ActrManifest> {
    // 尝试查找 manifest.json
    const manifestPath = `${extractDir}/manifest.json`;
    const exists = await RNFS.exists(manifestPath);

    if (!exists) {
      // 列出目录内容以便调试
      try {
        const items = await RNFS.readDir(extractDir);
        console.error(`[ActrLoader] manifest.json 不存在，目录内容:`);
        items.forEach((item) => {
          console.error(
            `[ActrLoader] ${item.name} (${item.isDirectory() ? '目录' : '文件'})`,
          );
        });
      } catch (error) {
        console.error(`[ActrLoader] 无法读取解压目录:`, error);
      }
      throw new Error(`[ActrLoader] manifest.json 不存在，路径: ${manifestPath}`);
    }

    // 读取文件内容
    const content = await RNFS.readFile(manifestPath, 'utf8');
    console.log(`[ActrLoader] manifest.json 内容长度: ${content.length}`);

    // 解析 JSON
    try {
      const manifest = parseActrManifest(content);

      console.log(
        `[ActrLoader] manifest 解析成功: ${manifest.name} v${manifest.version}, entry: ${manifest.entry}`,
      );
      return manifest;
    } catch (error) {
      throw new Error(
        `[ActrLoader] manifest.json 解析失败: ${error instanceof Error ? error.message : String(error)}`,
      );
    }
  }

  /**
   * 按 Rust-owned manifest integrity 计划校验包内文件。
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
        throw new Error(`[ActrLoader] integrity 文件不存在: ${file.path}`);
      }

      const actualHash = (await RNFS.hash(filePath, plan.algorithm)).toLowerCase();
      if (actualHash !== file.expectedHash) {
        throw new Error(
          `[ActrLoader] integrity 校验失败: ${file.path}, expected ${file.expectedHash}, got ${actualHash}`,
        );
      }
    }
  }

  /**
   * 读取入口文件内容
   */
  private async readEntryFile(
    extractDir: string,
    entryPath: string,
  ): Promise<string> {
    // 支持多种可能的路径格式
    const possiblePaths = [
      `${extractDir}/${entryPath}`,
      `${extractDir}/dist/${entryPath}`,
      `${extractDir}/${entryPath.replace(/^dist\//, '')}`,
    ];

    console.log(`[ActrLoader] 查找入口文件: ${entryPath}`);

    for (const path of possiblePaths) {
      const exists = await RNFS.exists(path);
      if (exists) {
        const content = await RNFS.readFile(path, 'utf8');
        return content;
      }
    }

    throw new Error(`[ActrLoader] 入口文件不存在: ${entryPath}`);
  }

  /**
   * 递归扫描目录，收集所有图片资源文件的相对路径
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
          // 返回相对于 baseDir 的路径
          const relativePath = relativePathFrom(item.path, baseDir);
          results.push(relativePath);
        }
      }
    }

    return results;
  }

  /**
   * 复制图片资源到持久化目录
   * 如果目标目录已存在则先清空，再复制
   * @returns 成功复制的文件名列表（不含目录前缀）
   */
  private async copyAssets(
    workDir: string,
    assetDir: string,
  ): Promise<string[]> {
    // 扫描解压目录中的所有图片资源
    const relativePaths = await this.scanAssetFiles(workDir, workDir);

    if (relativePaths.length === 0) {
      return [];
    }

    console.log(
      `[ActrLoader] 发现 ${relativePaths.length} 个图片资源`,
    );

    // 如果目标目录已存在，先清空
    const dirExists = await RNFS.exists(assetDir);
    if (dirExists) {
      await RNFS.unlink(assetDir);
      console.log(`[ActrLoader] 已清空旧资源目录: ${assetDir}`);
    }

    // 创建目标目录
    await RNFS.mkdir(assetDir);

    // 逐个复制资源文件
    const copiedFileNames: string[] = [];

    for (const relativePath of relativePaths) {
      const srcPath = `${workDir}/${relativePath}`;
      const fileName = relativePath.split('/').pop()!;
      const destPath = `${assetDir}/${fileName}`;

      try {
        // 确保源文件存在
        const srcExists = await RNFS.exists(srcPath);
        if (!srcExists) {
          console.warn(`[ActrLoader] 资源文件不存在，跳过: ${srcPath}`);
          continue;
        }

        await RNFS.copyFile(srcPath, destPath);
        copiedFileNames.push(fileName);
      } catch (error) {
        console.warn(
          `[ActrLoader] 复制资源失败: ${srcPath} -> ${destPath}`,
          error,
        );
      }
    }

    console.log(
      `[ActrLoader] 资源复制完成: ${copiedFileNames.length}/${relativePaths.length}`,
    );

    return copiedFileNames;
  }

  /**
   * 替换脚本中的相对路径为 file:// URI
   *
   * Bun build 产物中的格式:
   *   module2.exports = "./clear-653n4c11.png";
   * 替换为:
   *   module2.exports = {uri:"file:///path/to/actr-assets/{appId}/clear-653n4c11.png"};
   *
   * 图片文件名带 content hash，误替换风险极低
   */
  private rewriteAssetPaths(
    script: string,
    assetDir: string,
    assetFileNames: string[],
  ): string {
    let result = script;

    for (const fileName of assetFileNames) {
      // 匹配 "./{fileName}" 或 "../{fileName}" 等相对路径字符串
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
   * 清理临时文件
   */
  private async cleanup(
    zipPath: string | null,
    extractDir: string | null,
  ): Promise<void> {
    try {
      // 删除 zip 文件
      if (zipPath) {
        const zipExists = await RNFS.exists(zipPath);
        if (zipExists) {
          await RNFS.unlink(zipPath);
          console.log(`[ActrLoader] 已删除临时文件`);
        }
      }

      // 删除解压目录
      if (extractDir) {
        const dirExists = await RNFS.exists(extractDir);
        if (dirExists) {
          await RNFS.unlink(extractDir);
          // console.log(`[ActrLoader] 已删除临时目录: ${extractDir}`);
        }
      }
    } catch (error) {
      console.log(`[ActrLoader] 清理临时文件失败:`, error);
      // 清理失败不影响主流程
    }
  }

  /**
   * 清除缓存
   */
  clearCache(url?: string): void {
    if (url) {
      this.cache.delete(url);
      console.log(`[ActrLoader] 已清除缓存: ${url}`);
    } else {
      this.cache.clear();
      console.log(`[ActrLoader] 已清除所有缓存`);
    }
  }
}

/**
 * 转义正则表达式特殊字符
 */
function escapeRegExp(str: string): string {
  return str.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

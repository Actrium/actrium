/**
 * .actr 文件加载器
 *
 * 功能：
 * 1. 从 URL 下载 .actr 文件（zip 包）
 * 2. 解压 zip 包
 * 3. 解析 manifest.json
 * 4. 读取入口文件（unified-app.js）
 * 5. 返回可以直接传给 ActrTabView 的数据
 */

import RNFS from 'react-native-fs';
import { unzip } from 'react-native-zip-archive';
import type {
  ActrManifest,
  ActrPackage,
  ActrLoaderOptions,
} from './ActrLoader.types';

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
   * @returns 解析后的包数据
   */
  async load(url: string): Promise<ActrPackage> {
    console.log(`[ActrLoader] 开始加载: ${url}`);

    // 检查缓存
    if (this.options.enableCache) {
      const cached = this.cache.get(url);
      if (cached && Date.now() - cached.timestamp < this.options.cacheExpiry) {
        console.log(`[ActrLoader] 使用缓存: ${url}`);
        return cached.package;
      }
    }

    try {
      // 1. 下载 .actr 文件
      const zipPath = await this.downloadFile(url);
      console.log(`[ActrLoader] 下载完成`);

      // 2. 解压 zip 文件
      const { workDir, rootDir } = await this.extractZip(zipPath);
      console.log(`[ActrLoader] 解压完成`);

      // 3. 解析 manifest.json
      const manifest = await this.parseManifest(workDir);
      console.log(
        `[ActrLoader] 解析 manifest: ${manifest.name} v${manifest.version}`,
      );

      // 4. 读取入口文件
      const unifiedScript = await this.readEntryFile(workDir, manifest.entry);
      console.log(
        `[ActrLoader] 读取入口文件: ${manifest.entry} (${unifiedScript.length} bytes)`,
      );

      // 5. 清理临时文件（使用根目录）
      await this.cleanup(zipPath, rootDir);

      const result: ActrPackage = {
        manifest,
        unifiedScript,
        sourceUrl: url,
      };

      // 缓存结果
      if (this.options.enableCache) {
        this.cache.set(url, { package: result, timestamp: Date.now() });
      }

      return result;
    } catch (error) {
      console.error(`[ActrLoader] 加载失败: ${url}`, error);
      throw error;
    }
  }

  /**
   * 下载文件到临时目录
   */
  private async downloadFile(url: string): Promise<string> {
    const fileName = `actr_${Date.now()}.zip`;
    const downloadPath = `${RNFS.TemporaryDirectoryPath}/${fileName}`;

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
    const extractDir = `${RNFS.TemporaryDirectoryPath}/actr_${Date.now()}`;

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
      const manifest = JSON.parse(content) as ActrManifest;

      // 验证必需字段
      if (!manifest.name || !manifest.version || !manifest.entry) {
        throw new Error('[ActrLoader] manifest.json 缺少必需字段 (name, version, entry)');
      }

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
   * 清理临时文件
   */
  private async cleanup(zipPath: string, extractDir: string): Promise<void> {
    try {
      // 删除 zip 文件
      const zipExists = await RNFS.exists(zipPath);
      if (zipExists) {
        await RNFS.unlink(zipPath);
        console.log(`[ActrLoader] 已删除临时文件`);
      }

      // 删除解压目录
      const dirExists = await RNFS.exists(extractDir);
      if (dirExists) {
        await RNFS.unlink(extractDir);
        // console.log(`[ActrLoader] 已删除临时目录: ${extractDir}`);
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

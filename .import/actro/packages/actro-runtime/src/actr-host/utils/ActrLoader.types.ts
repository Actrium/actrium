/**
 * .actr 文件加载器类型定义
 */

import type { ActrManifest } from '../coreContract';

export type { ActrManifest } from '../coreContract';
export { ASSET_EXTENSIONS } from '../coreContract';

/**
 * 解析后的 .actr 包数据
 */
export interface ActrPackage {
  manifest: ActrManifest;
  unifiedScript: string; // 入口文件的代码内容
  sourceUrl: string; // 原始 .actr 文件的 URL
  archiveHash?: string; // 下载得到的 .actr zip SHA-256（如果调用方要求校验）
}

/**
 * 单次加载选项
 */
export interface ActrLoadOptions {
  expectedSha256?: string | null; // 期望的 .actr zip SHA-256
}

/**
 * 加载选项
 */
export interface ActrLoaderOptions {
  enableCache?: boolean; // 是否启用缓存
  cacheExpiry?: number; // 缓存过期时间（毫秒）
  timeout?: number; // 下载超时时间（毫秒）
  verifyIntegrity?: boolean; // 是否验证文件完整性
}

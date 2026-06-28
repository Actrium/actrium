import { NativeModules } from 'react-native';
import { ActrLoader } from './ActrLoader';
import type {
  ActrLoaderOptions,
  ActrManifest,
  ActrPackage,
} from './ActrLoader.types';

export const ENABLE_DEV_LOCAL_ACTR_SCRIPT = true;

/**
 * dev 本地加载模式：
 * - 'js'   直接加载裸 JS 文件（如 unified-app.js）
 * - 'actr' 直接加载 .actr 包文件（如 counterapp.actr），走完整的解压解析流程
 */
type DevLocalActrMode = 'js' | 'actr';
const DEV_LOCAL_ACTR_MODE: DevLocalActrMode = 'actr';

const DEV_LOCAL_ACTR_SCRIPT_PORT = 3000;
const DEV_LOCAL_ACTR_SCRIPT_FILE = 'unified-app.js';
const DEV_LOCAL_ACTR_PACKAGE_FILE = 'counterapp.actr';

function isDevRuntime(): boolean {
  return false
  return typeof (globalThis as any).__DEV__ === 'boolean'
    ? !!(globalThis as any).__DEV__
    : true;
}

function resolveMetroHost(): string {
  try {
    const sourceCode = NativeModules.SourceCode;
    const scriptURL: string | undefined =
      sourceCode?.getConstants?.()?.scriptURL ?? sourceCode?.scriptURL;
    const match = scriptURL?.match(/^https?:\/\/([^:/]+)/);
    if (match?.[1]) {
      return match[1];
    }
  } catch {
    // 开发环境读取 SourceCode 失败时，回退到本机模拟器常用地址。
  }

  return 'localhost';
}

function resolveDevLocalScriptUrl(): string {
  return `http://${resolveMetroHost()}:${DEV_LOCAL_ACTR_SCRIPT_PORT}/${DEV_LOCAL_ACTR_SCRIPT_FILE}`;
}

function resolveDevLocalActrUrl(): string {
  return `http://${resolveMetroHost()}:${DEV_LOCAL_ACTR_SCRIPT_PORT}/${DEV_LOCAL_ACTR_PACKAGE_FILE}`;
}

function createDevManifest(sourceUrl: string): ActrManifest {
  return {
    name: 'dev-local-actr-script',
    version: '0.0.0-dev',
    entry: DEV_LOCAL_ACTR_SCRIPT_FILE,
    description: `Loaded from ${sourceUrl}`,
  };
}

export class DevActrLoader extends ActrLoader {
  async load(url: string, appId: string): Promise<ActrPackage> {
    if (!ENABLE_DEV_LOCAL_ACTR_SCRIPT || !isDevRuntime()) {
      return super.load(url, appId);
    }

    if (DEV_LOCAL_ACTR_MODE === 'actr') {
      return this.loadLocalActr(url, appId);
    }

    return this.loadLocalScript(url, appId);
  }

  /**
   * 'actr' 模式：直接加载本地 .actr 包文件，走完整的解压解析流程
   */
  private async loadLocalActr(url: string, appId: string): Promise<ActrPackage> {
    const actrUrl = resolveDevLocalActrUrl();
    try {
      const result = await super.load(actrUrl, appId);
      console.log(
        `[DevActrLoader] Loaded local .actr from ${actrUrl}, script size: ${result.unifiedScript.length} bytes`,
      );
      return result;
    } catch (error) {
      console.warn(
        `[DevActrLoader] Failed to load local .actr from ${actrUrl}, fallback to remote .actr: ${url}`,
        error,
      );
      return super.load(url, appId);
    }
  }

  /**
   * 'js' 模式：直接加载裸 JS 文件
   */
  private async loadLocalScript(url: string, appId: string): Promise<ActrPackage> {
    const sourceUrl = resolveDevLocalScriptUrl();
    try {
      const response = await fetch(sourceUrl);

      if (!response.ok) {
        throw new Error(
          `Failed to fetch dev local actr script: ${response.status} ${response.statusText}`,
        );
      }

      const unifiedScript = await response.text();
      console.log(
        `[DevActrLoader] Loaded dev local script from ${sourceUrl}, script size: ${unifiedScript.length} bytes`,
      );

      return {
        manifest: createDevManifest(sourceUrl),
        unifiedScript,
        sourceUrl,
      };
    } catch (error) {
      console.warn(
        `[DevActrLoader] Failed to load dev local script, fallback to .actr: ${sourceUrl}`,
        error,
      );
      return super.load(url, appId);
    }
  }
}

export function createActrLoader(options: ActrLoaderOptions = {}): ActrLoader {
  return ENABLE_DEV_LOCAL_ACTR_SCRIPT && isDevRuntime()
    ? new DevActrLoader(options)
    : new ActrLoader(options);
}

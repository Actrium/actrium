import { NativeModules } from 'react-native';
import { ActrPackageLoader } from './ActrPackageLoader';
import type {
  ActrPackageLoaderOptions,
  ActrPackage,
} from './ActrPackageLoader.types';

export const ENABLE_DEV_LOCAL_ACTR_SCRIPT = true;

/**
 * Development mode loads a local .actr through the same verification and
 * extraction path as a remotely distributed package.
 */
const DEV_LOCAL_ACTR_SCRIPT_PORT = 3000;
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
    // Fall back to the conventional simulator address when SourceCode is unavailable.
  }

  return 'localhost';
}

function resolveDevLocalActrUrl(): string {
  return `http://${resolveMetroHost()}:${DEV_LOCAL_ACTR_SCRIPT_PORT}/${DEV_LOCAL_ACTR_PACKAGE_FILE}`;
}

export class DevActrPackageLoader extends ActrPackageLoader {
  async load(url: string, appId: string): Promise<ActrPackage> {
    if (!ENABLE_DEV_LOCAL_ACTR_SCRIPT || !isDevRuntime()) {
      return super.load(url, appId);
    }

    return this.loadLocalActr(url, appId);
  }

  /**
   * Load a local .actr package through the complete extraction and parsing path.
   */
  private async loadLocalActr(url: string, appId: string): Promise<ActrPackage> {
    const actrUrl = resolveDevLocalActrUrl();
    try {
      const result = await super.load(actrUrl, appId);
      console.log(
        `[DevActrPackageLoader] Loaded local .actr from ${actrUrl}, script size: ${result.unifiedScript.length} bytes`,
      );
      return result;
    } catch (error) {
      console.warn(
        `[DevActrPackageLoader] Failed to load local .actr from ${actrUrl}, fallback to remote .actr: ${url}`,
        error,
      );
      return super.load(url, appId);
    }
  }

}

export function createActrPackageLoader(options: ActrPackageLoaderOptions = {}): ActrPackageLoader {
  return ENABLE_DEV_LOCAL_ACTR_SCRIPT && isDevRuntime()
    ? new DevActrPackageLoader(options)
    : new ActrPackageLoader(options);
}

import { Platform } from 'react-native';
import ImageResizer from '@bam.tech/react-native-image-resizer';
import { http } from '../api/https';
import {
  AUTH_ENDPOINT_CONTRACT,
  AUTH_ENVIRONMENT_CONTRACT,
  authResourceUploaderBaseUrl,
  type AuthEnvironment,
} from '../runtime-host/coreContract';

// Polyfill for COS SDK which expects navigator.userAgent and location in browser environments
if (typeof navigator === 'object' && !navigator.userAgent) {
  (navigator as any).userAgent = `ReactNative/${Platform.Version} ${Platform.OS}`;
}
if (typeof (global as any).location === 'undefined') {
  (global as any).location = { protocol: 'https:' };
}
if (typeof (global as any).Blob === 'undefined') {
  (global as any).Blob = function () {};
}

// Use require to ensure polyfill is applied before COS initialization
const COS = require('cos-js-sdk-v5');

export interface UploadResult {
  localPath: string;
  remotePath: string | null;
  cdnHost: string | null;
  cdnUrl: string | null;
  success: boolean;
  errorMessage?: string;
}

export class ImageUploadService {
  private static getEnv(): AuthEnvironment {
    // Default Use beta Environment, unless you can find a more accurate way of judging.
    return AUTH_ENVIRONMENT_CONTRACT.environments.beta;
  }

  private static getBaseUrl() {
    const env = this.getEnv();
    return authResourceUploaderBaseUrl(env);
  }

  /**
   * Get temporary vouchers
   */
  private static async fetchCredentials(fileName: string) {
    const baseUrl = this.getBaseUrl();
    const url = `${baseUrl}${AUTH_ENDPOINT_CONTRACT.paths.resourceUploadToken}?file_name=${encodeURIComponent(fileName)}`;
    
    const res = await http.get<any>(url);
    if (res.success && res.data) {
      return res.data;
    }
    throw new Error(res.message || 'Failed to fetch COS credentials');
  }

  /**
   * If the picture is larger 5MB, compression
   */
  public static async compressIfNeeded(uri: string, fileSize?: number): Promise<{ uri: string; compressed: boolean }> {
    // If not imported fileSizeor fileSize > 5MB (5 * 1024 * 1024)
    if (fileSize === undefined || fileSize > 5 * 1024 * 1024) {
      console.log(`[ImageUploadService] Compressing image: ${uri}, original size: ${fileSize}`);
      try {
        const result = await ImageResizer.createResizedImage(
          uri,
          2000, // maxWidth
          2000, // maxHeight
          'JPEG',
          80, // quality
          0, // rotation
          undefined, // outputPath
          false, // keepMeta
          { mode: 'contain', onlyScaleDown: true }
        );
        console.log(`[ImageUploadService] Compressed image size: ${result.size}`);
        return { uri: result.uri, compressed: true };
      } catch (err) {
        console.warn('[ImageUploadService] Compression failed, using original:', err);
        return { uri, compressed: false };
      }
    }
    return { uri, compressed: false };
  }

  /**
   * Upload pictures to COS
   */
  public static async uploadImage(uri: string, fileName: string, fileSize?: number): Promise<UploadResult> {
    try {
      // 1. Compression (If you need to.)
      const { uri: uploadUri } = await this.compressIfNeeded(uri, fileSize);

      // 2. Access Token
      const tokenData = await this.fetchCredentials(fileName);
      const { credentials, cdnHost, fileName: remoteFileName } = tokenData;

      if (!credentials) {
        throw new Error('Missing credentials in token response');
      }

      // 3. Initialize COS
      const cos = new COS({
        getAuthorization: (options: any, callback: any) => {
          callback({
            TmpSecretId: credentials.tmpSecretId,
            TmpSecretKey: credentials.tmpSecretKey,
            XCosSecurityToken: credentials.sessionToken,
            StartTime: tokenData.startTime,
            ExpiredTime: tokenData.expiredTime,
          });
        },
      });

      // 4. Execute Upload
      return new Promise((resolve) => {
        const bucket = "test-5d89e75d6db2d-1253159997"; // with Swift Same version.
        const region = "ap-beijing";

        // Process URI (React Native System path may need to be processed.)
        const cleanUri = uploadUri.startsWith('file://') ? uploadUri : `file://${uploadUri}`;

        const body = { 
          uri: cleanUri, 
          name: fileName, 
          type: 'image/jpeg',
          toString: () => '[object Blob]' // - Liar. SDK ♪ Check it out so it thinks it's a Blob
        };

        cos.putObject({
          Bucket: bucket,
          Region: region,
          Key: remoteFileName || fileName,
          Body: body as any, 
        }, (err: any, data: any) => {
          if (err) {
            console.error('[ImageUploadService] COS Upload Error:', err);
            resolve({
              localPath: uri,
              remotePath: null,
              cdnHost: cdnHost,
              cdnUrl: null,
              success: false,
              errorMessage: err.message || 'COS upload failed',
            });
          } else {
            const finalPath = remoteFileName || fileName;
            const cdnUrl = `${cdnHost}${finalPath}`;
            console.log('[ImageUploadService] Upload successful:', cdnUrl);
            
            resolve({
              localPath: uri,
              remotePath: finalPath,
              cdnHost: cdnHost,
              cdnUrl: cdnUrl,
              success: true,
            });
          }
        });
      });
    } catch (error: any) {
      console.error('[ImageUploadService] Upload Flow Error:', error);
      return {
        localPath: uri,
        remotePath: null,
        cdnHost: null,
        cdnUrl: null,
        success: false,
        errorMessage: error.message,
      };
    }
  }
}

import { Platform } from 'react-native';
import ImageResizer from '@bam.tech/react-native-image-resizer';
import { http } from '../api/https';

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
  private static getEnv() {
    // 默认使用 beta 环境，除非能找到更准确的判断方式
    return 'beta'; 
  }

  private static getBaseUrl() {
    const env = this.getEnv();
    switch (env) {
      case 'dev':
        return 'https://resource-uploader.develenv.com';
      case 'beta':
        return 'https://resource-uploader.betaenv.com';
      case 'prod':
        return 'https://resource-uploader.askaway.chat';
      default:
        return 'https://resource-uploader.askaway.chat';
    }
  }

  /**
   * 获取临时凭证
   */
  private static async fetchCredentials(fileName: string) {
    const baseUrl = this.getBaseUrl();
    const url = `${baseUrl}/api/get-token?file_name=${encodeURIComponent(fileName)}`;
    
    const res = await http.get<any>(url);
    if (res.success && res.data) {
      return res.data;
    }
    throw new Error(res.message || 'Failed to fetch COS credentials');
  }

  /**
   * 如果图片大于 5MB，进行压缩
   */
  public static async compressIfNeeded(uri: string, fileSize?: number): Promise<{ uri: string; compressed: boolean }> {
    // 如果没有传入 fileSize，或者 fileSize > 5MB (5 * 1024 * 1024)
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
   * 上传图片到 COS
   */
  public static async uploadImage(uri: string, fileName: string, fileSize?: number): Promise<UploadResult> {
    try {
      // 1. 压缩 (如果需要)
      const { uri: uploadUri } = await this.compressIfNeeded(uri, fileSize);

      // 2. 获取 Token
      const tokenData = await this.fetchCredentials(fileName);
      const { credentials, cdnHost, fileName: remoteFileName } = tokenData;

      if (!credentials) {
        throw new Error('Missing credentials in token response');
      }

      // 3. 初始化 COS
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

      // 4. 执行上传
      return new Promise((resolve) => {
        const bucket = "test-5d89e75d6db2d-1253159997"; // 与 Swift 版一致
        const region = "ap-beijing";

        // 处理 URI (React Native 系统路径可能需要处理)
        const cleanUri = uploadUri.startsWith('file://') ? uploadUri : `file://${uploadUri}`;

        const body = { 
          uri: cleanUri, 
          name: fileName, 
          type: 'image/jpeg',
          toString: () => '[object Blob]' // 欺骗 SDK 的校验，让它认为这是一个 Blob
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

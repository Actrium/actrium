import { Buffer } from 'buffer';
import { v4 as uuidv4 } from 'uuid';

export class Utils {
  /**
   * 生成随机ID
   * @returns 随机ID
   */
  static generateId = () => {
    const myId = uuidv4();
    return myId;
  };

  /**
   * 将 Uint8Array 转换为 base64 字符串
   * @param data Uint8Array 数组
   * @returns base64 字符串
   */
  static uint8ArrayToBase64 = (data: Uint8Array): string => {
    return Buffer.from(data).toString('base64');
  };
}

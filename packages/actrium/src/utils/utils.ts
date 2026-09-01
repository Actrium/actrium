import { Buffer } from 'buffer';
import { v4 as uuidv4 } from 'uuid';

export class Utils {
  /**
   * Generate RandomID
   * @returns RandomID
   */
  static generateId = () => {
    const myId = uuidv4();
    return myId;
  };

  /**
   * Will Uint8Array Convert to base64 String
   * @param data Uint8Array Array
   * @returns base64 String
   */
  static uint8ArrayToBase64 = (data: Uint8Array): string => {
    return Buffer.from(data).toString('base64');
  };
}

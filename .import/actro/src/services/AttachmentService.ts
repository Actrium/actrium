import { launchCamera, launchImageLibrary, Asset } from 'react-native-image-picker';
import DocumentPicker, { DocumentPickerResponse } from 'react-native-document-picker';
import { Alert } from 'react-native';

export class AttachmentService {
  /**
   * 拍照
   */
  static async takePhoto(): Promise<Asset[]> {
    try {
      const result = await launchCamera({
        mediaType: 'photo',
        quality: 0.8,
      });
      return result.assets || [];
    } catch (error) {
      console.error('[AttachmentService] Camera error:', error);
      Alert.alert('错误', '打开相机失败');
      return [];
    }
  }

  /**
   * 从相册选择
   */
  static async pickImage(): Promise<Asset[]> {
    try {
      const result = await launchImageLibrary({
        mediaType: 'photo',
        quality: 0.8,
        selectionLimit: 0, // 0 means no limit (up to 6 will be handled by UI)
      });
      return result.assets || [];
    } catch (error) {
      console.error('[AttachmentService] Gallery error:', error);
      Alert.alert('错误', '打开相册失败');
      return [];
    }
  }

  /**
   * 选择文档 (限制 PDF/Word/Excel/PPT/TXT)
   */
  static async pickDocument(): Promise<DocumentPickerResponse[]> {
    try {
      const results = await DocumentPicker.pick({
        allowMultiSelection: true,
        type: [
          DocumentPicker.types.pdf,
          DocumentPicker.types.doc,
          DocumentPicker.types.docx,
          DocumentPicker.types.xls,
          DocumentPicker.types.xlsx,
          DocumentPicker.types.ppt,
          DocumentPicker.types.pptx,
          DocumentPicker.types.plainText,
          DocumentPicker.types.zip,
        ],
      });
      return results;
    } catch (err) {
      if (DocumentPicker.isCancel(err)) {
        console.log('[AttachmentService] Document selection cancelled');
      } else {
        console.error('[AttachmentService] Document picker error:', err);
        Alert.alert('错误', '选择文件出错');
      }
      return [];
    }
  }
}

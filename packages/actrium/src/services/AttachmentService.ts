import { launchCamera, launchImageLibrary, Asset } from 'react-native-image-picker';
import i18n from '../i18n';
import type { DocumentPickerResponse } from '@react-native-documents/picker';
import { Alert, Platform } from 'react-native';
import { GlobalAlert } from '../components';

export class AttachmentService {
  private static loadDocumentPicker() {
    try {
      return require('@react-native-documents/picker') as typeof import('@react-native-documents/picker');
    } catch (error) {
      console.error('[AttachmentService] DocumentPicker native module unavailable:', error);
      return null;
    }
  }

  /**
   * Take pictures.
   */
  static async takePhoto(): Promise<Asset[]> {
    try {
      const result = await launchCamera({
        mediaType: 'photo',
        quality: 0.8,
        includeBase64: false,
      });

      if (result.didCancel) {
        return [];
      }
      if (result.errorCode) {
        console.error('[AttachmentService] Camera errorCode:', result.errorCode, result.errorMessage);
        GlobalAlert.alert('Error', result.errorMessage || 'Failed to open camera');
        return [];
      }

      return result.assets || [];
    } catch (error) {
      console.error('[AttachmentService] Camera error:', error);
      GlobalAlert.alert(i18n.t('common.error'), i18n.t('actr.attachment.cameraError'));
      return [];
    }
  }

  /**
   * Select From Album
   */
  static async pickImage(): Promise<Asset[]> {
    try {
      const result = await launchImageLibrary({
        mediaType: 'photo',
        quality: 0.8,
        selectionLimit: 0,
        includeBase64: false,
      });

      if (result.didCancel) {
        return [];
      }
      if (result.errorCode) {
        console.error('[AttachmentService] Gallery errorCode:', result.errorCode, result.errorMessage);
        GlobalAlert.alert('Error', result.errorMessage || 'Failed to open album');
        return [];
      }

      return result.assets || [];
    } catch (error) {
      console.error('[AttachmentService] Gallery error:', error);
      GlobalAlert.alert(i18n.t('common.error'), i18n.t('actr.attachment.galleryError'));
      return [];
    }
  }

  /**
   * Select Document (Limits PDF/Word/Excel/PPT/TXT)
   */
  static async pickDocument(): Promise<DocumentPickerResponse[]> {
    try {
      const documentPicker = AttachmentService.loadDocumentPicker();
      if (!documentPicker) {
        GlobalAlert.alert('Error', 'Document selection module not available');
        return [];
      }

      const { pick, types, keepLocalCopy, isErrorWithCode, errorCodes } = documentPicker;

      const results = await pick({
        allowMultiSelection: true,
        type: [
          types.pdf,
          types.doc,
          types.docx,
          types.xls,
          types.xlsx,
          types.ppt,
          types.pptx,
          types.plainText,
          types.zip,
        ],
      });

      if (Platform.OS === 'android') {
        const files = results.map((item, index) => ({
          uri: item.uri,
          fileName: item.name || `document-${Date.now()}-${index}`,
        })) as [
          { uri: string; fileName: string },
          ...Array<{ uri: string; fileName: string }>
        ];

        const copies = await keepLocalCopy({
          files,
          destination: 'cachesDirectory',
        });

        const copiedUriMap = new Map<string, string>();
        copies.forEach((copyResult) => {
          if (copyResult.status === 'success') {
            copiedUriMap.set(copyResult.sourceUri, copyResult.localUri);
          }
        });

        return results.map((item) => ({
          ...item,
          uri: copiedUriMap.get(item.uri) || item.uri,
        }));
      }

      return results;
    } catch (err) {
      const documentPicker = AttachmentService.loadDocumentPicker();
      if (
        documentPicker &&
        documentPicker.isErrorWithCode(err) &&
        err.code === documentPicker.errorCodes.OPERATION_CANCELED
      ) {
        console.log('[AttachmentService] Document selection cancelled');
      } else {
        console.error('[AttachmentService] Document picker error:', err);
        GlobalAlert.alert(i18n.t('common.error'), i18n.t('actr.attachment.fileError'));
      }
      return [];
    }
  }
}

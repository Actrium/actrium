import { Alert, Platform } from 'react-native';
import { useTranslation } from 'react-i18next';
import { IAttachmentItem } from '../types/attachment';
import { AttachmentService } from '../../services/AttachmentService';
import { useInputStore, EMPTY_ATTACHMENTS } from '../store/useInputStore';
import RNFS from 'react-native-fs';
import { Utils } from '../../utils/utils';
import { GlobalAlert } from '../../../src/components';

export const useAttachmentManager = (appId: string) => {
  const { t } = useTranslation();
  const attachments = useInputStore(
    (state) => state.inputs[appId]?.attachments || EMPTY_ATTACHMENTS,
  );
  const setStoreAttachments = useInputStore((state) => state.setAttachments);

  const removeAttachment = (id: string) => {
    const next = attachments.filter((item) => item.id !== id);
    setStoreAttachments(appId, next);
  };

  const clearAttachments = () => {
    setStoreAttachments(appId, []);
  };

  const copyAssetToTemp = async (
    uri: string,
    filename: string,
    id: string,
  ): Promise<string> => {
    try {
      const tempDir = `${RNFS.TemporaryDirectoryPath}/attachments`;
      const exists = await RNFS.exists(tempDir);
      if (!exists) {
        await RNFS.mkdir(tempDir);
      }

      const destPath = `${tempDir}/${id}-${filename}`;

      if (Platform.OS === 'android' && uri.startsWith('content://')) {
        // Android content:// URI: read as base64 then write to temp file
        const base64Data = await RNFS.readFile(uri, 'base64');
        await RNFS.writeFile(destPath, base64Data, 'base64');
      } else {
        // iOS or file:// URI: direct copy
        const cleanUri = uri.startsWith('file://')
          ? uri.replace('file://', '')
          : uri;
        await RNFS.copyFile(cleanUri, destPath);
      }

      return `file://${destPath}`;
    } catch (error) {
      console.error('[useAttachmentManager] copyAssetToTemp failed:', error);
      return uri;
    }
  };

  const handleAddAttachment = async (
    type: 'image' | 'file',
    getAssets: () => Promise<any[]>,
  ) => {
    const hasConflict =
      attachments.length > 0 && attachments[0].attachment_type !== type;

    if (hasConflict) {
      const confirmed = await new Promise<boolean>((resolve) => {
        GlobalAlert.alert(t('common.tip'), t('actr.attachment.overwriteConfirm'), [
          { text: t('common.cancel'), style: 'cancel', onPress: () => resolve(false) },
          {
            text: t('common.confirm'),
            onPress: () => resolve(true),
          },
        ]);
      });
      if (!confirmed) return;
    } else if (attachments.length >= 6) {
      GlobalAlert.alert(t('common.tip'), t('actr.attachment.limitReached'));
      return;
    }

    let results: any[];
    try {
      results = await getAssets();
    } catch (error) {
      console.error('[useAttachmentManager] getAssets failed:', error);
      return;
    }

    if (!results || results.length === 0) {
      return;
    }

    // 检查文件大小是否超过 10MB
    const maxSize = 10 * 1024 * 1024;
    const validResults = results.filter((res) => {
      const size = res.fileSize || res.size || 0;
      return size <= maxSize;
    });

    if (validResults.length < results.length) {
      GlobalAlert.alert(t('actr.attachment.fileTooLarge'), t('actr.attachment.fileSizeLimit'));
    }

    // Check for unsupported file types (e.g., no type found)
    const supportedResults = validResults.filter((res) => {
      // Simple check: if it's a file but has no type/extension that we filters for
      // DocumentPicker usually returns types like 'application/pdf', etc.
      return !!res.type || !!res.mimeType || type === 'image';
    });

    if (supportedResults.length < validResults.length) {
      GlobalAlert.alert(t('common.tip'), t('actr.attachment.unsupportedType'));
    }

    if (supportedResults.length === 0) return;

    const isDifferentType =
      attachments.length > 0 && attachments[0].attachment_type !== type;
    const currentList = isDifferentType ? [] : attachments;
    const remainingSlots = 6 - currentList.length;

    if (remainingSlots <= 0) return;

    const newItemsToAdd: IAttachmentItem[] = await Promise.all(
      supportedResults.slice(0, remainingSlots).map(async (res) => {
        const id = Utils.generateId();
        const fileName =
          res.fileName ||
          res.name ||
          (type === 'image' ? `img-${Date.now()}.jpg` : `file-${Date.now()}`);

        // 复制到临时目录
        const localUrl = await copyAssetToTemp(res.uri, fileName, id);

        return {
          id,
          attachment_type: type,
          local_url: localUrl,
          cdn_url: '',
          type: res.type,
          name: fileName,
          aspect_ratio: res.width && res.height ? res.width / res.height : 1,
        };
      }),
    );

    if (supportedResults.length > remainingSlots) {
      GlobalAlert.alert(
        t('common.tip'),
        t('actr.attachment.limitPartial', { count: remainingSlots }),
      );
    }

    setStoreAttachments(appId, [...currentList, ...newItemsToAdd]);
  };

  const handleConfirmLocation = async (data: {
    latitude: number;
    longitude: number;
    snapshot: string;
  }) => {
    const hasConflict =
      attachments.length > 0 && attachments[0].attachment_type !== 'location';
    if (hasConflict) {
      const confirmed = await new Promise<boolean>((resolve) => {
        GlobalAlert.alert(t('common.tip'), t('actr.attachment.overwriteConfirm'), [
          { text: t('common.cancel'), style: 'cancel', onPress: () => resolve(false) },
          { text: t('common.confirm'), onPress: () => resolve(true) },
        ]);
      });
      if (!confirmed) return;
    }

    const id = Utils.generateId();

    // Android 无截图，iOS 有截图
    let localUrl = '';
    if (data.snapshot) {
      const snapshotFileName = `map-${Date.now()}.png`;
      localUrl = await copyAssetToTemp(data.snapshot, snapshotFileName, id);
    }

    const locationItem: IAttachmentItem = {
      id: id,
      attachment_type: 'location',
      local_url: localUrl,
      cdn_url: '',
      latitude: data.latitude,
      longitude: data.longitude,
      place_name: t('actr.chat.locationInfo'),
      aspect_ratio: 1,
    };

    setStoreAttachments(appId, [locationItem]);
  };

  return {
    attachments,
    removeAttachment,
    clearAttachments,
    handleAddAttachment,
    handleConfirmLocation,
    takePhoto: () => handleAddAttachment('image', AttachmentService.takePhoto),
    pickImage: () => handleAddAttachment('image', AttachmentService.pickImage),
    pickDocument: () =>
      handleAddAttachment('file', AttachmentService.pickDocument),
  };
};

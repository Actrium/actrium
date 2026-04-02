import React from 'react';
import {
  View,
  TouchableOpacity,
  StyleSheet,
  Linking,
  Platform,
  ScrollView as RNScrollView,
} from 'react-native';
import { IAttachmentItem } from '../types/attachment';
import { SmartImage } from './SmartImage';
import { Icon } from './Icon';
import { AppText } from '../../components';
import {
  attachmentPreview,
  getTypographyStyle,
  spacing,
} from '../../theme/tokens';

interface AttachmentPreviewBarProps {
  attachments: IAttachmentItem[];
  removeAttachment: (id: string) => void;
  hideSeparator?: boolean;
}

export const AttachmentPreviewBar: React.FC<AttachmentPreviewBarProps> = ({
  attachments,
  removeAttachment,
  hideSeparator = false,
}) => {
  if (attachments.length === 0) return null;

  const getImagePreviewWidth = (item: IAttachmentItem): number => {
    const ratio = item.aspect_ratio && item.aspect_ratio > 0
      ? item.aspect_ratio
      : 1;
    return Math.max(57, Math.round(82 * ratio));
  };

  const getDisplayFileName = (item: IAttachmentItem): string => {
    const legacyName = (item as any).name as string | undefined;
    const fileName = item.file_name || legacyName;
    if (fileName) return fileName;

    const rawName = item.local_url.split('/').pop() || '';
    try {
      return decodeURIComponent(rawName);
    } catch {
      return rawName;
    }
  };

  const getDisplayFileType = (item: IAttachmentItem): string => {
    const name = getDisplayFileName(item);
    const parts = name.split('.');
    if (parts.length > 1 && parts[parts.length - 1]) {
      return parts[parts.length - 1].toUpperCase();
    }

    const legacyType = (item as any).type as string | undefined;
    const fileType = item.file_type || legacyType || '';
    if (fileType.includes('/')) {
      return fileType.split('/').pop()!.toUpperCase();
    }
    return fileType.toUpperCase();
  };

  return (
    <>
      <RNScrollView
        horizontal
        showsHorizontalScrollIndicator={false}
        contentContainerStyle={styles.attachmentList}
        style={styles.attachmentScroll}
      >
        {attachments.map((item) => (
          <View
            key={item.id}
            style={[
              styles.attachmentItem,
              // 中文注释：图片附件维持固定高度，并按原始比例撑开；过窄时由最小宽度兜住，避免竖图缩成一条。
              item.attachment_type === 'image'
                ? [
                    styles.imageAttachmentItem,
                    { width: getImagePreviewWidth(item) },
                  ]
                : null,
            ]}
          >
            {item.attachment_type === 'image' ? (
              <View style={styles.imagePreviewFrame}>
                <SmartImage
                  cdnUrl={item.cdn_url}
                  localUrl={item.local_url}
                  style={styles.previewImageContain}
                  resizeMode='cover'
                />
              </View>
            ) : item.attachment_type === 'location' ? (
              <TouchableOpacity
                activeOpacity={0.7}
                onPress={() => {
                  if (item.latitude && item.longitude) {
                    const url = Platform.select({
                      ios: `http://maps.apple.com/?ll=${item.latitude},${item.longitude}`,
                      android: `geo:${item.latitude},${item.longitude}?q=${item.latitude},${item.longitude}`,
                    });
                    if (url) {
                      Linking.openURL(url).catch((err) =>
                        console.error('An error occurred', err),
                      );
                    }
                  }
                }}
                style={styles.locationContainer}
              >
                {item.local_url ? (
                  <>
                    <SmartImage
                      cdnUrl={item.cdn_url}
                      localUrl={item.local_url}
                      style={[
                        styles.previewImage,
                        { aspectRatio: item.aspect_ratio },
                      ]}
                      resizeMode='cover'
                    />
                  </>
                ) : (
                  <View style={styles.locationPlaceholder}>
                    <Icon type='geolocation' size={24} color='#34A8FB' />
                    <AppText style={styles.locationText}>
                      {item.latitude?.toFixed(4)}, {item.longitude?.toFixed(4)}
                    </AppText>
                  </View>
                )}
              </TouchableOpacity>
            ) : (
              <View style={styles.fileBlock}>
                <View style={styles.fileInfo}>
                  <AppText style={styles.fileName} numberOfLines={1}>
                    {getDisplayFileName(item)}
                  </AppText>
                  <View style={styles.fileMetaRow}>
                    <Icon type='file' size={12} color='#34A8FB' />
                    <AppText style={styles.fileType}>{getDisplayFileType(item)}</AppText>
                  </View>
                </View>
              </View>
            )}
            <TouchableOpacity
              style={styles.removeButton}
              onPress={() => removeAttachment(item.id)}
            >
              <AppText style={styles.removeButtonText}>×</AppText>
            </TouchableOpacity>
          </View>
        ))}
      </RNScrollView>
      {!hideSeparator && <View style={styles.separator} />}
    </>
  );
};

const styles = StyleSheet.create({
  attachmentScroll: {
    marginBottom: spacing.md,
  },
  attachmentList: {
    paddingRight: 16,
    height: 82,
  },
  attachmentItem: {
    height: 82,
    marginRight: 16,
    position: 'relative',
    borderRadius: 10,
    overflow: 'hidden',
  },
  imageAttachmentItem: {
    backgroundColor: '#f5f5f5',
    justifyContent: 'center',
    alignItems: 'center',
  },
  imagePreviewFrame: {
    width: '100%',
    height: '100%',
    justifyContent: 'center',
    alignItems: 'center',
  },
  previewImage: {
    height: '100%',
    backgroundColor: '#f5f5f5',
  },
  previewImageContain: {
    width: '100%',
    height: '100%',
    backgroundColor: '#f5f5f5',
  },
  fileBlock: {
    width: 156,
    height: '100%',
    backgroundColor: '#F7F7F7',
    borderRadius: 10,
    paddingHorizontal: spacing.md,
    paddingVertical: 10,
    borderWidth: 1,
    borderColor: '#E8E8E8',
  },
  locationContainer: {
    height: '100%',
  },
  locationPlaceholder: {
    width: 120,
    height: '100%',
    backgroundColor: '#F0F8FF',
    borderRadius: 10,
    justifyContent: 'center',
    alignItems: 'center',
    borderWidth: 1,
    borderColor: '#34A8FB',
    paddingHorizontal: spacing.sm,
  },
  locationText: {
    ...getTypographyStyle('caption1'),
    color: '#34A8FB',
    marginTop: spacing.xs,
    textAlign: 'center',
  },
  fileInfo: {
    flex: 1,
    justifyContent: 'space-between',
  },
  fileName: {
    ...getTypographyStyle('body'),
    color: '#333',
    fontWeight: '500',
    textAlign: 'left',
  },
  fileMetaRow: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 6,
  },
  fileType: {
    ...getTypographyStyle('footnote'),
    color: '#999',
    textAlign: 'left',
  },
  removeButton: {
    position: 'absolute',
    top: 4,
    right: 4,
    width: attachmentPreview.removeButtonSize,
    height: attachmentPreview.removeButtonSize,
    borderRadius: attachmentPreview.removeButtonSize / 2,
    backgroundColor: 'rgba(0,0,0,0.5)',
    justifyContent: 'center',
    alignItems: 'center',
    zIndex: 10,
  },
  removeButtonText: {
    color: '#fff',
    fontSize: 18,
    fontWeight: 'bold',
  },
  separator: {
    height: 1,
    backgroundColor: '#EAEAEA',
    width: '100%',
    marginBottom: spacing.lg,
  },
});

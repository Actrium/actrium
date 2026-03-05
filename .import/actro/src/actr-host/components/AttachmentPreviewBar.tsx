import React from 'react';
import {
  View,
  Text,
  TouchableOpacity,
  StyleSheet,
  Linking,
  Platform,
  ScrollView as RNScrollView,
} from 'react-native';
import { IAttachmentItem } from '../types/attachment';
import { SmartImage } from './SmartImage';
import { Icon } from './Icon';

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
          <View key={item.id} style={styles.attachmentItem}>
            {item.attachment_type === 'image' ? (
              <SmartImage
                cdnUrl={item.cdn_url}
                localUrl={item.local_url}
                style={[
                  styles.previewImage,
                  { aspectRatio: item.aspect_ratio },
                ]}
                resizeMode='cover'
              />
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
                    <View style={styles.locationOverlay}>
                      <Icon type='geolocation' size={14} color='#fff' />
                    </View>
                  </>
                ) : (
                  <View style={styles.locationPlaceholder}>
                    <Icon type='geolocation' size={24} color='#34A8FB' />
                    <Text style={styles.locationText}>
                      {item.latitude?.toFixed(4)}, {item.longitude?.toFixed(4)}
                    </Text>
                  </View>
                )}
              </TouchableOpacity>
            ) : (
              <View style={styles.fileBlock}>
                <View style={styles.fileInfo}>
                  <Text style={styles.fileName} numberOfLines={1}>
                    {getDisplayFileName(item)}
                  </Text>
                  <View style={styles.fileMetaRow}>
                    <Icon type='file' size={12} color='#34A8FB' />
                    <Text style={styles.fileType}>{getDisplayFileType(item)}</Text>
                  </View>
                </View>
              </View>
            )}
            <TouchableOpacity
              style={styles.removeButton}
              onPress={() => removeAttachment(item.id)}
            >
              <Text style={styles.removeButtonText}>×</Text>
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
    marginBottom: 12,
  },
  attachmentList: {
    paddingRight: 16,
    height: 67,
  },
  attachmentItem: {
    height: 67,
    marginRight: 16,
    position: 'relative',
    borderRadius: 10,
    overflow: 'hidden',
  },
  previewImage: {
    height: '100%',
    backgroundColor: '#f5f5f5',
  },
  fileBlock: {
    width: 156,
    height: '100%',
    backgroundColor: '#F7F7F7',
    borderRadius: 10,
    paddingHorizontal: 12,
    paddingVertical: 10,
    borderWidth: 1,
    borderColor: '#E8E8E8',
  },
  locationOverlay: {
    position: 'absolute',
    bottom: 4,
    right: 4,
    backgroundColor: 'rgba(52, 168, 251, 0.8)',
    borderRadius: 6,
    padding: 4,
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
    paddingHorizontal: 8,
  },
  locationText: {
    fontSize: 11,
    color: '#34A8FB',
    marginTop: 6,
    textAlign: 'center',
  },
  fileInfo: {
    flex: 1,
    justifyContent: 'space-between',
  },
  fileName: {
    fontSize: 14,
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
    fontSize: 12,
    color: '#999',
    textAlign: 'left',
  },
  removeButton: {
    position: 'absolute',
    top: 6,
    right: 6,
    width: 24,
    height: 24,
    borderRadius: 12,
    backgroundColor: 'rgba(0,0,0,0.5)',
    justifyContent: 'center',
    alignItems: 'center',
    zIndex: 10,
  },
  removeButtonText: {
    color: '#fff',
    fontSize: 18,
    fontWeight: 'bold',
    top: -1,
  },
  separator: {
    height: 1,
    backgroundColor: '#EAEAEA',
    width: '100%',
    marginBottom: 16,
  },
});

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
            {item.attachment_type === 'image' ||
            item.attachment_type === 'location' ? (
              <TouchableOpacity
                activeOpacity={item.attachment_type === 'location' ? 0.7 : 1}
                onPress={() => {
                  if (
                    item.attachment_type === 'location' &&
                    item.latitude &&
                    item.longitude
                  ) {
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
              >
                <SmartImage
                  cdnUrl={item.cdn_url}
                  localUrl={item.local_url}
                  style={[
                    styles.previewImage,
                    { aspectRatio: item.aspect_ratio },
                  ]}
                  resizeMode='cover'
                />
                {item.attachment_type === 'location' && (
                  <View style={styles.locationOverlay}>
                    <Icon type='geolocation' size={14} color='#fff' />
                  </View>
                )}
              </TouchableOpacity>
            ) : (
              <View style={styles.fileBlock}>
                <Icon type='file' size={24} color='#34A8FB' />
                <View style={styles.fileInfo}>
                  <Text style={styles.fileName} numberOfLines={1}>
                    {item.name}
                  </Text>
                  <Text style={styles.fileType}>{item.type}</Text>
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
    height: 100,
  },
  attachmentItem: {
    height: 100,
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
    width: 300,
    height: '100%',
    backgroundColor: '#F7F7F7',
    borderRadius: 10,
    flexDirection: 'row',
    alignItems: 'center',
    paddingHorizontal: 16,
    gap: 12,
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
  fileInfo: {
    flex: 1,
  },
  fileName: {
    fontSize: 15,
    color: '#333',
    fontWeight: '500',
    marginBottom: 4,
  },
  fileType: {
    fontSize: 12,
    color: '#999',
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

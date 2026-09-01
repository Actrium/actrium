import React from 'react';
import { View, Text, StyleSheet, useColorScheme } from 'react-native';
import { AppText, Icon } from '../../../../components';
import { SmartImage } from '../../shared/SmartImage';
import { Supramark } from '@supramark/rn';
import type { SupramarkContentStyle } from '@supramark/rn';
import {
  getTypographyStyle,
  typography,
} from '../../../../theme/tokens';
import { useColors } from '../../../../theme/useColors';
import type { ColorTokens } from '../../../../theme/colors';
import type { IAttachmentItem } from '../../../types/attachment';
import {
  SUPRAMARK_CONTAINER_RENDERERS,
  SUPRAMARK_FULL_CONFIG,
} from '../../../libs/supramarkConfig';
import {
  formatVoiceDuration,
  parseVoicePayload,
} from '../../../utils/VoiceMessageStorage';

interface ChatShareBubbleProps {
  content?: string;
  voicePayload?: string;
  type: 'question' | 'answer';
  attachments?: string;
  onAttachmentImageSettled?: (attachmentKey: string) => void;
}

export default function ChatShareBubble({
  content,
  voicePayload,
  type,
  attachments,
  onAttachmentImageSettled,
}: ChatShareBubbleProps) {
  const colors = useColors();
  const colorScheme = useColorScheme();
  const styles = React.useMemo(() => makeStyles(colors), [colors]);
  const isQuestion = type === 'question';
  // Supramark 主题需要由分享视图显式传入，不能只依赖外层颜色 token。
  const supramarkTheme = colorScheme === 'dark' ? 'dark' : 'light';
  const parsedVoicePayload = React.useMemo(
    () => parseVoicePayload(voicePayload),
    [voicePayload],
  );

  const parsedAttachments = React.useMemo(() => {
    if (!attachments) return [];
    try {
      return JSON.parse(attachments) as IAttachmentItem[];
    } catch (error) {
      console.error('[ChatShareBubble] Failed to parse attachments:', error);
      return [];
    }
  }, [attachments]);

  const isAttachmentOnly = parsedAttachments.length > 0 && !content;
  const hasAttachments = parsedAttachments.length > 0;
  const messageTypography = typography.messageBody;
  // 分享气泡只向 Supramark 传递正文 token，结构样式由 Supramark 主题自己控制。
  const markdownContentStyle = React.useMemo(
    () => ({
      color: colors.textHeading,
      headingColor: colors.textHeading,
      fontSize: messageTypography.fontSize,
      lineHeight: messageTypography.lineHeight,
    }) satisfies SupramarkContentStyle,
    [colors.textHeading, messageTypography.fontSize, messageTypography.lineHeight],
  );

  const getDisplayFileName = React.useCallback((item: IAttachmentItem): string => {
    const legacyName = (item as any).name as string | undefined;
    const fileName = item.file_name || legacyName;
    if (fileName) return fileName;

    const rawName = item.local_url.split('/').pop() || '';
    try {
      return decodeURIComponent(rawName);
    } catch {
      return rawName;
    }
  }, []);

  const getDisplayFileType = React.useCallback(
    (item: IAttachmentItem): string => {
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
    },
    [getDisplayFileName],
  );

  return (
    <View style={isQuestion ? styles.questionWrapper : styles.answerWrapper}>
      <View
        style={[
          styles.bubbleOuter,
          hasAttachments
            ? styles.bubbleOuterWithAttachments
            : isQuestion
              ? styles.bubbleOuterCompactQuestion
              : styles.bubbleOuterCompactAnswer,
        ]}
      >
        <View
          style={[
            isAttachmentOnly
              ? styles.attachmentBareBubble
              : isQuestion
                ? styles.questionBubble
                : styles.answerBubble,
          ]}
        >
          {parsedAttachments.length > 0 && (
            <View
              style={[
                styles.attachmentsContainer,
                isQuestion && styles.attachmentsContainerQuestion,
                !content && { marginBottom: 0 },
              ]}
            >
              {parsedAttachments.map((item, index) => {
                const isLast = index === parsedAttachments.length - 1;
                const itemStyle = [
                  item.aspect_ratio ? { aspectRatio: item.aspect_ratio } : null,
                  !isLast && { marginRight: 8 },
                ];

                if (item.attachment_type === 'image') {
                  const attachmentKey = item.id || `image-${index}`;
                  return (
                    <SmartImage
                      key={`share-image-${attachmentKey}`}
                      cdnUrl={item.cdn_url}
                      localUrl={item.local_url}
                      style={[styles.attachmentImage, itemStyle]}
                      resizeMode='cover'
                      onLoadEnd={() => {
                        onAttachmentImageSettled?.(attachmentKey);
                      }}
                      onError={() => {
                        onAttachmentImageSettled?.(attachmentKey);
                      }}
                    />
                  );
                }

                if (item.attachment_type === 'file') {
                  return (
                    <View
                      key={`share-file-${index}`}
                      style={[
                        styles.attachmentFileBlock,
                        !isLast && { marginRight: 8 },
                      ]}
                    >
                      <View style={styles.attachmentFileInfo}>
                        <Text
                          style={styles.attachmentFileName}
                          numberOfLines={2}
                          ellipsizeMode='tail'
                        >
                          {getDisplayFileName(item)}
                        </Text>
                        <View style={styles.attachmentFileMetaRow}>
                          <Icon type='file' size={12} color='#34A8FB' />
                          <AppText style={styles.attachmentFileType}>
                            {getDisplayFileType(item)}
                          </AppText>
                        </View>
                      </View>
                    </View>
                  );
                }

                if (item.attachment_type === 'location') {
                  const attachmentKey = item.id || `location-${index}`;
                  return (
                    <View
                      key={`share-location-${attachmentKey}`}
                      style={[styles.locationWrapper, !isLast && { marginRight: 8 }]}
                    >
                      {item.local_url || item.cdn_url ? (
                        <SmartImage
                          cdnUrl={item.cdn_url}
                          localUrl={item.local_url}
                          style={[
                            styles.attachmentImage,
                            item.aspect_ratio ? { aspectRatio: item.aspect_ratio } : null,
                          ]}
                          resizeMode='cover'
                          onLoadEnd={() => {
                            onAttachmentImageSettled?.(attachmentKey);
                          }}
                          onError={() => {
                            onAttachmentImageSettled?.(attachmentKey);
                          }}
                        />
                      ) : (
                        <View style={styles.locationPlaceholder}>
                          <Icon type='geolocation' size={20} color='#34A8FB' />
                          <Text style={styles.locationPlaceholderText}>
                            {item.latitude?.toFixed(4)}, {item.longitude?.toFixed(4)}
                          </Text>
                        </View>
                      )}
                    </View>
                  );
                }

                return null;
              })}
            </View>
          )}

          {parsedVoicePayload ? (
            <View style={styles.voiceBubbleCard}>
              <View style={styles.voiceBubbleMain}>
                <Icon type='voice' size={18} color={colors.textMenuLabel} />
                <Text style={styles.voiceDurationText}>
                  {formatVoiceDuration(parsedVoicePayload.duration_ms)}
                </Text>
              </View>
              <AppText style={styles.voiceHintText}>语音消息</AppText>
            </View>
          ) : content ? (
            <View style={styles.markdownContainer}>
              <Supramark
                markdown={content}
                config={SUPRAMARK_FULL_CONFIG}
                containerRenderers={SUPRAMARK_CONTAINER_RENDERERS as any}
                // 分享卡片沿用聊天正文渲染配置，保证截图样式与对话页一致。
                contentStyle={markdownContentStyle}
                theme={supramarkTheme}
              />
            </View>
          ) : null}
        </View>
      </View>
    </View>
  );
}

const makeStyles = (colors: ColorTokens) =>
  StyleSheet.create({
    questionWrapper: {
      alignItems: 'flex-end',
      marginBottom: 12,
      width: '100%',
    },
    answerWrapper: {
      alignItems: 'flex-start',
      width: '100%',
    },
    bubbleOuter: {
      maxWidth: '100%',
    },
    bubbleOuterWithAttachments: {
      width: '100%',
      alignSelf: 'flex-end',
    },
    bubbleOuterCompactQuestion: {
      alignSelf: 'flex-end',
    },
    bubbleOuterCompactAnswer: {
      alignSelf: 'flex-start',
    },
    questionBubble: {
      backgroundColor: colors.bgBubbleQuestion,
      paddingHorizontal: 15,
      paddingVertical: 14,
      borderRadius: 20,
      minWidth: 44,
      alignSelf: 'flex-end',
      maxWidth: '100%',
    },
    answerBubble: {
      backgroundColor: colors.bgBubbleAnswer,
      paddingHorizontal: 15,
      paddingVertical: 14,
      borderRadius: 20,
      minWidth: 44,
      alignSelf: 'flex-start',
      maxWidth: '100%',
    },
    attachmentBareBubble: {
      paddingHorizontal: 0,
      paddingVertical: 0,
      backgroundColor: 'transparent',
      minWidth: 0,
    },
    markdownContainer: {
      flexShrink: 1,
      width: '100%',
    },
    attachmentsContainer: {
      marginBottom: 8,
      flexDirection: 'row',
      flexWrap: 'wrap',
      width: '100%',
    },
    attachmentsContainerQuestion: {
      justifyContent: 'flex-end',
    },
    attachmentImage: {
      height: 82,
      maxHeight: 82,
      borderRadius: 10,
      borderWidth: 1,
      borderColor: colors.borderFileBlock,
      backgroundColor: colors.bgFileBlock,
      width: 110,
    },
    attachmentFileBlock: {
      width: 156,
      maxHeight: 82,
      borderRadius: 10,
      borderWidth: 1,
      borderColor: colors.borderFileBlock,
      backgroundColor: colors.bgFileBlock,
      paddingHorizontal: 10,
      paddingVertical: 8,
    },
    attachmentFileInfo: {
      flexShrink: 1,
    },
    attachmentFileName: {
      fontSize: 15,
      color: colors.textPrimary,
      fontWeight: 'bold',
      textAlign: 'left',
    },
    attachmentFileMetaRow: {
      flexDirection: 'row',
      alignItems: 'center',
      marginTop: 16,
      gap: 6,
    },
    attachmentFileType: {
      ...getTypographyStyle('footnote'),
      color: colors.textTertiary,
      textAlign: 'left',
    },
    locationWrapper: {
      position: 'relative',
    },
    locationPlaceholder: {
      width: 100,
      height: 80,
      backgroundColor: colors.bgFileBlock,
      borderRadius: 8,
      justifyContent: 'center',
      alignItems: 'center',
      borderWidth: 1,
      borderColor: colors.borderFileBlock,
      paddingHorizontal: 6,
    },
    locationPlaceholderText: {
      fontSize: 9,
      color: colors.accentBlue,
      marginTop: 4,
      textAlign: 'center',
    },
    voiceBubbleCard: {
      minWidth: 120,
      paddingVertical: 10,
      paddingHorizontal: 12,
      borderRadius: 14,
      backgroundColor: 'rgba(255,255,255,0.18)',
      gap: 4,
    },
    voiceBubbleMain: {
      flexDirection: 'row',
      alignItems: 'center',
      gap: 8,
    },
    voiceDurationText: {
      fontSize: 16,
      fontWeight: '600',
      color: colors.textHeading,
    },
    voiceHintText: {
      ...getTypographyStyle('footnote'),
      color: colors.textSecondary,
    },
  });

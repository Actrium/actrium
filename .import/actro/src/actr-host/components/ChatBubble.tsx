import React from 'react';
import {
  View,
  Text,
  StyleSheet,
  ViewStyle,
  Image,
  ScrollView,
  Pressable,
  ActivityIndicator,
  Modal,
  TouchableWithoutFeedback,
  Keyboard,
} from 'react-native';
import { Icon } from './Icon';
import { useTranslation } from 'react-i18next';
import { SmartImage } from './SmartImage';

import { LongPressMenu } from './LongPressMenu';
import { WaveForm } from './WaveForm';
import { useVoiceStore } from '../store/useVoiceStore';
import {
  DropdownMenu,
  DropdownMenuItem,
  DropdownMenuText,
} from './DropdownMenu';
import { useMessageStore } from '../store/useMessageStore';

import { Animated, Easing } from 'react-native';
import { FeedbackFormModal } from './FeedbackFormModal';
import { IAttachmentItem } from '../types/attachment';
import { Supramark } from '@supramark/rn';
import {
  SUPRAMARK_CONTAINER_RENDERERS,
  SUPRAMARK_FULL_CONFIG,
} from '../libs/supramarkConfig';

interface ChatBubbleProps {
  content?: string;
  type: 'question' | 'answer';
  size?: 'small' | 'normal';
  numberOfLines?: number;
  containerStyle?: ViewStyle;
  attachments?: string;
  onEdit?: () => void;
  onCopy?: () => void;
  isLatest?: boolean;
  msgId?: string;
  tabId?: string;
  appId?: string;
  selectionKey?: string;
  isSelected?: boolean;
  onToggleSelection?: () => void;
}

export default function ChatBubble({
  content,
  type,
  size = 'normal',
  numberOfLines,
  containerStyle,
  attachments,
  onEdit,
  onCopy,
  isLatest = false,
  msgId,
  tabId,
  appId,
  selectionKey,
  isSelected = false,
  onToggleSelection,
}: ChatBubbleProps) {
  const { t } = useTranslation();
  const isQuestion = type === 'question';
  const isSmall = size === 'small';

  const { playingId, setPlayingId } = useVoiceStore();
  const deleteMessage = useMessageStore((s) => s.deleteMessage);
  const selectionMode = useMessageStore((s) => s.selectionMode);
  const enterSelectionMode = useMessageStore((s) => s.enterSelectionMode);

  const [readAloudStatus, setReadAloudStatus] = React.useState<
    'idle' | 'loading' | 'playing'
  >('idle');

  const [isCopied, setIsCopied] = React.useState(false);
  const [showFeedbackModal, setShowFeedbackModal] = React.useState(false);
  const [showMoreMenu, setShowMoreMenu] = React.useState(false);
  const [moreMenuPosition, setMoreMenuPosition] = React.useState<{
    top: number;
    left: number;
    width: number;
  } | null>(null);

  const spinValue = React.useRef(new Animated.Value(0)).current;

  React.useEffect(() => {
    if (readAloudStatus === 'loading') {
      Animated.loop(
        Animated.timing(spinValue, {
          toValue: 1,
          duration: 1000,
          easing: Easing.linear,
          useNativeDriver: true,
        }),
      ).start();
    } else {
      spinValue.setValue(0);
    }
  }, [readAloudStatus]);

  React.useEffect(() => {
    if (isCopied) {
      const timer = setTimeout(() => setIsCopied(false), 1000);
      return () => clearTimeout(timer);
    }
  }, [isCopied]);

  const spin = spinValue.interpolate({
    inputRange: [0, 1],
    outputRange: ['0deg', '360deg'],
  });

  const startPlayback = React.useCallback(() => {
    if (!msgId) return;
    console.log('[ChatBubble] startPlayback:', content);
    setPlayingId(appId ?? null, msgId);
    setReadAloudStatus('loading');

    // 模拟加载 1s -> 播放 3s
    const loadTimer = setTimeout(() => {
      setReadAloudStatus('playing');
      const playTimer = setTimeout(() => {
        setReadAloudStatus('idle');
        setPlayingId(null, null);
      }, 3000);
      timersRef.current.push(playTimer);
    }, 1000);
    timersRef.current.push(loadTimer);
  }, [content, msgId, setPlayingId, appId]);

  const stopPlayback = React.useCallback(() => {
    console.log('[ChatBubble] stopPlayback');
    setReadAloudStatus('idle');
    if (playingId === msgId) {
      setPlayingId(null, null);
    }
  }, [msgId, playingId, setPlayingId]);

  const timersRef = React.useRef<ReturnType<typeof setTimeout>[]>([]);

  const clearLocalTimers = () => {
    timersRef.current.forEach(clearTimeout);
    timersRef.current = [];
  };

  const handleReadAloud = React.useCallback(() => {
    if (!msgId) return;

    // 如果当前正在播放/加载此消息，则停止
    if (playingId === msgId) {
      stopPlayback();
      return;
    }

    startPlayback();
  }, [msgId, playingId, startPlayback, stopPlayback]);

  // 监听全局播放 ID 变化，实现互斥播放
  React.useEffect(() => {
    if (playingId !== msgId && readAloudStatus !== 'idle') {
      console.log(
        '[ChatBubble] Global playback changed, stopping current msg:',
        msgId,
      );
      setReadAloudStatus('idle');
      clearLocalTimers();
    }
  }, [playingId, msgId, readAloudStatus]);

  React.useEffect(() => {
    return () => clearLocalTimers();
  }, []);

  const menuOptions = React.useMemo(() => {
    const options = [];
    if (isQuestion && onEdit) {
      options.push({
        label: t('common.edit'),
        onPress: onEdit,
        icon: <Icon type='edit' size={16} />,
      });
    }
    if (onCopy) {
      options.push({
        label: t('common.copy'),
        onPress: onCopy,
        icon: <Icon type='copy' size={16} />,
      });
    }
    if (!isSmall && !isQuestion) {
      options.push({
        label: t('actr.chat.readAloud'),
        onPress: handleReadAloud,
        icon: <Icon type='voice' size={16} />,
      });
    }
    if (!isQuestion) {
      options.push({
        label: t('common.more'),
        onPress: (position: any) => {
          setMoreMenuPosition(position);
          setTimeout(() => setShowMoreMenu(true), 300);
        },
        icon: <Icon type='menu_more' size={16} />,
      });
    }

    // 添加删除选项
    if (msgId && appId) {
      options.push({
        label: t('common.delete'),
        onPress: () => {
          console.log(
            'Delete clicked from long press menu:',
            msgId,
            'appId:',
            appId,
          );
          if (msgId && tabId) {
            const defaultPart =
              attachments && !content ? 'attachments' : undefined;
            enterSelectionMode(msgId, tabId, defaultPart);
          }
        },
        icon: <Icon type='delete' size={16} color='#FF3B30' />,
        textColor: '#FF3B30', // 红色
      });
    }
    return options;
  }, [
    t,
    isQuestion,
    onEdit,
    onCopy,
    isSmall,
    handleReadAloud,
    msgId,
    tabId,
    enterSelectionMode,
    attachments,
    content,
  ]);

  const parsedAttachments = React.useMemo(() => {
    if (!attachments) return [];
    try {
      return JSON.parse(attachments);
    } catch (e) {
      console.error('Failed to parse attachments in ChatBubble', e);
      return [];
    }
  }, [attachments]);
  const isAttachmentOnly = parsedAttachments.length > 0 && !content;
  const hasAttachments = parsedAttachments.length > 0;
  const markdownTextStyle = React.useMemo(
    () => ({
      color: '#252525',
      fontSize: isSmall ? 11 : 16,
      lineHeight: isSmall ? 15 : 22,
    }),
    [isSmall],
  );
  const markdownParagraphStyle = React.useMemo(
    () => ({
      ...markdownTextStyle,
      marginBottom: 0,
    }),
    [markdownTextStyle],
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

  const bubbleContent = (
    <View
      style={[
        isAttachmentOnly
          ? styles.attachmentBareBubble
          : isQuestion
            ? styles.questionBubble
            : styles.answerBubble,
        !isAttachmentOnly &&
          isSmall &&
          (isQuestion ? styles.smallQuestionBubble : styles.smallAnswerBubble),
      ]}
    >
      {/* Render Attachments */}
      {parsedAttachments.length > 0 && (
        <View
          style={[styles.attachmentsContainer, !content && { marginBottom: 0 }]}
          pointerEvents='box-none'
        >
          <ScrollView
            horizontal
            showsHorizontalScrollIndicator={false}
            style={styles.attachmentsScrollView}
            contentContainerStyle={styles.attachmentsScrollContent}
            scrollEnabled
            scrollEventThrottle={16}
          >
            {parsedAttachments.map((item: IAttachmentItem, idx: number) => {
              const isLast = idx === parsedAttachments.length - 1;
              const itemStyle = [
                item.aspect_ratio ? { aspectRatio: item.aspect_ratio } : null,
                !isLast && { marginRight: 8 },
              ];

              if (item.attachment_type === 'image') {
                return (
                  <SmartImage
                    key={`img-${idx}`}
                    cdnUrl={item.cdn_url}
                    localUrl={item.local_url}
                    style={[styles.attachmentImage, itemStyle]}
                    resizeMode='cover'
                  />
                );
              } else if (item.attachment_type === 'file') {
                return (
                  <View
                    key={`file-${idx}`}
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
                        <Text style={styles.attachmentFileType}>
                          {getDisplayFileType(item)}
                        </Text>
                      </View>
                    </View>
                  </View>
                );
              } else if (item.attachment_type === 'location') {
                return (
                  <View
                    key={`loc-${idx}`}
                    style={[
                      { position: 'relative' },
                      !isLast && { marginRight: 8 },
                    ]}
                  >
                    {item.local_url || item.cdn_url ? (
                      <>
                        <SmartImage
                          cdnUrl={item.cdn_url}
                          localUrl={item.local_url}
                          style={[
                            styles.attachmentImage,
                            item.aspect_ratio
                              ? { aspectRatio: item.aspect_ratio }
                              : null,
                          ]}
                          resizeMode='cover'
                        />
                        <View style={styles.locationTag}>
                          <Icon type='geolocation' size={12} color='#fff' />
                          <Text style={styles.locationTagText}>{t('actr.attachment.locationTag')}</Text>
                        </View>
                      </>
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
          </ScrollView>
        </View>
      )}
      {content ? (
        numberOfLines ? (
          <Text
            style={[
              isQuestion ? styles.questionText : styles.answerText,
              isSmall && styles.smallText,
            ]}
            numberOfLines={numberOfLines}
          >
            {content}
          </Text>
        ) : (
          <View style={styles.markdownContainer}>
            <Supramark
              markdown={content}
              config={SUPRAMARK_FULL_CONFIG}
              containerRenderers={SUPRAMARK_CONTAINER_RENDERERS as any}
              styles={{
                paragraph: markdownParagraphStyle,
                // text: markdownTextStyle,
                listItemText: markdownTextStyle,
                inlineCode: markdownTextStyle,
                code: markdownTextStyle,
              }}
            />
          </View>
        )
      ) : null}

      {!isQuestion && isLatest && !isSmall && (
        <View style={styles.toolbarContainer}>
          <Pressable
            style={[
              styles.toolbarButton,
              selectionMode && styles.toolbarButtonDisabled,
            ]}
            onPress={() => {
              if (selectionMode) return;
              setTimeout(() => {
                onCopy?.();
                setIsCopied(true);
              }, 0);
            }}
            disabled={selectionMode}
            hitSlop={8}
          >
            <Icon type={isCopied ? 'more' : 'copy'} size={18} />
          </Pressable>
          <Pressable
            style={[
              styles.toolbarButton,
              readAloudStatus === 'playing' && styles.toolbarButtonPlaying,
              selectionMode && styles.toolbarButtonDisabled,
            ]}
            onPress={handleReadAloud}
            disabled={readAloudStatus !== 'idle' || selectionMode}
            hitSlop={8}
          >
            {readAloudStatus === 'loading' && (
              <Animated.View
                style={[
                  styles.loadingBorder,
                  { transform: [{ rotate: spin }] },
                ]}
              />
            )}
            {readAloudStatus === 'playing' ? (
              <WaveForm />
            ) : (
              <Icon type='voice' size={18} />
            )}
          </Pressable>
          <DropdownMenu
            trigger={
              <View style={styles.toolbarButton}>
                <Icon type='more' size={18} />
              </View>
            }
            disabled={selectionMode}
          >
            {(close) => (
              <>
                <DropdownMenuItem
                  onPress={() => {
                    close();
                    setShowFeedbackModal(true);
                  }}
                >
                  <Icon type='suggest' size={16} />
                  <DropdownMenuText>{t('actr.chat.feedback')}</DropdownMenuText>
                </DropdownMenuItem>
                <DropdownMenuItem
                  onPress={() => {
                    if (msgId && appId) {
                      enterSelectionMode(msgId, tabId);
                    }
                    close();
                  }}
                >
                  <Icon type='delete' size={16} color='#FF3B30' />
                  <DropdownMenuText color='#FF3B30'>{t('common.delete')}</DropdownMenuText>
                </DropdownMenuItem>
              </>
            )}
          </DropdownMenu>
        </View>
      )}
      <FeedbackFormModal
        visible={showFeedbackModal}
        onClose={() => setShowFeedbackModal(false)}
        tabId={tabId}
        appId={appId}
        messageId={msgId}
      />
      <Modal visible={showMoreMenu} transparent animationType='fade'>
        <TouchableWithoutFeedback
          onPress={() => {
            setShowMoreMenu(false);
            setMoreMenuPosition(null);
          }}
        >
          <View style={styles.moreMenuBackdrop}>
            <View
              style={[
                styles.moreMenuContainer,
                moreMenuPosition && {
                  top: moreMenuPosition.top,
                  left: moreMenuPosition.left,
                  width: moreMenuPosition.width,
                  position: 'absolute',
                },
              ]}
            >
              <Pressable
                style={({ pressed }) => [
                  styles.moreMenuItem,
                  pressed && styles.moreMenuItemPressed,
                ]}
                onPress={() => {
                  setShowMoreMenu(false);
                  setMoreMenuPosition(null);
                  setTimeout(() => setShowFeedbackModal(true), 100);
                }}
              >
                <Icon type='suggest' size={16} />
                <Text style={styles.moreMenuText}>{t('actr.chat.feedback')}</Text>
              </Pressable>
            </View>
          </View>
        </TouchableWithoutFeedback>
      </Modal>
    </View>
  );

  const handleBubblePress = () => {
    Keyboard.dismiss();
    if (selectionMode && onToggleSelection) {
      onToggleSelection();
    }
  };

  return (
    <View
      style={[
        isQuestion ? styles.questionWrapper : styles.answerWrapper,
        containerStyle,
      ]}
    >
      <View
        style={{
          width: hasAttachments ? '100%' : undefined,
          maxWidth: '100%',
          alignSelf: isQuestion ? 'flex-end' : 'flex-start',
        }}
      >
        <Pressable
          onPress={handleBubblePress}
          style={({ pressed }) => [
            selectionMode && pressed && styles.bubblePressed,
          ]}
        >
          {(() => {
            const shouldShowMenu =
              !selectionMode &&
              menuOptions.length > 0 &&
              (!isLatest || isQuestion) &&
              !hasAttachments;
            if (shouldShowMenu) {
              return (
                <LongPressMenu options={menuOptions}>
                  {bubbleContent}
                </LongPressMenu>
              );
            }
            return bubbleContent;
          })()}
        </Pressable>
      </View>
    </View>
  );
}

const styles = StyleSheet.create({
  toolbarContainer: {
    flexDirection: 'row',
    alignItems: 'center',
    marginTop: 12,
    paddingTop: 8,
    borderTopWidth: 1,
    borderTopColor: '#f1f5f9',
    gap: 10,
  },
  toolbarButton: {
    width: 36,
    height: 36,
    borderRadius: 18,
    backgroundColor: '#EAEAEA',
    alignItems: 'center',
    justifyContent: 'center',
  },
  toolbarButtonPlaying: {
    backgroundColor: '#e6f4ff',
  },
  loadingBorder: {
    position: 'absolute',
    width: 36,
    height: 36,
    borderRadius: 18,
    borderWidth: 2,
    borderColor: 'transparent',
    borderTopColor: '#34a8fb',
    borderRightColor: '#34a8fb',
  },
  // 基础包装容器（控制左右对齐）
  questionWrapper: {
    alignItems: 'flex-end',
    marginBottom: 12,
    width: '100%',
  },
  answerWrapper: {
    alignItems: 'flex-start',
    width: '100%',
  },
  // 气泡基础样式
  questionBubble: {
    backgroundColor: '#E6E9F6',
    paddingHorizontal: 15,
    paddingVertical: 14,
    borderRadius: 20,
    minWidth: 44, // 确保单个字时也有足够宽度
  },
  answerBubble: {
    backgroundColor: '#F9F9FB',
    paddingHorizontal: 15,
    paddingVertical: 14,
    borderRadius: 20,
    minWidth: 44,
  },
  // 文本基础样式
  questionText: {
    color: '#252525',
    fontSize: 16,
  },
  answerText: {
    color: '#252525',
    fontSize: 16,
  },
  // -------------------------
  // 小尺寸变体 (用于书签预览)
  // -------------------------
  smallQuestionBubble: {
    paddingHorizontal: 12,
    paddingVertical: 10,
    borderRadius: 12,
    maxWidth: '90%',
  },
  smallAnswerBubble: {
    paddingHorizontal: 10,
    paddingVertical: 8,
    borderRadius: 10,
    maxWidth: '85%',
  },
  attachmentBareBubble: {
    paddingHorizontal: 0,
    paddingVertical: 0,
    backgroundColor: 'transparent',
    minWidth: 0,
  },
  smallText: {
    fontSize: 11,
    lineHeight: 15,
  },
  markdownContainer: {
    flexShrink: 1,
    width: '100%',
  },
  attachmentsContainer: {
    marginBottom: 8,
    flexDirection: 'row',
    width: '100%',
  },
  attachmentsScrollView: {
    width: '100%',
  },
  attachmentsScrollContent: {
    flexGrow: 1,
    paddingRight: 0,
    justifyContent: 'flex-end',
  },
  attachmentImage: {
    height: 82,
    maxHeight: 82,
    borderRadius: 10,
    borderWidth: 1,
    borderColor: '#ACACAC',
    backgroundColor: '#E5E5EA',
  },
  attachmentFileBlock: {
    width: 156,
    maxHeight: 82,
    borderRadius: 10,
    borderWidth: 1,
    borderColor: '#ACACAC',
    backgroundColor: '#E5E5EA',
    paddingHorizontal: 10,
    paddingVertical: 8,
  },
  attachmentFileInfo: {
    flexShrink: 1,
  },
  attachmentFileName: {
    fontSize: 15,
    color: '#000000',
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
    fontSize: 12,
    color: '#999',
    textAlign: 'left',
  },
  locationTag: {
    position: 'absolute',
    bottom: 4,
    right: 12,
    backgroundColor: 'rgba(52, 168, 251, 0.8)',
    borderRadius: 4,
    paddingHorizontal: 4,
    paddingVertical: 2,
    flexDirection: 'row',
    alignItems: 'center',
    gap: 2,
  },
  locationTagText: {
    color: '#fff',
    fontSize: 10,
  },
  locationPlaceholder: {
    width: 100,
    height: 80,
    backgroundColor: '#F0F8FF',
    borderRadius: 8,
    justifyContent: 'center',
    alignItems: 'center',
    borderWidth: 1,
    borderColor: '#34A8FB',
    paddingHorizontal: 6,
  },
  locationPlaceholderText: {
    fontSize: 9,
    color: '#34A8FB',
    marginTop: 4,
    textAlign: 'center',
  },
  bubblePressed: {
    opacity: 0.7,
  },
  toolbarButtonDisabled: {
    opacity: 0.3,
  },
  moreMenuBackdrop: {
    flex: 1,
    backgroundColor: 'rgba(0,0,0,0.1)',
  },

  moreMenuContainer: {
    backgroundColor: '#FFFFFF',
    borderRadius: 34,
    padding: 10,
    width: 228,
    gap: 8,
    shadowColor: '#000',
    shadowOffset: { width: 0, height: 8 },
    shadowOpacity: 0.12,
    shadowRadius: 16,
    elevation: 8,
  },
  moreMenuItem: {
    flexDirection: 'row',
    alignItems: 'center',
    paddingVertical: 12,
    paddingHorizontal: 16,
    borderRadius: 34,
    gap: 12,
  },
  moreMenuItemPressed: {
    backgroundColor: '#F2F2F7',
  },
  moreMenuText: {
    fontSize: 17,
    color: '#333333',
    fontWeight: '500',
  },
});

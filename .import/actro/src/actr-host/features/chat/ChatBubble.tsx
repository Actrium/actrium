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
  Alert,
} from 'react-native';
import { Icon } from '../../../components';
import { useTranslation } from 'react-i18next';
import { SmartImage } from '../shared/SmartImage';
import { AppText } from '../../../components';
import {
  chatBubbleToolbar,
  getTypographyStyle,
  longPressMenu,
  typography,
} from '../../../theme/tokens';
import { useColors } from '../../../theme/useColors';
import type { ColorTokens } from '../../../theme/colors';

import { LongPressMenu } from '../shared/LongPressMenu';
import { useVoicePlaybackStore } from '../../store/useVoicePlaybackStore';
import {
  DropdownMenu,
  DropdownMenuItem,
  DropdownMenuText,
} from '../shared/DropdownMenu';
import { useMessageStore, type SelectionMode } from '../../store/useMessageStore';
import { getHostShadows } from '../../styles/shadows';
import { triggerHaptic } from '../../utils/haptics';

import { FeedbackFormModal } from './FeedbackFormModal';
import { IAttachmentItem } from '../../types/attachment';
import { Supramark } from '@supramark/rn';
import type { SupramarkStyles } from '@supramark/rn';
import {
  SUPRAMARK_CONTAINER_RENDERERS,
  SUPRAMARK_FULL_CONFIG,
} from '../../libs/supramarkConfig';
import {
  formatVoiceDuration,
  parseVoicePayload,
  playVoiceMessageFile,
  stopVoiceMessagePlayback,
} from '../../utils/VoiceMessageStorage';
import {
  ReadAloudActionButton,
  useReadAloudAction,
} from './ReadAloudActionButton';

const SHOW_AI_READ_ALOUD_ENTRY = false;

interface ChatBubbleProps {
  content?: string;
  voicePayload?: string;
  type: 'question' | 'answer';
  isLoading?: boolean;
  progress?: number;
  numberOfLines?: number;
  containerStyle?: ViewStyle;
  attachments?: string;
  onEdit?: () => void;
  onCopy?: () => void;
  onFeedback?: () => void;
  isLatest?: boolean;
  msgId?: string;
  tabId?: string;
  appId?: string;
  selectionMode?: SelectionMode;
  selectionKey?: string;
  isSelected?: boolean;
  onToggleSelection?: () => void;
}

export default function ChatBubble({
  content,
  voicePayload,
  type,
  isLoading = false,
  progress,
  numberOfLines,
  containerStyle,
  attachments,
  onEdit,
  onCopy,
  onFeedback,
  isLatest = false,
  msgId,
  tabId,
  appId,
  selectionMode = 'none',
  selectionKey,
  isSelected = false,
  onToggleSelection,
}: ChatBubbleProps) {
  const { t } = useTranslation();
  const colors = useColors();
  const styles = React.useMemo(() => makeStyles(colors), [colors]);
  const isQuestion = type === 'question';
  const shouldShowLoadingBubble =
    !isQuestion && isLoading && !content && !voicePayload;
  const shouldShowLatestAnswerToolbar =
    !isQuestion && isLatest && !shouldShowLoadingBubble && progress === 2;
  const parsedVoicePayload = React.useMemo(
    () => parseVoicePayload(voicePayload),
    [voicePayload],
  );
  const isVoiceQuestion = isQuestion && !!parsedVoicePayload;
  const isVoiceAnswer = !isQuestion && !!parsedVoicePayload;
  const isVoiceBubble = !!parsedVoicePayload;
  const bubbleFrameStyle = isQuestion
    ? styles.questionBubbleFrame
    : styles.answerBubbleFrame;

  const { activePlayback, setActivePlayback, clearPlayback } =
    useVoicePlaybackStore();
  const enterSelectionMode = useMessageStore((s) => s.enterSelectionMode);

  const [isCopied, setIsCopied] = React.useState(false);
  const [showMoreMenu, setShowMoreMenu] = React.useState(false);
  const [moreMenuPosition, setMoreMenuPosition] = React.useState<{
    top: number;
    left: number;
    width: number;
  } | null>(null);

  const bubblePart = type;
  const isCurrentPlaybackTarget =
    activePlayback.messageId === msgId &&
    activePlayback.part === bubblePart &&
    activePlayback.appId === (appId ?? null);
  const isVoiceBubblePlaying =
    isCurrentPlaybackTarget && activePlayback.mode === 'voice_message';
  const beginPlayback = React.useCallback(
    (status: 'loading' | 'playing' = 'playing') => {
      if (!msgId) return;
      // 中文注释：切换全局播放目标前先停掉本地语音文件播放，保证新的播放态和实际音频输出一致。
      stopVoiceMessagePlayback();
      setActivePlayback({
        appId: appId ?? null,
        messageId: msgId,
        part: bubblePart,
        mode: 'voice_message',
        status,
      });
    },
    [appId, bubblePart, msgId, setActivePlayback],
  );

  const clearPlaybackIfCurrent = React.useCallback(() => {
    const playback = useVoicePlaybackStore.getState().activePlayback;
    const isSamePlayback =
      playback.messageId === msgId &&
      playback.part === bubblePart &&
      playback.appId === (appId ?? null);
    if (isSamePlayback) {
      clearPlayback();
    }
  }, [appId, bubblePart, clearPlayback, msgId]);

  React.useEffect(() => {
    if (isCopied) {
      const timer = setTimeout(() => setIsCopied(false), 1000);
      return () => clearTimeout(timer);
    }
  }, [isCopied]);

  const readAloudAction = useReadAloudAction({
    appId,
    msgId,
    part: bubblePart,
  });

  const handleVoiceBubblePress = React.useCallback(async () => {
    if (!msgId || !parsedVoicePayload) return;

    if (isVoiceBubblePlaying) {
      stopVoiceMessagePlayback();
      clearPlaybackIfCurrent();
      return;
    }

    try {
      beginPlayback();
      await playVoiceMessageFile(voicePayload);
    } catch (error) {
      Alert.alert(
        t('common.hint'),
        error instanceof Error ? error.message : t('common.error'),
      );
    } finally {
      clearPlaybackIfCurrent();
    }
  }, [
    beginPlayback,
    clearPlaybackIfCurrent,
    isVoiceBubblePlaying,
    msgId,
    parsedVoicePayload,
    t,
    voicePayload,
  ]);

  const menuOptions = React.useMemo(() => {
    const options = [];
    if (isQuestion && onEdit) {
      options.push({
        label: t('common.edit'),
        onPress: onEdit,
        icon: <Icon type="edit" size={16} />,
      });
    }
    if (onCopy) {
      options.push({
        label: t('common.copy'),
        onPress: onCopy,
        icon: <Icon type="copy" size={16} />,
      });
    }
    // 中文注释：分享入口进入分享选择模式，用户多选后点击确认才生成分享图。
    if (msgId && tabId) {
      options.push({
        label: t('actr.chat.share'),
        onPress: () => {
          enterSelectionMode(msgId, tabId, undefined, 'share');
        },
        icon: <Icon type="share" size={16} />,
      });
    }
    // 中文注释：AI 朗读能力保留在独立组件/动作里，当前仅关闭入口显示，不影响后续单独恢复。
    if (SHOW_AI_READ_ALOUD_ENTRY && !isQuestion) {
      options.push({
        label: t('actr.chat.readAloud'),
        onPress: readAloudAction.handlePress,
        icon: <Icon type="voice" size={16} />,
      });
    }
    if (!isQuestion) {
      options.push({
        label: t('common.more'),
        onPress: (position: any) => {
          setMoreMenuPosition(position);
          setTimeout(() => {
            triggerHaptic('impactHeavy');
            setShowMoreMenu(true);
          }, 300);
        },
        icon: <Icon type="more" size={16} />,
      });
    }

    // 中文注释：删除入口进入删除选择模式，显式传入 'delete'。
    if (msgId && tabId) {
      const defaultPart =
        attachments && !content ? 'attachments' : undefined;
      options.push({
        label: t('common.delete'),
        onPress: () => {
          enterSelectionMode(msgId, tabId, defaultPart, 'delete');
        },
        icon: <Icon type="delete" size={16} color={colors.danger} />,
        textColor: colors.danger,
      });
    }
    return options;
  }, [
    t,
    isQuestion,
    onEdit,
    onCopy,
    readAloudAction.handlePress,
    msgId,
    tabId,
    enterSelectionMode,
    attachments,
    content,
    colors,
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
  const messageTypography = typography.messageBody;
  const markdownTextStyle = React.useMemo(
    () => ({
      color: colors.textHeading,
      fontSize: messageTypography.fontSize,
      lineHeight: messageTypography.lineHeight,
    }),
    [
      colors.textHeading,
      messageTypography.fontSize,
      messageTypography.lineHeight,
    ],
  );
  const markdownParagraphStyle = React.useMemo(
    () => ({
      ...markdownTextStyle,
      marginBottom: 0,
    }),
    [markdownTextStyle],
  );
  const markdownHeadingStyles = React.useMemo(
    () => ({
      h1: {
        ...getTypographyStyle('title2'),
        color: colors.textHeading,
        marginBottom: 12,
      },
      h2: {
        ...getTypographyStyle('title3'),
        color: colors.textHeading,
        marginBottom: 10,
      },
      h3: {
        ...getTypographyStyle('bodyInput'),
        color: colors.textHeading,
        fontWeight: '600',
        marginBottom: 8,
      },
      h4: {
        ...getTypographyStyle('bodyInput'),
        color: colors.textHeading,
        fontWeight: '600',
        marginBottom: 6,
      },
      h5: {
        ...getTypographyStyle('subhead'),
        color: colors.textHeading,
        fontWeight: '600',
        marginBottom: 4,
      },
      h6: {
        ...getTypographyStyle('footnote'),
        color: colors.textHeading,
        fontWeight: '600',
        marginBottom: 4,
      },
    }),
    [colors.textHeading],
  );
  const markdownSupramarkStyles = React.useMemo(
    () =>
      ({
        paragraph: markdownParagraphStyle,
        ...markdownHeadingStyles,
        listItemText: markdownTextStyle,
        inlineCode: markdownTextStyle,
        code: markdownTextStyle,
      }) as unknown as SupramarkStyles,
    [markdownHeadingStyles, markdownParagraphStyle, markdownTextStyle],
  );

  const getDisplayFileName = React.useCallback(
    (item: IAttachmentItem): string => {
      const legacyName = (item as any).name as string | undefined;
      const fileName = item.file_name || legacyName;
      if (fileName) return fileName;

      const rawName = item.local_url.split('/').pop() || '';
      try {
        return decodeURIComponent(rawName);
      } catch {
        return rawName;
      }
    },
    [],
  );

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
      ]}
    >
      {/* Render Attachments */}
      {parsedAttachments.length > 0 && (
        <View
          style={[styles.attachmentsContainer, !content && { marginBottom: 0 }]}
          pointerEvents="box-none"
        >
          <ScrollView
            horizontal
            showsHorizontalScrollIndicator={false}
            style={styles.attachmentsScrollView}
            contentContainerStyle={styles.attachmentsScrollContent}
            scrollEnabled
            scrollEventThrottle={16}
            // 中文注释：附件区域需要保留横向滚动能力，改为在自身触摸/拖动开始时收键盘，避免外层 Pressable 抢手势。
            onTouchStart={Keyboard.dismiss}
            onScrollBeginDrag={Keyboard.dismiss}
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
                    resizeMode="cover"
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
                        ellipsizeMode="tail"
                      >
                        {getDisplayFileName(item)}
                      </Text>
                      <View style={styles.attachmentFileMetaRow}>
                        <Icon type="file" size={12} color="#34A8FB" />
                        <AppText style={styles.attachmentFileType}>
                          {getDisplayFileType(item)}
                        </AppText>
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
                          resizeMode="cover"
                        />
                      </>
                    ) : (
                      <View style={styles.locationPlaceholder}>
                        <Icon type="geolocation" size={20} color="#34A8FB" />
                        <Text style={styles.locationPlaceholderText}>
                          {item.latitude?.toFixed(4)},{' '}
                          {item.longitude?.toFixed(4)}
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
      {shouldShowLoadingBubble ? (
        // 中文注释：等待回答中的占位态也走 answer 气泡本体，保证 icon 和文案始终落在同一气泡结构里。
        <View style={styles.loadingBubbleContent}>
          <ActivityIndicator size="small" color={colors.textHeading} />
          <AppText style={styles.loadingText}>
            {t('actr.chat.thinking')}
          </AppText>
        </View>
      ) : isVoiceBubble && parsedVoicePayload ? (
        <View
          style={[
            styles.voiceBubbleCard,
            isVoiceBubblePlaying && styles.voiceBubbleCardPlaying,
          ]}
        >
          <View style={styles.voiceBubbleMain}>
            <Icon type="voice" size={18} color={colors.textMenuLabel} />
            <Text style={styles.voiceDurationText}>
              {formatVoiceDuration(parsedVoicePayload.duration_ms)}
            </Text>
          </View>
          <AppText style={styles.voiceHintText}>
            {isVoiceBubblePlaying ? '播放中' : '点击播放语音'}
          </AppText>
        </View>
      ) : content ? (
        numberOfLines ? (
          <AppText
            variant="messageBody"
            style={[isQuestion ? styles.questionText : styles.answerText]}
            numberOfLines={numberOfLines}
          >
            {content}
          </AppText>
        ) : (
          <View style={styles.markdownContainer}>
            <Supramark
              markdown={content}
              config={SUPRAMARK_FULL_CONFIG}
              containerRenderers={SUPRAMARK_CONTAINER_RENDERERS as any}
              // 中文注释：宿主与 supermark 工作区各自持有一份 react-native 类型定义，
              // 这里做局部类型桥接，避免样式对象在 TS 层被误判为不兼容。
              styles={markdownSupramarkStyles}
            />
          </View>
        )
      ) : null}

      {!isQuestion && shouldShowLatestAnswerToolbar && (
        <View style={styles.toolbarContainer}>
          <Pressable
            style={[
              styles.toolbarButton,
              selectionMode !== 'none' && styles.toolbarButtonDisabled,
            ]}
            onPress={() => {
              if (selectionMode !== 'none') return;
              setTimeout(() => {
                onCopy?.();
                setIsCopied(true);
              }, 0);
            }}
            disabled={selectionMode !== 'none'}
            hitSlop={8}
          >
            <Icon
              type={isCopied ? 'success' : 'copy'}
              size={chatBubbleToolbar.iconSize}
            />
          </Pressable>
          {SHOW_AI_READ_ALOUD_ENTRY ? (
            <ReadAloudActionButton
              appId={appId}
              msgId={msgId}
              part={bubblePart}
              selectionMode={selectionMode}
            />
          ) : null}
          <DropdownMenu
            width={longPressMenu.width}
            estimatedWidth={longPressMenu.width}
            anchorMode="end"
            trigger={
              <View style={styles.toolbarButton}>
                <Icon type="more" size={chatBubbleToolbar.iconSize} />
              </View>
            }
            disabled={selectionMode !== 'none'}
          >
            {(close) => (
              <>
                {/* 中文注释：工具栏下拉菜单中的分享入口，进入分享选择模式。
                    使用 close(callback) 将 enterSelectionMode 推迟到菜单关闭动画完成后执行，
                    避免与 DropdownMenu 内部 Reanimated 动画产生 useInsertionEffect 冲突。 */}
                {msgId && tabId ? (
                  <DropdownMenuItem
                    onPress={() => {
                      close(() => {
                        enterSelectionMode(msgId, tabId, undefined, 'share');
                      });
                    }}
                  >
                    <Icon type="share" size={16} />
                    <DropdownMenuText>{t('actr.chat.share')}</DropdownMenuText>
                  </DropdownMenuItem>
                ) : null}
                <DropdownMenuItem
                  onPress={() => {
                    close(() => {
                      onFeedback?.();
                    });
                  }}
                >
                  <Icon type="suggest" size={16} />
                  <DropdownMenuText>{t('actr.chat.feedback')}</DropdownMenuText>
                </DropdownMenuItem>
                {/* 中文注释：工具栏下拉菜单中的删除入口，同样使用 close(callback) 避免时序冲突。 */}
                <DropdownMenuItem
                  onPress={() => {
                    if (msgId && tabId) {
                      close(() => {
                        enterSelectionMode(msgId, tabId, undefined, 'delete');
                      });
                    }
                  }}
                >
                  <Icon type="delete" size={16} color={colors.danger} />
                  <DropdownMenuText color={colors.danger}>
                    {t('common.delete')}
                  </DropdownMenuText>
                </DropdownMenuItem>
              </>
            )}
          </DropdownMenu>
        </View>
      )}
      <Modal visible={showMoreMenu} transparent animationType="fade">
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
                  setTimeout(() => onFeedback?.(), 100);
                }}
              >
                <Icon type="suggest" size={16} />
                <AppText style={styles.moreMenuText}>
                  {t('actr.chat.feedback')}
                </AppText>
              </Pressable>
            </View>
          </View>
        </TouchableWithoutFeedback>
      </Modal>
    </View>
  );

  const handleBubblePress = () => {
    Keyboard.dismiss();
    if (selectionMode !== 'none' && onToggleSelection) {
      onToggleSelection();
      return;
    }

    // 中文注释：语音气泡的点击与长按统一走外层容器，避免内层 Pressable 抢占长按菜单手势。
    if (isVoiceBubble) {
      void handleVoiceBubblePress();
    }
  };

  return (
    <View
      style={[
        isQuestion ? styles.questionWrapper : styles.answerWrapper,
        containerStyle,
      ]}
    >
      <View style={bubbleFrameStyle}>
        {hasAttachments ? (
          // 中文注释：附件气泡不再包外层 Pressable，避免与内部横向 ScrollView 竞争 responder。
          <View>{bubbleContent}</View>
        ) : (
          <Pressable
            onPress={handleBubblePress}
            style={({ pressed }) => [
              selectionMode !== 'none' && pressed && styles.bubblePressed,
            ]}
          >
            {(() => {
              const shouldShowMenu =
                selectionMode === 'none' && menuOptions.length > 0 && !hasAttachments;
              if (shouldShowMenu) {
                return (
                  <LongPressMenu
                    options={menuOptions}
                    onPress={isVoiceBubble ? handleBubblePress : undefined}
                  >
                    {bubbleContent}
                  </LongPressMenu>
                );
              }
              return bubbleContent;
            })()}
          </Pressable>
        )}
      </View>
    </View>
  );
}

const makeStyles = (colors: ColorTokens) => {
  const hostShadows = getHostShadows(colors);

  return StyleSheet.create({
    toolbarContainer: {
      flexDirection: 'row',
      alignItems: 'center',
      marginTop: 12,
      paddingTop: 8,
      borderTopWidth: 1,
      borderTopColor: colors.borderToolbarTop,
      gap: 10,
    },
    toolbarButton: {
      width: chatBubbleToolbar.buttonSize,
      height: chatBubbleToolbar.buttonSize,
      borderRadius: chatBubbleToolbar.buttonSize / 2,
      backgroundColor: colors.bgToolbarButton,
      alignItems: 'center',
      justifyContent: 'center',
    },
    questionWrapper: {
      alignItems: 'flex-end',
      marginBottom: 12,
      width: '100%',
    },
    answerWrapper: {
      alignItems: 'flex-start',
      width: '100%',
    },
    questionBubbleFrame: {
      maxWidth: 316,
      alignSelf: 'flex-end',
    },
    answerBubbleFrame: {
      maxWidth: '100%',
      alignSelf: 'flex-start',
    },
    questionBubble: {
      backgroundColor: colors.bgBubbleQuestion,
      paddingHorizontal: 15,
      paddingVertical: 14,
      borderRadius: 20,
      minWidth: 44,
    },
    answerBubble: {
      backgroundColor: colors.bgBubbleAnswer,
      paddingHorizontal: 15,
      paddingVertical: 14,
      borderRadius: 20,
      minWidth: 44,
    },
    loadingBubbleContent: {
      flexDirection: 'row',
      alignItems: 'center',
      minHeight: 20,
    },
    loadingText: {
      marginLeft: 8,
      color: colors.textHeading,
      ...getTypographyStyle('messageBody'),
    },
    voiceBubbleCard: {
      minWidth: 120,
      paddingVertical: 10,
      paddingHorizontal: 12,
      borderRadius: 14,
      backgroundColor: 'rgba(255,255,255,0.18)',
      gap: 4,
    },
    voiceBubbleCardPlaying: {
      opacity: 0.85,
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
    questionText: {
      color: colors.textHeading,
      ...getTypographyStyle('messageBody'),
    },
    answerText: {
      color: colors.textHeading,
      ...getTypographyStyle('messageBody'),
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
      borderColor: colors.borderFileBlock,
      backgroundColor: colors.bgFileBlock,
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
      backgroundColor: colors.bgSurface,
      borderRadius: 34,
      padding: 10,
      width: 228,
      gap: 8,
      ...hostShadows.menuSurface,
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
      backgroundColor: colors.bgMenuItemPressed,
    },
    moreMenuText: {
      ...getTypographyStyle('title3'),
      color: colors.textMenuLabel,
      fontWeight: '500',
    },
  });
};

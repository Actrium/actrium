import React from 'react';
import {
  View,
  Text,
  StyleSheet,
  ViewStyle,
  ScrollView,
  Pressable,
  ActivityIndicator,
  Modal,
  TouchableWithoutFeedback,
  Keyboard,
  type TextStyle,
} from 'react-native';
import Animated from 'react-native-reanimated';
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
import {
  DropdownMenu,
  DropdownMenuItem,
  DropdownMenuText,
} from '../shared/DropdownMenu';
import { type SelectionMode } from '../../store/useMessageStore';
import { isTablet } from '../../../utils/device';
import { getHostShadows } from '../../styles/shadows';

import { IAttachmentItem } from '../../types/attachment';
import { Typewriter } from './Typewriter';
import {
  formatVoiceDuration,
  parseVoicePayload,
} from '../../utils/VoiceMessageStorage';
import {
  ReadAloudActionButton,
} from './ReadAloudActionButton';
import { useChatBubbleEvents } from './hooks/useChatBubbleEvents';
import { triggerHaptic } from '../../utils/haptics';

interface ChatBubbleProps {
  content?: string;
  voicePayload?: string;
  type: 'question' | 'answer';
  isLoading?: boolean;
  progress?: number;
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
  onToggleSelection?: () => void;
}

interface ChatBubbleBodyProps {
  content?: string;
  isAttachmentOnly: boolean;
  isQuestion: boolean;
  isVoiceBubble: boolean;
  isVoiceBubblePlaying: boolean;
  parsedAttachments: IAttachmentItem[];
  parsedVoicePayload: ReturnType<typeof parseVoicePayload>;
  shouldShowLoadingBubble: boolean;
  shouldShowLatestAnswerToolbar: boolean;
  selectionMode: SelectionMode;
  isCopied: boolean;
  styles: ReturnType<typeof makeStyles>;
  colors: ColorTokens;
  getDisplayFileName: (item: IAttachmentItem) => string;
  getDisplayFileType: (item: IAttachmentItem) => string;
  markdownContentStyle: TextStyle;
  typewriterEnabled: boolean;
  onTypewriterComplete?: () => void;
  t: ReturnType<typeof useTranslation>['t'];
  onCopy?: () => void;
  onFeedback?: () => void;
  onEnterShareSelection?: () => void;
  onEnterDeleteSelection?: () => void;
  onReadAloudPress?: () => void;
  readAloudStatus: React.ComponentProps<typeof ReadAloudActionButton>['status'];
  shouldShowReadAloudEntry: boolean;
  onToolbarCopy?: () => void;
}

const ChatBubbleBody = React.memo(function ChatBubbleBody({
  content,
  isAttachmentOnly,
  isQuestion,
  isVoiceBubble,
  isVoiceBubblePlaying,
  parsedAttachments,
  parsedVoicePayload,
  shouldShowLoadingBubble,
  shouldShowLatestAnswerToolbar,
  selectionMode,
  isCopied,
  styles,
  colors,
  getDisplayFileName,
  getDisplayFileType,
  markdownContentStyle,
  typewriterEnabled,
  onTypewriterComplete,
  t,
  onCopy,
  onFeedback,
  onEnterShareSelection,
  onEnterDeleteSelection,
  onReadAloudPress,
  readAloudStatus,
  shouldShowReadAloudEntry,
  onToolbarCopy,
}: ChatBubbleBodyProps) {
  return (
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
            // Annex regions need to retain a horizontal rolling capacity instead of touching themselves/Drag to start keyboarding, avoid outer layers Pressable Grab the gesture.
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
              }
              if (item.attachment_type === 'file') {
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
              }
              if (item.attachment_type === 'location') {
                return (
                  <View
                    key={`loc-${idx}`}
                    style={[
                      { position: 'relative' },
                      !isLast && { marginRight: 8 },
                    ]}
                  >
                    {item.local_url || item.cdn_url ? (
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
        // And the position in the waiting for answers. answer The bubbles, I promise. icon The case was always in the same bubble structure.
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
            {isVoiceBubblePlaying ? 'Playing' : 'Click to play a voice'}
          </AppText>
        </View>
      ) : content ? (
        <View style={styles.markdownContainer}>
          {isQuestion ? (
            <AppText style={markdownContentStyle}>{content}</AppText>
          ) : (
            <Typewriter
              content={content}
              contentStyle={markdownContentStyle}
              // typewriterEnabled One-time determination at mount:
              // Fluent Message → true♪ Pushing word for word till nature chases ♪ content;
              // Old message → false, the full text is displayed directly.
              enabled={typewriterEnabled}
              charsPerTick={2}
              tickIntervalMs={30}
              lineBreakDelayMs={100}
              onComplete={onTypewriterComplete}
            />
          )}
        </View>
      ) : null}
      {!isQuestion && shouldShowLatestAnswerToolbar && (
        <View style={styles.toolbarContainer}>
          <Pressable
            style={[
              styles.toolbarButton,
              selectionMode !== 'none' && styles.toolbarButtonDisabled,
            ]}
            onPress={() => {
              onToolbarCopy?.();
            }}
            disabled={selectionMode !== 'none'}
            hitSlop={8}
          >
            <Icon
              type={isCopied ? 'success' : 'copy'}
              size={chatBubbleToolbar.iconSize}
            />
          </Pressable>
          {shouldShowReadAloudEntry ? (
            <ReadAloudActionButton
              status={readAloudStatus}
              selectionMode={selectionMode}
              onPress={() => {
                void onReadAloudPress?.();
              }}
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
                {/* Chinese Comment: The sharing entry in the menu is drawn down in the toolbar to enter the sharing selection mode.
                    Use close(callback) Will enterSelectionMode The project is being implemented until the menu closes the animation is completed.
                    Avoid with DropdownMenu Internal Reanimated Animation useInsertionEffect Conflict. */}
                {onEnterShareSelection ? (
                  <DropdownMenuItem
                    onPress={() => {
                      close(() => {
                        onEnterShareSelection();
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
                {/* Chinese Comment: Delete Entry in the Pull Menu under Toolbar, same as close(callback) Avoiding time-series conflicts. */}
                {onEnterDeleteSelection ? (
                  <DropdownMenuItem
                    onPress={() => {
                      close(() => {
                        onEnterDeleteSelection();
                      });
                    }}
                  >
                    <Icon type="delete" size={16} color={colors.danger} />
                    <DropdownMenuText color={colors.danger}>
                      {t('common.delete')}
                    </DropdownMenuText>
                  </DropdownMenuItem>
                ) : null}
              </>
            )}
          </DropdownMenu>
        </View>
      )}
    </View>
  );
});

export default function ChatBubble({
  content,
  voicePayload,
  type,
  isLoading = false,
  progress,
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
  onToggleSelection,
}: ChatBubbleProps) {
  const { t } = useTranslation();
  const colors = useColors();
  const styles = React.useMemo(() => makeStyles(colors), [colors]);
  const isQuestion = type === 'question';
  // Typewriter enabled One-time determination on mount, and no change thereafter.
  // Old Messageprogress Already 2)→ enabled=false, directly displays the full text;
  // Fluent Message (Flow Message)progress < 2)→ enabled=true, Typewriter The government is not going to be able to get to the bottom of the road.
  // It's not because progress And become 2 And the text is dumped in the middle of the road.
  const [typewriterEnabled] = React.useState(() => progress !== 2);
  // Tracks whether the typing animation is complete.
  // Every time content Reset Changes to false(Typewriter The blogger adds that the government is not yet in a position to do so.
  // Wait. Typewriter We'll catch up and pass. onComplete Reset As true.
  const [isTypingComplete, setIsTypingComplete] = React.useState(
    progress === 2,
  );
  // content Reset typing to mark when changing, but only current messages (typewriterEnabled=trueI'm not sure I'm going to be able to do this.
  // History Message (Hype)typewriterEnabled=false) full-text is visible, no animation, etc., should not be reset to false It can't be recovered after.
  React.useEffect(() => {
    if (typewriterEnabled) {
      setIsTypingComplete(false);
    }
  }, [content, typewriterEnabled]);
  const handleTypewriterComplete = React.useCallback(() => {
    setIsTypingComplete(true);
    // Typewriter When the visual pursuit is over, it triggers two vibrations, and it aligns with the touch and the vision.
    // Not on the data level. isEnd The post was published in the blog of the blog @Assamblog, which is a post on the blog of the blog #StandStandStandStamp.
    triggerHaptic('impactHeavy');
    setTimeout(() => {
      triggerHaptic('impactHeavy');
    }, 50);
  }, []);
  const shouldShowLoadingBubble =
    !isQuestion && isLoading && !content && !voicePayload;
  // The toolbars need to meet two conditions to show:
  // 1. Message Receiving Completed (progress === 2)
  // 2. Type-Animation Done (Phone)isTypingComplete) avoids the toolbars when animation is still in progress
  const shouldShowLatestAnswerToolbar =
    !isQuestion && isLatest && !shouldShowLoadingBubble && progress === 2 && isTypingComplete;
  const parsedVoicePayload = React.useMemo(
    () => parseVoicePayload(voicePayload),
    [voicePayload],
  );
  const isVoiceBubble = !!parsedVoicePayload;
  const bubbleFrameStyle = isQuestion
    ? styles.questionBubbleFrame
    : styles.answerBubbleFrame;
  const {
    handleEnterDeleteSelection,
    handleEnterShareSelection,
    handleMoreMenuBackdropPress,
    handleMoreMenuFeedbackPress,
    handleMoreMenuLayout,
    handleNormalPress,
    handleToolbarCopy,
    isCopied,
    isVoiceBubblePlaying,
    menuOptions,
    moreMenuAnimatedStyle,
    moreMenuBackdropAnimatedStyle,
    readAloudStatus,
    resolvedMoreMenuStyle,
    shouldShowReadAloudEntry,
    showMoreMenu,
    triggerReadAloud,
  } = useChatBubbleEvents({
    appId,
    attachments,
    colors,
    content,
    isQuestion,
    msgId,
    onCopy,
    onEdit,
    onFeedback,
    parsedVoicePayload,
    selectionMode,
    t,
    tabId,
    type,
    voicePayload,
  });

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
  // Keep message typography local to the chat bubble.
  const markdownContentStyle = React.useMemo(
    () => ({
      color: colors.textHeading,
      fontSize: messageTypography.fontSize,
      lineHeight: messageTypography.lineHeight,
    }) satisfies TextStyle,
    [
      colors.textHeading,
      messageTypography.fontSize,
      messageTypography.lineHeight,
    ],
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
  const bubbleBody = (
    <ChatBubbleBody
      content={content}
      isAttachmentOnly={isAttachmentOnly}
      isQuestion={isQuestion}
      isVoiceBubble={isVoiceBubble}
      isVoiceBubblePlaying={isVoiceBubblePlaying}
      parsedAttachments={parsedAttachments}
      parsedVoicePayload={parsedVoicePayload}
      shouldShowLoadingBubble={shouldShowLoadingBubble}
      shouldShowLatestAnswerToolbar={shouldShowLatestAnswerToolbar}
      selectionMode={selectionMode}
      isCopied={isCopied}
      styles={styles}
      colors={colors}
      getDisplayFileName={getDisplayFileName}
      getDisplayFileType={getDisplayFileType}
      markdownContentStyle={markdownContentStyle}
      typewriterEnabled={typewriterEnabled}
      onTypewriterComplete={handleTypewriterComplete}
      t={t}
      onCopy={onCopy}
      onFeedback={onFeedback}
      onEnterShareSelection={msgId && tabId ? handleEnterShareSelection : undefined}
      onEnterDeleteSelection={msgId && tabId ? handleEnterDeleteSelection : undefined}
      onReadAloudPress={triggerReadAloud}
      readAloudStatus={readAloudStatus}
      shouldShowReadAloudEntry={shouldShowReadAloudEntry}
      onToolbarCopy={handleToolbarCopy}
    />
  );

  const bubbleContent = (
    <>
      {bubbleBody}
      <Modal
        visible={showMoreMenu}
        transparent
        animationType="none"
        onRequestClose={handleMoreMenuBackdropPress}
      >
        <TouchableWithoutFeedback
          onPress={handleMoreMenuBackdropPress}
        >
          <Animated.View
            style={[styles.moreMenuBackdrop, moreMenuBackdropAnimatedStyle]}
          >
            {resolvedMoreMenuStyle ? (
              <Animated.View
                style={[
                  styles.moreMenuContainer,
                  resolvedMoreMenuStyle,
                  moreMenuAnimatedStyle,
                ]}
                onLayout={handleMoreMenuLayout}
              >
                <Pressable
                  style={({ pressed }) => [
                    styles.moreMenuItem,
                    pressed && styles.moreMenuItemPressed,
                  ]}
                  onPress={handleMoreMenuFeedbackPress}
                >
                  <Icon type="suggest" size={16} />
                  <AppText style={styles.moreMenuText}>
                    {t('actr.chat.feedback')}
                  </AppText>
                </Pressable>
              </Animated.View>
            ) : null}
          </Animated.View>
        </TouchableWithoutFeedback>
      </Modal>
    </>
  );

  const bubbleShell = menuOptions.length > 0 ? (
    <LongPressMenu
      options={menuOptions}
      avoidPressable={hasAttachments || !!content}
      disabled={selectionMode !== 'none'}
      onPress={!hasAttachments && isVoiceBubble ? handleNormalPress : undefined}
    >
      {bubbleContent}
    </LongPressMenu>
  ) : !hasAttachments ? (
    <Pressable onPress={isVoiceBubble ? handleNormalPress : undefined}>
      {bubbleContent}
    </Pressable>
  ) : (
    <View>{bubbleContent}</View>
  );

  return (
    <View
      style={[
        isQuestion ? styles.questionWrapper : styles.answerWrapper,
        containerStyle,
      ]}
    >
      <View style={bubbleFrameStyle}>
        {bubbleShell}
        {selectionMode !== 'none' && onToggleSelection ? (
          <Pressable
            style={({ pressed }) => [
              styles.selectionOverlay,
              pressed && styles.bubblePressed,
            ]}
            onPress={onToggleSelection}
          />
        ) : null}
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
      minWidth: 0,
    },
    answerWrapper: {
      alignItems: 'flex-start',
      width: '100%',
      minWidth: 0,
      flexShrink: 1,
    },
    questionBubbleFrame: {
      maxWidth: isTablet ? 632 : 316,
      alignSelf: 'flex-end',
      minWidth: 0,
    },
    answerBubbleFrame: {
      maxWidth: '100%',
      alignSelf: 'flex-start',
      minWidth: 0,
      flexShrink: 1,
    },
    selectionOverlay: {
      ...StyleSheet.absoluteFillObject,
      borderRadius: 20,
      zIndex: 2,
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
    attachmentBareBubble: {
      paddingHorizontal: 0,
      paddingVertical: 0,
      backgroundColor: 'transparent',
      minWidth: 0,
    },
    markdownContainer: {
      flexShrink: 1,
      width: '100%',
      minWidth: 0,
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

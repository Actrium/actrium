import React, {
  useEffect,
  useState,
  useCallback,
  useMemo,
  useRef,
} from 'react';
import {
  Animated,
  View,
  Text,
  StyleSheet,
  FlatList,
  ActivityIndicator,
  Pressable,
  useWindowDimensions,
  NativeSyntheticEvent,
  NativeScrollEvent,
  Platform,
  GestureResponderEvent,
  LayoutChangeEvent,
} from 'react-native';
import { useTranslation } from 'react-i18next';
import Clipboard from '@react-native-clipboard/clipboard';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import { AppText } from '../../../components';
import AppInput from '../input/AppInput';
import MobileAttachmentPanel, {
  getMobileAttachmentPanelHeight,
} from '../input/MobileAttachmentPanel';
import LocationPicker from '../extension/LocationPicker';
import {
  MessageRepository,
  IMessage,
  Progress,
} from '../../../repository/MessageRepository';
import {
  useMessageStore,
  type MessagePart,
  type SelectionMode,
} from '../../store/useMessageStore';
import { useAppStore } from '../../store/useAppStore';
import ChatBubble from './ChatBubble';
import { FeedbackFormModal } from './FeedbackFormModal';
import { Toast, ToastRef } from '../shared/Toast';
import ChatEmpty from './ChatEmpty';
import { useTabIdMapStore } from '../../store/useTabIdMapStore';
import { useTab } from '../../contexts/TabContext';
import { useInputStore } from '../../store/useInputStore';
import { IAttachmentItem } from '../../types/attachment';
import { DeleteButton } from '../shared/DeleteButton';
import { ShareConfirmButton } from '../shared/ShareConfirmButton';
import { ScrollToBottomButton } from '../shared/ScrollToBottomButton';
import { Checkbox } from '../shared/Checkbox';
import { AppSessionService } from '../../../services/AppSessionService';
import { getHostLayoutTokens } from '../../../utils/layout';
import { useKeyboardInset } from '../../../hooks/useKeyboardInset';
import { useUnreadStore } from '../../store/useUnreadStore';
import { GlobalAlert } from '../../../components';
import { CHAT_HEADER_HEIGHT } from '../../constants';
import { AppRepository } from '../../../repository/AppRepository';
import {
  updateTabInfo,
} from '../../../native-bridge/tab/TabModule';
import {
  KeyboardStickyView,
  useReanimatedKeyboardAnimation,
} from 'react-native-keyboard-controller';
import Reanimated, {
  useAnimatedStyle,
} from 'react-native-reanimated';
import {
  getTypographyStyle,
  scrollToBottomButton,
  spacing,
} from '../../../theme/tokens';
import { useBookmarkStore } from '../../store/useBookmarkStore';
import { useColors } from '../../../theme/useColors';
import type { ColorTokens } from '../../../theme/colors';
import ChatSharePreviewModal from './share/ChatSharePreviewModal';
import { usePhoneAttachmentPanelController } from './hooks/usePhoneAttachmentPanelController';
import { useReadAloudPlaybackController } from './hooks/useReadAloudPlayback';

const PAGE_SIZE = 20;
const EMPTY_MESSAGE_IDS: string[] = [];
const EMPTY_SELECTED_PARTS = new Set<'attachments' | 'question' | 'answer'>();
const ATTACHMENT_PANEL_OPEN_DURATION = 220;
const ATTACHMENT_PANEL_CLOSE_DURATION = 180;
// The maximum pressure time allowed to be recognized for a content area short of the attachment area closes.
const CONTENT_TAP_MAX_DURATION = 220;
// The maximum offset of the click is allowed when content area is short-clicked to close attachment area.
const CONTENT_TAP_MAX_MOVE = 8;

interface MessageRowProps {
  tabId: string;
  appId: string;
  messageId: string;
  layoutTokens: ReturnType<typeof getHostLayoutTokens>;
  isLatest: boolean;
  toggleSelection: (
    messageId: string,
    part: MessagePart,
  ) => void;
  handleEdit: (msg: IMessage) => void;
  handleCopy: (text: string) => void;
  handleFeedback?: (msgId: string) => void;
  styles: ReturnType<typeof makeStyles>;
}

const SELECTION_CHECKBOX_SLOT_WIDTH = 30;

const DeleteSelectionBar = React.memo(function DeleteSelectionBar({
  onPress,
  onCancel,
}: {
  onPress: () => void;
  onCancel: () => void;
}) {
  const selectedCount = useMessageStore(
    useCallback((state) => {
      let count = 0;
      for (const parts of Object.values(state.selectedItems)) {
        count += parts.size;
      }
      return count;
    }, []),
  );

  return (
    <DeleteButton
      selectedCount={selectedCount}
      onPress={onPress}
      onCancel={onCancel}
    />
  );
});

const MessageRow = React.memo(function MessageRow({
  tabId,
  appId,
  messageId,
  layoutTokens,
  isLatest,
  toggleSelection,
  handleEdit,
  handleCopy,
  handleFeedback,
  styles,
}: MessageRowProps) {
  const item = useMessageStore(
    useCallback(
      (state) => state.messageById[tabId]?.[messageId],
      [tabId, messageId],
    ),
  );
  const selectionMode = useMessageStore((state) => state.selectionMode);
  const selectedParts = useMessageStore(
    useCallback(
      (state) => state.selectedItems[messageId] || EMPTY_SELECTED_PARTS,
      [messageId],
    ),
  );
  const isSelectionMode = selectionMode !== 'none';

  if (!item) return null;

  return (
    <View
      style={[
        styles.messageListWrapper,
        {
          maxWidth: layoutTokens.contentMaxWidth,
          paddingHorizontal: layoutTokens.messageHorizontalPadding,
        },
      ]}
    >
      <View style={styles.messageGroup}>
        {item.attachments && (
          <View style={styles.bubbleWithCheckbox}>
            {isSelectionMode ? (
              <View style={styles.checkboxContainer}>
                <Checkbox
                  checked={selectedParts.has('attachments')}
                  onPress={() =>
                    toggleSelection(item.message_id, 'attachments')
                  }
                />
              </View>
            ) : null}
            <View style={styles.questionBubbleWrapper}>
              <ChatBubble
                type='question'
                attachments={item.attachments}
                containerStyle={{ marginBottom: 4 }}
                onEdit={
                  isLatest && selectionMode === 'none' && !item.is_edited
                    ? () => handleEdit(item)
                    : undefined
                }
                onCopy={() => handleCopy(item.question_content)}
                onFeedback={() => handleFeedback?.(item.message_id)}
                msgId={item.message_id}
                tabId={tabId}
                appId={appId}
                selectionMode={selectionMode}
                onToggleSelection={() =>
                  toggleSelection(item.message_id, 'attachments')
                }
              />
            </View>
          </View>
        )}
        {item.question_content || item.question_type === 'voice' ? (
          <View style={styles.bubbleWithCheckbox}>
            {isSelectionMode ? (
              <View style={styles.checkboxContainer}>
                <Checkbox
                  checked={selectedParts.has('question')}
                  onPress={() => toggleSelection(item.message_id, 'question')}
                />
              </View>
            ) : null}
            <View style={styles.questionBubbleWrapper}>
              <ChatBubble
                type='question'
                msgId={item.message_id}
                tabId={tabId}
                appId={appId}
                content={item.question_content}
                voicePayload={item.question_voice_payload}
                selectionMode={selectionMode}
                onEdit={
                  isLatest && selectionMode === 'none' && item.question_type !== 'voice' && !item.is_edited
                    ? () => handleEdit(item)
                    : undefined
                }
                onCopy={
                  item.question_type !== 'voice'
                    ? () => handleCopy(item.question_content)
                    : undefined
                }
                onFeedback={() => handleFeedback?.(item.message_id)}
                onToggleSelection={() =>
                  toggleSelection(item.message_id, 'question')
                }
              />
            </View>
          </View>
        ) : null}
        {item.answer_content ||
        item.answer_type === 'voice' ||
        (isLatest &&
          (item.progress === Progress.pending ||
            item.progress === Progress.processing)) ? (
          <View style={styles.bubbleWithCheckbox}>
            {isSelectionMode ? (
              <View style={styles.checkboxContainer}>
                <Checkbox
                  checked={selectedParts.has('answer')}
                  onPress={() => toggleSelection(item.message_id, 'answer')}
                />
              </View>
            ) : null}
            <View style={styles.answerBubbleWrapper}>
              <ChatBubble
                type='answer'
                msgId={item.message_id}
                tabId={tabId}
                appId={appId}
                content={item.answer_content}
                voicePayload={item.answer_voice_payload}
                selectionMode={selectionMode}
                isLoading={
                  isLatest &&
                  (item.progress === Progress.pending ||
                    item.progress === Progress.processing)
                }
                progress={item.progress}
                isLatest={isLatest}
                onCopy={
                  item.answer_type !== 'voice'
                    ? () => handleCopy(item.answer_content)
                    : undefined
                }
                onFeedback={() => handleFeedback?.(item.message_id)}
                onToggleSelection={() =>
                  toggleSelection(item.message_id, 'answer')
                }
              />
            </View>
          </View>
        ) : null}
      </View>
    </View>
  );
});

export default function Chat() {
  useReadAloudPlaybackController();

  const { t } = useTranslation();
  const { tab } = useTab();
  const { width } = useWindowDimensions();
  const insets = useSafeAreaInsets();
  const appId = useTabIdMapStore((state) => state.tabIdMap[tab.id]);
  const layoutTokens = useMemo(() => getHostLayoutTokens(width), [width]);
  const attachmentPanelHeight = useMemo(
    () => getMobileAttachmentPanelHeight(insets.bottom),
    [insets.bottom],
  );
  const colors = useColors();
  const styles = useMemo(() => makeStyles(colors), [colors]);
  const phoneAttachmentPanelController = usePhoneAttachmentPanelController(
    appId,
    tab.id,
  );

  const messageIds = useMessageStore(
    useCallback(
      (state) => state.messageIds[tab.id] || EMPTY_MESSAGE_IDS,
      [tab.id],
    ),
  );
  const setMessagesInStore = useMessageStore((state) => state.setMessages);
  const appendOldMessages = useMessageStore((state) => state.appendOldMessages);
  const updateMessage = useMessageStore((state) => state.updateMessage);
  const cleanMessage = useMessageStore((state) => state.cleanMessage);
  const selectionMode = useMessageStore((state) => state.selectionMode);
  const toggleSelection = useMessageStore((state) => state.toggleSelection);
  const exitSelectionMode = useMessageStore((state) => state.exitSelectionMode);
  const deleteSelectedMessages = useMessageStore(
    (state) => state.deleteSelectedMessages,
  );

  const appInfo = useAppStore((state) => state.apps[appId]);
  const setAppInfo = useAppStore((state) => state.setAppInfo);
  const removeTabIdMap = useTabIdMapStore((state) => state.removeTabIdMap);
  const setStoreText = useInputStore((state) => state.setInputText);
  const clearInput = useInputStore((state) => state.clearInput);
  const setStoreAttachments = useInputStore((state) => state.setAttachments);
  const setEditingMessageId = useInputStore(
    (state) => state.setEditingMessageId,
  );
  const editingMessageId = useInputStore(
    (state) => state.inputs[tab.id]?.editingMessageId,
  );

  const [isLoadingMore, setIsLoadingMore] = useState(false);
  const [hasLoadedInitialMessages, setHasLoadedInitialMessages] =
    useState(false);
  const [hasMore, setHasMore] = useState(true);
  const [showScrollToBottom, setShowScrollToBottom] = useState(false);
  const [showLoadMoreHeader, setShowLoadMoreHeader] = useState(false);
  const [feedbackMessageId, setFeedbackMessageId] = useState<string | null>(null);
  // A list of information to be shared after the sharing mode has been confirmed, which is open for sharing previews when you are not empty.
  const [shareMessages, setShareMessages] = useState<IMessage[]>([]);
  const [inputDockHeight, setInputDockHeight] = useState(0);
  const [inputDockMinHeight, setInputDockMinHeight] = useState(0);
  const { keyboardInset, isKeyboardVisible, dismissKeyboard } =
    useKeyboardInset();
  const { height: keyboardHeight } = useReanimatedKeyboardAnimation();
  const toastRef = React.useRef<ToastRef>(null);
  const flatListRef = useRef<FlatList>(null);
  const isNearBottomRef = useRef(true);
  const previousMessageCountRef = useRef(messageIds.length);
  const forceScrollToBottomUntilRef = useRef(0);
  const firstMessageIdRef = useRef<string | undefined>(messageIds[0]);
  const deferredExitSelectionFrameRef = useRef<number | null>(null);

  // Passed under the positive sequence. scrollToEnd Back to the bottom.
  const scrollToBottom = useCallback(() => {
    flatListRef.current?.scrollToEnd({ animated: true });
  }, []);

  useEffect(() => {
    setInputDockHeight(0);
    setInputDockMinHeight(0);
  }, [layoutTokens.isTablet]);

  const handleInputLayout = useCallback((event: LayoutChangeEvent) => {
    const nextHeight = Math.round(event.nativeEvent.layout.height);
    if (nextHeight <= 0) return;

    setInputDockHeight((prev) => (
      Math.abs(prev - nextHeight) > 1 ? nextHeight : prev
    ));
    setInputDockMinHeight((prev) => {
      if (prev <= 0) return nextHeight;
      return Math.min(prev, nextHeight);
    });
  }, []);

  // The input area is higher in multiple rows of text or attachment previews, and the buttons compensate for increases beyond the baseline and avoid blocking the input area.
  const scrollToBottomBottomOffset = useMemo(() => {
    const baseOffset = layoutTokens.isTablet
      ? scrollToBottomButton.bottomOffsetPad
      : scrollToBottomButton.bottomOffset;
    const extraOffset = Math.max(0, inputDockHeight - inputDockMinHeight);
    return baseOffset + extraOffset;
  }, [inputDockHeight, inputDockMinHeight, layoutTokens.isTablet]);

  const handleContentSizeChange = useCallback(() => {
    if (selectionMode !== 'none') return;

    // Keep the list pinned when asynchronous message content grows near the bottom.
    const shouldKeepBottom =
      isNearBottomRef.current ||
      Date.now() < forceScrollToBottomUntilRef.current;
    if (!shouldKeepBottom) return;

    // animated: true The project is designed to help people to move in a higher and more smooth way.
    // Complement Typewriter It's... LayoutAnimation Remove the bottom flash.
    requestAnimationFrame(() => {
      flatListRef.current?.scrollToEnd({ animated: true });
    });
  }, [selectionMode]);

  const handleCopy = useCallback((text: string) => {
    if (!text) return;
    try {
      Clipboard.setString(text);
      toastRef.current?.show(t('actr.chat.copySuccess'));
    } catch (e) {
      console.error('Failed to copy to clipboard', e);
      toastRef.current?.show(t('actr.chat.copyFailed'));
    }
  }, []);

  const showTopToast = useCallback((message: string) => {
    toastRef.current?.show(message);
  }, []);

  const loadAppInfo = useCallback(async () => {
    if (!appId || appInfo) return;

    const initResult = await AppSessionService.initSession(tab.id, appId);
    if (initResult.ok) return;

    // - Yeah. preset appUrl Enter Chat — Priority is given to respecting the results on the line;
    // Only when the interface fails will it be allowed to return to SQLite The old application information in it continues to restore the history session.
    if (!tab.appUrl) {
      return;
    }

    const localAppInfo = await AppRepository.getApp(appId);
    if (!localAppInfo) {
      return;
    }

    // DB - Yes. url Could be old, keep the current first when the old session is restored tab It's over the news. appUrl,
    // ♪ Promised to follow-up sharing,tab The message and host re-enactment semantics still correspond to the address opened this time.
    const resolvedAppInfo = {
      ...localAppInfo,
      url: tab.appUrl || localAppInfo.url,
    };

    setAppInfo(appId, resolvedAppInfo);
    void updateTabInfo({
      tabId: tab.id,
      title: resolvedAppInfo.instance_name,
      icon: resolvedAppInfo.info?.logo || '',
      appUrl: resolvedAppInfo.url || '',
      status: resolvedAppInfo.app_status
    });
  }, [appId, appInfo, setAppInfo, tab.appUrl, tab.id]);

  const syncInitialBookmarkStatus = useCallback(async () => {
    if (!appId) return;

    try {
      // Chat Pages Only Bookmarks store ;
      // If not already loaded with bookmarks in the current process, first store Ununified Return Source SQLite.
      await useBookmarkStore.getState().loadBookmarks();
      await useBookmarkStore.getState().syncNativeBookmarkStatusByAppId({
        appId,
      });
    } catch (error) {
      console.error('[Chat] Failed to sync initial bookmark status:', error);
    }
  }, [appId, tab.id]);

  // Keep it for now.
  // Initialise Loading Page 1
  const loadInitialMessages = useCallback(async () => {
    setHasLoadedInitialMessages(false);
    try {
      const data = await MessageRepository.getAllMessages(tab.id, PAGE_SIZE, 0);
      setMessagesInStore(tab.id, data);

      if (tab.appUrl && data.length > 0) {
        const latestMessage = data.reduce((latest, current) => {
          if (current.question_time !== latest.question_time) {
            return current.question_time > latest.question_time
              ? current
              : latest;
          }
          return current.message_id.localeCompare(latest.message_id) > 0
            ? current
            : latest;
        });

        // - Yeah. appUrl Entering a chat page is usually an old session in recovery, initializing the current tab The last news forced the completion of the process.
        // Avoiding the legacy of history. pending/processing The state is stuck after the re-opening.
        if (latestMessage.progress !== Progress.completed) {
          updateMessage(
            tab.id,
            latestMessage.message_id,
            { progress: Progress.completed },
            true,
          );
        }
      }

      setHasMore(data.length === PAGE_SIZE);
    } catch (error) {
      console.error('[Main] Failed to load initial messages:', error);
      setMessagesInStore(tab.id, []);
    } finally {
      // Only initial historical queries are allowed to display empty space and to avoid short flashings from loading process, when the results are still empty ChatEmpty.
      setHasLoadedInitialMessages(true);
    }
  }, [tab.id, tab.appUrl, setMessagesInStore, updateMessage]);

  // Load More
  const handleLoadMore = useCallback(async () => {
    // Prevent loading before completion or completion on the first page messages Trigger for time-out
    if (isLoadingMore || !hasMore || messageIds.length === 0) return;

    setIsLoadingMore(true);
    try {
      const offset = messageIds.length;
      const data = await MessageRepository.getAllMessages(
        tab.id,
        PAGE_SIZE,
        offset,
      );

      if (data.length > 0) {
        appendOldMessages(tab.id, data);
      }

      setHasMore(data.length === PAGE_SIZE);
    } catch (error) {
      console.error('[Main] Failed to load more messages:', error);
    } finally {
      setIsLoadingMore(false);
    }
  }, [tab.id, messageIds.length, isLoadingMore, hasMore, appendOldMessages]);

  // Listens to the scroll position, determines whether the bottom button is shown and loads older messages as they approach the top.
  const handleScroll = useCallback(
    (event: NativeSyntheticEvent<NativeScrollEvent>) => {
      const { contentOffset, contentSize, layoutMeasurement } =
        event.nativeEvent;
      const distanceFromBottom =
        contentSize.height - layoutMeasurement.height - contentOffset.y;
      const canScrollForHistory =
        contentSize.height > layoutMeasurement.height + 8;
      const isNearTop = contentOffset.y < 120;

      isNearBottomRef.current = distanceFromBottom < 80;
      setShowScrollToBottom(distanceFromBottom > 200);
      setShowLoadMoreHeader(hasMore && canScrollForHistory && isNearTop);

      if (hasMore && canScrollForHistory && isNearTop) {
        handleLoadMore();
      }
    },
    [handleLoadMore, hasMore],
  );

  useEffect(() => {
    // FIXME Not from now on. db Load history messages because each time they are created tab It's not the same.
    // 🌟 Unconditional: ensure that each mount is synchronized with the latest data on the first page, covering the errors caused by the old cache
    loadInitialMessages();
    loadAppInfo();

    return () => {
      // Clear current on destruction tab The history of this conversation.id Map, unread messages, output box state,tabidMap
      // debug The environment is hot and up-to-date, not clean.
      if (!__DEV__) {
        console.warn(`Clear Data tab: ${tab.id}`)
        cleanMessage(tab.id);
        removeTabIdMap(tab.id);
        useUnreadStore.getState().clearUnread(tab.id);
        clearInput(tab.id);
        removeTabIdMap(tab.id);
      }
      if (deferredExitSelectionFrameRef.current !== null) {
        cancelAnimationFrame(deferredExitSelectionFrameRef.current);
        deferredExitSelectionFrameRef.current = null;
      }
    };
  }, [tab.id]);

  useEffect(() => {
    void syncInitialBookmarkStatus();
  }, [syncInitialBookmarkStatus]);

  useEffect(() => {
    const previousCount = previousMessageCountRef.current;
    if (messageIds.length > previousCount) {
      // By comparing the first message ID The blogger says that the video is not a historical message:
      // History message inserted into the head messageIds[0] It changes. It's not a time to roll down.
      // By maintainVisibleContentPosition Keeps the current position.
      const isHistoryLoad = messageIds[0] !== firstMessageIdRef.current;
      if (!isHistoryLoad) {
        // Short window after new message insert, allow follow-up loading/The step-by-step content continues to push the list to the end.
        forceScrollToBottomUntilRef.current = Date.now() + 1200;
        requestAnimationFrame(() => {
          flatListRef.current?.scrollToEnd({ animated: false });
        });
        setTimeout(() => {
          flatListRef.current?.scrollToEnd({ animated: false });
        }, 120);
      }
    }
    firstMessageIdRef.current = messageIds[0];
    previousMessageCountRef.current = messageIds.length;
  }, [messageIds.length]);

  useEffect(() => {
    if (!isKeyboardVisible) return;
    // Follow the keyboard frame Change continues to bottom up, avoid“Keyboard Raises, Lists and Scrolls”Two senses.
    requestAnimationFrame(() => {
      flatListRef.current?.scrollToEnd({ animated: false });
    });
  }, [isKeyboardVisible, keyboardInset]);

  const handleEdit = useCallback(
    (msg: IMessage) => {
      // Fill Text
      setStoreText(tab.id, msg.question_content || '');

      // Setup Editing Message ID
      setEditingMessageId(tab.id, msg.message_id);

      // Fill Attachment
      if (msg.attachments) {
        try {
          const parsed = JSON.parse(msg.attachments);
          const mapped: IAttachmentItem[] = parsed.map((att: any) => ({
            id: Date.now().toString() + Math.random(),
            attachment_type: att.attachment_type,
            local_url: att.local_url,
            cdn_url: att.cdn_url,
            file_type: att.file_type,
            file_name: att.file_name,
            aspect_ratio: att.aspect_ratio,
            latitude: att.latitude,
            longitude: att.longitude,
          }));
          setStoreAttachments(tab.id, mapped);
        } catch (e) {
          console.error('Failed to parse attachments for edit', e);
        }
      } else {
        setStoreAttachments(tab.id, []);
      }
    },
    [tab.id, setStoreText, setStoreAttachments, setEditingMessageId],
  );

  const handleCancelEdit = useCallback(() => {
    // One-time emptying of text, attachments and editing IDAvoid Android Exiting the editing and cleaning the text create a focus middle.
    clearInput(tab.id);
  }, [tab.id, clearInput]);

  const input = useMemo(() => {
    return (
      <AppInput
        appId={appId}
        showTopToast={showTopToast}
        onLayout={handleInputLayout}
        mobileAttachmentController={
          layoutTokens.isTablet ? undefined : phoneAttachmentPanelController
        }
      />
    );
  }, [
    appId,
    handleInputLayout,
    layoutTokens.isTablet,
    phoneAttachmentPanelController,
    showTopToast,
  ]);

  // Android The host is here. Native Layer processing keyboard lift,
  // Chat Pages only iOS ♪ Connections, avoid ♪ Android Pad One more layer. keyboardInset.
  const shouldApplyKeyboardInset = Platform.OS === 'ios' && layoutTokens.isTablet;
  const attachmentPanelTranslateY = useRef(new Animated.Value(0)).current;

  useEffect(() => {
    if (layoutTokens.isTablet) {
      attachmentPanelTranslateY.setValue(0);
      return;
    }

    Animated.timing(attachmentPanelTranslateY, {
      // Phone End attachments are located below the input box and are then expanded by moving the whole section up to the attachment area,
      // No more compression FlatList Height.
      toValue: phoneAttachmentPanelController.isAttachmentPanelOpen
        ? -attachmentPanelHeight
        : 0,
      duration: phoneAttachmentPanelController.isAttachmentPanelOpen
        ? ATTACHMENT_PANEL_OPEN_DURATION
        : ATTACHMENT_PANEL_CLOSE_DURATION,
      useNativeDriver: true,
    }).start();
  }, [
    attachmentPanelTranslateY,
    attachmentPanelHeight,
    layoutTokens.isTablet,
    phoneAttachmentPanelController.isAttachmentPanelOpen,
  ]);

  const contentAreaAnimatedStyle = useAnimatedStyle(() => {
    if (Platform.OS !== 'ios' || layoutTokens.isTablet) {
      return {
        paddingBottom: 0,
      };
    }

    return {
      // keyboard-controller Yes. iOS It gives negative shifts, and here it turns to the positive bottom edge.
      // Allows the content area to be compressed in real time with the keyboard height instead of staggering it when the keyboard is collected.
      paddingBottom: Math.max(-keyboardHeight.value, 0),
    };
  }, [layoutTokens.isTablet]);
  const isEmptyState =
    hasLoadedInitialMessages && messageIds.length === 0 && !isLoadingMore;
  // iOS Phone The content area will follow first in the air. keyboard-controller Move up,
  // Then enter the bottom security zone and leave it behind. insets.bottom Cut to a smaller value, resulting in a secondary pull back of the middle-air.
  // Here, the height of the shrinking input area is restored to the empty container, so as not to appear.“Up and down. / Go on, go on, go on.”The twitch.
  const emptyStateBottomCompensation =
    Platform.OS === 'ios' && !layoutTokens.isTablet && isKeyboardVisible
      ? Math.max(insets.bottom - Math.max(insets.bottom / 2, 10), 0)
      : 0;


  const handleFeedback = (msgId: string) => {
    if (appInfo?.app_status !== 'alive') {
      GlobalAlert.alert(t('actr.app.notExist'));
      return;
    }
    setFeedbackMessageId(msgId)
  }

  const renderItem = useCallback(
    ({ item: messageId, index }: { item: string; index: number }) => {
      return (
        <MessageRow
          tabId={tab.id}
          appId={appId}
          messageId={messageId}
          layoutTokens={layoutTokens}
          isLatest={index === messageIds.length - 1}
          toggleSelection={toggleSelection}
          handleEdit={handleEdit}
          handleCopy={handleCopy}
          handleFeedback={(msgId) => handleFeedback(msgId)}
          styles={styles}
        />
      );
    },
    [
      tab.id,
      appId,
      appInfo?.app_status,
      layoutTokens,
      messageIds.length,
      toggleSelection,
      handleEdit,
      handleCopy,
      styles,
    ],
  );

  const ListHeaderComponent = useMemo(() => {
    if (!showLoadMoreHeader) return null;
    return (
      <View style={{ paddingVertical: 20 }}>
        {isLoadingMore ? (
          <ActivityIndicator size='small' color={colors.textTertiary} />
        ) : (
          <AppText style={styles.loadMoreText}>
            {t('actr.chat.loadMore')}
          </AppText>
        )}
      </View>
    );
  }, [isLoadingMore, showLoadMoreHeader, t, colors, styles]);

  // Process the deletion button click
  const handleDeletePress = useCallback(() => {
    GlobalAlert.alert(
      t('actr.chat.deleteConfirmTitle'),
      t('actr.chat.deleteConfirmMsg'),
      [
        {
          text: t('common.cancel'),
          style: 'cancel',
          onPress: () => {
            exitSelectionMode();
          },
        },
        {
          text: t('common.delete'),
          style: 'destructive',
          onPress: () => {
            // Use setTimeout Avoid blocking. Alert Close Animation
            setTimeout(() => {
              deleteSelectedMessages(tab.id).catch((error) => {
                console.error(
                  '[Chat] Failed to delete selected messages:',
                  error,
                );
                toastRef.current?.show(t('actr.chat.deleteFailed'));
              });
            }, 0);
          },
        },
      ],
    );
  }, [tab.id, deleteSelectedMessages, exitSelectionMode, t]);

  // Click"Generate Preview", and then delay the frame exit selection mode to avoid the list exiting the animation block window frame.
  const handleShareConfirm = useCallback(() => {
    const byId = useMessageStore.getState().messageById[tab.id] || {};
    const orderedIds = useMessageStore.getState().messageIds[tab.id] || [];
    const currentSelectedItems = useMessageStore.getState().selectedItems;

    // Ensures that the message is in the original order of the list, and that the order of the message is the same as the chat.
    const orderedMessages: IMessage[] = [];
    for (const id of orderedIds) {
      const parts = currentSelectedItems[id];
      if (!parts || parts.size === 0) continue;
      const message = byId[id];
      if (!message) continue;

      // By Selected part Builds a partial message that contains only the user selected.
      const partial: IMessage = { ...message };
      if (!parts.has('attachments')) partial.attachments = undefined;
      if (!parts.has('question')) {
        partial.question_content = '';
        partial.question_voice_payload = undefined;
      }
      if (!parts.has('answer')) {
        partial.answer_content = '';
        partial.answer_voice_payload = undefined;
      }
      orderedMessages.push(partial);
    }

    setShareMessages(orderedMessages);
    if (deferredExitSelectionFrameRef.current !== null) {
      cancelAnimationFrame(deferredExitSelectionFrameRef.current);
    }
    deferredExitSelectionFrameRef.current = requestAnimationFrame(() => {
      deferredExitSelectionFrameRef.current = requestAnimationFrame(() => {
        deferredExitSelectionFrameRef.current = null;
        exitSelectionMode();
      });
    });
  }, [tab.id, exitSelectionMode]);

  const handleContentScrollBeginDrag = useCallback(() => {
    dismissKeyboard();

    // Phone , and avoids the list rolling and bottom extension being side by side.
    if (
      !layoutTokens.isTablet &&
      phoneAttachmentPanelController.isAttachmentPanelOpen
    ) {
      phoneAttachmentPanelController.closeAttachmentPanel();
    }
  }, [
    dismissKeyboard,
    layoutTokens.isTablet,
    phoneAttachmentPanelController,
  ]);

  const contentTouchStartRef = useRef<{
    time: number;
    x: number;
    y: number;
    moved: boolean;
  } | null>(null);

  // Records the starting point of the touch of the content area, which is used to distinguish short, long and drag.
  const handleContentTouchStart = useCallback(
    (event: GestureResponderEvent) => {
      const { pageX, pageY } = event.nativeEvent;
      contentTouchStartRef.current = {
        time: Date.now(),
        x: pageX,
        y: pageY,
        moved: false,
      };
    },
    [],
  );

  // Updates the touch state when your finger moves, avoiding a drag miscalculation to a short click.
  const handleContentTouchMove = useCallback(
    (event: GestureResponderEvent) => {
      const touchState = contentTouchStartRef.current;
      if (!touchState || touchState.moved) {
        return;
      }

      const { pageX, pageY } = event.nativeEvent;
      const movedX = Math.abs(pageX - touchState.x);
      const movedY = Math.abs(pageY - touchState.y);

      if (movedX > CONTENT_TAP_MAX_MOVE || movedY > CONTENT_TAP_MAX_MOVE) {
        touchState.moved = true;
      }
    },
    [],
  );

  // At the end of the touch of the content area, the attachment area is closed only by short-slash action, and the length of the length is maintained to interact with the drag.
  const handleContentTouchEnd = useCallback(() => {
    const touchState = contentTouchStartRef.current;
    contentTouchStartRef.current = null;

    if (!touchState) {
      return;
    }

    const duration = Date.now() - touchState.time;

    // Only short content zones are closed, and neither the long nor the drag will be triggered, avoiding the error of the long message by the ability to press.
    if (
      !layoutTokens.isTablet &&
      phoneAttachmentPanelController.isAttachmentPanelOpen &&
      !touchState.moved &&
      duration <= CONTENT_TAP_MAX_DURATION
    ) {
      phoneAttachmentPanelController.closeAttachmentPanel();
    }
  }, [layoutTokens.isTablet, phoneAttachmentPanelController]);

  const contentNode = (
    <Reanimated.View
      style={[styles.contentArea, contentAreaAnimatedStyle]}
      onTouchStart={handleContentTouchStart}
      onTouchMove={handleContentTouchMove}
      onTouchEnd={handleContentTouchEnd}
    >
      {isEmptyState ? (
        <View
          style={{
            flex: 1,
            paddingBottom: emptyStateBottomCompensation,
          }}
        >
          <ChatEmpty />
        </View>
      ) : (
        <FlatList
          ref={flatListRef}
          data={messageIds}
          renderItem={renderItem}
          keyExtractor={(item) => item}
          onScroll={handleScroll}
          onContentSizeChange={handleContentSizeChange}
          onScrollBeginDrag={handleContentScrollBeginDrag}
          scrollEventThrottle={16}
          style={styles.chatScrollView}
          contentContainerStyle={[
            styles.chatContent,
            {
              // Pad Modes Not Display ChatHeader, the top retains only the content breathing bits.
              paddingTop: layoutTokens.isTablet
                ? 12
                : CHAT_HEADER_HEIGHT + 12,
              paddingBottom: layoutTokens.chatListBottomPadding,
            },
          ]}
          ListHeaderComponent={ListHeaderComponent}
          ListHeaderComponentStyle={styles.listHeader}
          ListFooterComponent={<View style={styles.listFooterSpacer} />}
          showsHorizontalScrollIndicator={false}
          horizontal={false}
          // Let the scroller pack the keyboard when the list blank area clicks, while no more outer layers Touchable Cut the drag and drag gestures.
          keyboardShouldPersistTaps='never'
          keyboardDismissMode='on-drag'
          maintainVisibleContentPosition={{ minIndexForVisible: 0 }}
          // Performance optimization: reduce the exterior rendering range and limit the number of per batch
          windowSize={5}
          removeClippedSubviews={true}
          maxToRenderPerBatch={5}
          updateCellsBatchingPeriod={50}
        />
      )}
      {/* Chinese Comment: Edit the state mask covers only the message list area, avoids intercepting bottom input boxes and cancels the editing button. */}
      {!!editingMessageId && (
        <Pressable
          style={styles.editModeOverlay}
          onPress={handleCancelEdit}
        />
      )}
    </Reanimated.View>
  );

  return (
    <View style={styles.mainContainer}>
      {!layoutTokens.isTablet ? (
        <View style={styles.phoneMain}>
          <Animated.View
            style={[
              styles.phoneContentMotion,
              {
                transform: [{ translateY: attachmentPanelTranslateY }],
              },
            ]}
          >
            {contentNode}
          </Animated.View>

          <Animated.View
            style={[
              styles.phoneInputMotion,
              {
                transform: [{ translateY: attachmentPanelTranslateY }],
              },
            ]}
          >
            <KeyboardStickyView
              enabled={Platform.OS === 'ios' && !layoutTokens.isTablet}
              style={[
                styles.inputContainer,
                {
                  // backgroundColor: layoutTokens.isTablet ? colors.bgChatTablet : colors.bgChatPage,
                  marginTop: layoutTokens.isTablet ? 8 : 0,
                  marginBottom: shouldApplyKeyboardInset ? keyboardInset : 0,
                },
              ]}
            >
              <View style={styles.bottomDock}>
                <View style={styles.inputInner}>{input}</View>
              </View>
            </KeyboardStickyView>
          </Animated.View>

          <Animated.View
            pointerEvents={
              phoneAttachmentPanelController.isAttachmentPanelOpen ? 'auto' : 'none'
            }
            style={[
              styles.attachmentPanelOverlay,
              {
                bottom: -attachmentPanelHeight,
                transform: [{ translateY: attachmentPanelTranslateY }],
              },
            ]}
          >
            <MobileAttachmentPanel
              expanded={phoneAttachmentPanelController.isAttachmentPanelOpen}
              showSearchEntry={phoneAttachmentPanelController.showSearchEntry}
              showNewlineEntry={phoneAttachmentPanelController.showNewlineEntry}
              isWebSearchActive={phoneAttachmentPanelController.isWebSearchActive}
              isEnterForNewLine={phoneAttachmentPanelController.isEnterForNewLine}
              onPickCamera={phoneAttachmentPanelController.takePhoto}
              onPickImage={phoneAttachmentPanelController.pickImage}
              onPickFile={phoneAttachmentPanelController.pickDocument}
              onPickLocation={phoneAttachmentPanelController.pickLocation}
              onToggleWebSearch={phoneAttachmentPanelController.toggleWebSearch}
              onToggleEnterForNewLine={phoneAttachmentPanelController.toggleEnterForNewLine}
            />
          </Animated.View>
        </View>
      ) : (
        <>
          {contentNode}
          <KeyboardStickyView
            enabled={Platform.OS === 'ios' && !layoutTokens.isTablet}
            style={[
              styles.inputContainer,
              {
                marginTop: layoutTokens.isTablet ? 8 : 0,
                marginBottom: shouldApplyKeyboardInset ? keyboardInset : 0,
              },
            ]}
          >
            <View style={styles.bottomDock}>
              <View style={styles.inputInner}>{input}</View>
            </View>
          </KeyboardStickyView>
        </>
      )}

      {/* Back to Bottom button */}
      {selectionMode === 'none' && (
        <Animated.View
          style={[
            styles.scrollToBottomContainer,
            { bottom: scrollToBottomBottomOffset },
            !layoutTokens.isTablet && {
              transform: [{ translateY: attachmentPanelTranslateY }],
            },
          ]}
        >
          <ScrollToBottomButton
            visible={showScrollToBottom}
            onPress={scrollToBottom}
          />
        </Animated.View>
      )}

      {/* Comment in Chinese: Select the bottom operating bar of the mode, render different confirmation buttons according to the mode. */}
      {selectionMode === 'delete' && (
        <View
          style={[
            styles.deleteButtonContainer,
            { bottom: 0 },
          ]}
        >
          <DeleteSelectionBar
            onPress={handleDeletePress}
            onCancel={exitSelectionMode}
          />
        </View>
      )}
      {selectionMode === 'share' && (
        <View
          style={[
            styles.deleteButtonContainer,
            { bottom: 0 },
          ]}
        >
          <ShareConfirmButton
            onPress={handleShareConfirm}
            onCancel={exitSelectionMode}
          />
        </View>
      )}

      <FeedbackFormModal
        visible={!!feedbackMessageId}
        onClose={() => setFeedbackMessageId(null)}
        tabId={tab.id}
        appId={appId}
        messageId={feedbackMessageId || undefined}
      />

      <ChatSharePreviewModal
        visible={shareMessages.length > 0}
        messages={shareMessages}
        appName={appInfo?.instance_name || appInfo?.app_name}
        appUrl={appInfo?.url}
        onClose={() => setShareMessages([])}
        onSaved={() => {
          toastRef.current?.show(t('actr.chat.shareSaveSuccess'));
        }}
      />

      {!layoutTokens.isTablet && (
        <LocationPicker
          visible={phoneAttachmentPanelController.showMapModal}
          onClose={() => phoneAttachmentPanelController.setShowMapModal(false)}
          // Phone The end attachment locator bottomshows and the bottom attachment panel maintains the same interactive direction.
          presentation='bottom'
          onSelect={(data) => {
            phoneAttachmentPanelController.handleConfirmLocation(data);
            phoneAttachmentPanelController.setShowMapModal(false);
          }}
        />
      )}

      <Toast ref={toastRef} />
    </View>
  );
}

const makeStyles = (colors: ColorTokens) => ({
  ...StyleSheet.create({
    mainContainer: {
      flex: 1,
      backgroundColor: colors.bgChatPage,
    },
    contentArea: {
      flex: 1,
      width: '100%',
    },
    editModeOverlay: {
      position: 'absolute',
      top: 0,
      left: 0,
      right: 0,
      bottom: 0,
      backgroundColor: 'transparent',
      zIndex: 50,
    },
    inputContainer: {
      width: '100%',
      zIndex: 100,
    },
    phoneMain: {
      flex: 1,
    },
    phoneContentMotion: {
      flex: 1,
    },
    phoneInputMotion: {
      width: '100%',
    },
    attachmentPanelOverlay: {
      position: 'absolute',
      left: 0,
      right: 0,
      zIndex: 90,
    },
    bottomDock: {
      width: '100%',
    },
    inputInner: {
      width: '100%',
      // It must be retained. Transparency needs to evolve.
      marginTop: -24
    },
    scrollToBottomContainer: {
      position: 'absolute',
      left: 0,
      right: 0,
      alignItems: 'center',
      zIndex: 99,
      pointerEvents: 'box-none',
    },
    chatScrollView: {
      flex: 1,
      width: '100%',
    },
    chatContent: {
      width: '100%',
      alignItems: 'stretch',
    },
    listHeader: {
      paddingBottom: spacing.xl,
    },
    listFooterSpacer: {
      height: 24,
    },
    loadMoreText: {
      textAlign: 'center',
      color: colors.textTertiary,
      ...getTypographyStyle('footnote'),
    },
    messageListWrapper: {
      width: '100%',
      alignSelf: 'center',
    },
    messageGroup: {
      width: '100%',
      marginBottom: spacing.md,
    },
    bubbleWithCheckbox: {
      flexDirection: 'row',
      alignItems: 'flex-start',
      marginBottom: spacing.xxs,
      width: '100%',
    },
    questionBubbleWrapper: {
      flex: 1,
      alignItems: 'flex-end',
      minWidth: 0,
    },
    answerBubbleWrapper: {
      flex: 1,
      alignItems: 'flex-start',
      minWidth: 0,
    },
    checkboxContainer: {
      width: SELECTION_CHECKBOX_SLOT_WIDTH,
      paddingTop: 14,
      alignItems: 'flex-start',
    },
    deleteButtonContainer: {
      position: 'absolute',
      left: 0,
      right: 0,
      alignItems: 'center',
      justifyContent: 'center',
      zIndex: 1001,
      pointerEvents: 'box-none',
    },
  }),
});

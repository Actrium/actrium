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
import ActrInput from '../input/ActrInput';
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
import TemporarySupramarkMockChat from './ChatMock';
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
// 中文注释：内容区短按关闭附件区时，允许识别为点击的最长按压时长。
const CONTENT_TAP_MAX_DURATION = 220;
// 中文注释：内容区短按关闭附件区时，允许识别为点击的最大位移。
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

interface ChatProps {
  showTemporaryMock?: boolean;
}

export default function Chat({ showTemporaryMock = false }: ChatProps) {
  if (showTemporaryMock) {
    return <TemporarySupramarkMockChat />;
  }

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
  // 中文注释：分享模式确认后收集的待分享消息列表，非空时打开分享预览。
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

  // 正序列表下通过 scrollToEnd 回到底部。
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

  // 中文注释：输入区在多行文本或附件预览下会变高，按钮只补偿超出基线的增量，避免遮挡输入区。
  const scrollToBottomBottomOffset = useMemo(() => {
    const baseOffset = layoutTokens.isTablet
      ? scrollToBottomButton.bottomOffsetPad
      : scrollToBottomButton.bottomOffset;
    const extraOffset = Math.max(0, inputDockHeight - inputDockMinHeight);
    return baseOffset + extraOffset;
  }, [inputDockHeight, inputDockMinHeight, layoutTokens.isTablet]);

  const handleContentSizeChange = useCallback(() => {
    if (selectionMode !== 'none') return;

    // 异步内容（如 Supramark 卡片）撑高时，若用户仍在底部附近，则保持贴底。
    const shouldKeepBottom =
      isNearBottomRef.current ||
      Date.now() < forceScrollToBottomUntilRef.current;
    if (!shouldKeepBottom) return;

    requestAnimationFrame(() => {
      flatListRef.current?.scrollToEnd({ animated: false });
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

    // 中文注释：带 preset appUrl 进入 Chat 时，优先尊重线上结果；
    // 只有接口失败后，才允许回落到 SQLite 里的旧应用信息继续恢复历史会话。
    if (!tab.appUrl) {
      return;
    }

    const localAppInfo = await AppRepository.getApp(appId);
    if (!localAppInfo) {
      return;
    }

    // 中文注释：DB 里的 url 可能是旧值，恢复旧会话时要优先保留当前 tab 透传的 appUrl，
    // 保证后续分享、tab 信息和宿主恢复语义仍然对应本次打开的地址。
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
      // 中文注释：聊天页只消费书签 store 的内存态；
      // 若当前进程内尚未加载过书签，则先由 store 统一回源 SQLite。
      await useBookmarkStore.getState().loadBookmarks();
      await useBookmarkStore.getState().syncNativeBookmarkStatusByAppId({
        appId,
      });
    } catch (error) {
      console.error('[Chat] Failed to sync initial bookmark status:', error);
    }
  }, [appId, tab.id]);

  // 暂时先留着
  // 初始化加载第一页
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

        // 带 appUrl 进入聊天页通常是恢复中的旧会话，初始化后把当前 tab 最后一条消息强制收敛为完成态，
        // 避免历史会话残留的 pending/processing 状态在重开后一直卡住。
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
      // 只有初始历史消息查询已经完成，且结果仍为空时，才允许展示空态，避免加载过程短暂闪出 ChatEmpty。
      setHasLoadedInitialMessages(true);
    }
  }, [tab.id, tab.appUrl, setMessagesInStore, updateMessage]);

  // 加载更多
  const handleLoadMore = useCallback(async () => {
    // 防止在第一页加载完成前或 messages 为空时触发
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

  // 监听滚动位置，判断是否显示回到底部按钮，并在接近顶部时加载更旧消息。
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
    // FIXME 暂时不再从 db 加载历史消息，因为每次创建的 tab 都不一样
    // 🌟 无条件执行：确保每次挂载都能同步最新第一页数据，覆盖旧缓存导致的错乱
    loadInitialMessages();
    loadAppInfo();

    return () => {
      // 销毁时清除当前 tab 下的历史对话，id 映射，未读消息数量，输出框状态，tabid映射
      // debug 环境方便热更新，不做清理
      if (!__DEV__) {
        console.warn(`清理数据 tab: ${tab.id}`)
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
      // 通过比较第一条消息 ID 判断是否为历史消息加载：
      // 历史消息插入到头部时 messageIds[0] 会变化，此时不应滚底，
      // 由 maintainVisibleContentPosition 保持当前位置。
      const isHistoryLoad = messageIds[0] !== firstMessageIdRef.current;
      if (!isHistoryLoad) {
        // 新消息插入后的短时间窗口内，允许后续 loading/异步内容继续把列表推到底。
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
    // 跟随键盘 frame 变化持续贴底，避免“键盘先抬起、列表再补滚动”的二段感。
    requestAnimationFrame(() => {
      flatListRef.current?.scrollToEnd({ animated: false });
    });
  }, [isKeyboardVisible, keyboardInset]);

  const handleEdit = useCallback(
    (msg: IMessage) => {
      // 填充文字
      setStoreText(tab.id, msg.question_content || '');

      // 设置正在编辑的消息 ID
      setEditingMessageId(tab.id, msg.message_id);

      // 填充附件
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
    setEditingMessageId(tab.id, null);
    clearInput(tab.id);
  }, [tab.id, setEditingMessageId, clearInput]);

  const input = useMemo(() => {
    return (
      <ActrInput
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

  // 中文注释：Android 宿主已经在 Native 层处理键盘抬升，
  // Chat 页只保留 iOS 的联动，避免 Android Pad 再叠加一层 keyboardInset。
  const shouldApplyKeyboardInset = Platform.OS === 'ios' && layoutTokens.isTablet;
  const attachmentPanelTranslateY = useRef(new Animated.Value(0)).current;

  useEffect(() => {
    if (layoutTokens.isTablet) {
      attachmentPanelTranslateY.setValue(0);
      return;
    }

    Animated.timing(attachmentPanelTranslateY, {
      // 中文注释：Phone 端附件区常驻在输入框下方，展开时通过整块内容上移来露出附件区，
      // 不再压缩 FlatList 高度。
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
      // 中文注释：keyboard-controller 在 iOS 上给出的是负位移，这里转成正的底部内边距，
      // 让内容区随键盘高度实时压缩，而不是等键盘收起后再错位回弹。
      paddingBottom: Math.max(-keyboardHeight.value, 0),
    };
  }, [layoutTokens.isTablet]);
  const isEmptyState =
    hasLoadedInitialMessages && messageIds.length === 0 && !isLoadingMore;
  // 中文注释：iOS Phone 空态下，内容区会先跟随 keyboard-controller 上移，
  // 随后输入区底部安全区留白又从 insets.bottom 切到较小值，导致居中空态被二次拉回。
  // 这里把输入区收缩掉的那段高度补回给空态容器，避免出现“先上后下 / 先下后上”的抖动。
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

  // 处理删除按钮点击
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
            // 使用 setTimeout 避免阻塞 Alert 关闭动画
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

  // 中文注释：点击"生成预览图"时先打开预览层，再延后一帧退出选择模式，避免列表退出动画阻塞弹窗首帧。
  const handleShareConfirm = useCallback(() => {
    const byId = useMessageStore.getState().messageById[tab.id] || {};
    const orderedIds = useMessageStore.getState().messageIds[tab.id] || [];
    const currentSelectedItems = useMessageStore.getState().selectedItems;

    // 中文注释：按消息在列表中的原始顺序排列，保证分享图内容顺序与聊天一致。
    const orderedMessages: IMessage[] = [];
    for (const id of orderedIds) {
      const parts = currentSelectedItems[id];
      if (!parts || parts.size === 0) continue;
      const message = byId[id];
      if (!message) continue;

      // 中文注释：根据选中的 part 构建部分消息，只包含用户选中的内容。
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

    // 中文注释：Phone 端附件区展开时，内容区一旦开始滚动就立即收起，避免列表滚动和底部扩展态并存。
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

  // 中文注释：记录内容区触摸起点，用于区分短按、长按和拖动。
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

  // 中文注释：在手指移动时更新触摸状态，避免把拖动误判成短按点击。
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

  // 中文注释：内容区触摸结束时，只对短按动作收起附件区，保留长按和拖动交互。
  const handleContentTouchEnd = useCallback(() => {
    const touchState = contentTouchStartRef.current;
    contentTouchStartRef.current = null;

    if (!touchState) {
      return;
    }

    const duration = Date.now() - touchState.time;

    // 中文注释：只有短按内容区才收起附件区，长按和拖动都不触发，避免误伤消息长按能力。
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
              // Pad 模式不展示 ChatHeader，顶部只保留内容呼吸位。
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
          // 让列表空白区域点击时由滚动容器负责收起键盘，同时不再用外层 Touchable 截断拖拽手势。
          keyboardShouldPersistTaps='never'
          keyboardDismissMode='on-drag'
          maintainVisibleContentPosition={{ minIndexForVisible: 0 }}
          // 性能优化：减少屏幕外渲染范围，限制每批渲染数量
          windowSize={5}
          removeClippedSubviews={true}
          maxToRenderPerBatch={5}
          updateCellsBatchingPeriod={50}
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

      {/* 编辑模式蒙版 - 点击退出编辑状态 */}
      {!!editingMessageId && (
        <Pressable
          style={styles.editModeOverlay}
          onPress={handleCancelEdit}
        />
      )}

      {/* 回到底部按钮 */}
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

      {/* 中文注释：选择模式底部操作栏，根据模式渲染不同的确认按钮。 */}
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
      // 必须保留，透明度渐变需要
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

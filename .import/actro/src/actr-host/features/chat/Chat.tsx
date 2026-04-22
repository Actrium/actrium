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
} from 'react-native';
import { useTranslation } from 'react-i18next';
import Clipboard from '@react-native-clipboard/clipboard';
import { AppText } from '../../../components';
import ActrInput from '../input/ActrInput';
import {
  MessageRepository,
  IMessage,
  Progress,
} from '../../../repository/MessageRepository';
import { useMessageStore } from '../../store/useMessageStore';
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
import { ScrollToBottomButton } from '../shared/ScrollToBottomButton';
import { Checkbox } from '../shared/Checkbox';
import { AppSessionService } from '../../../services/AppSessionService';
import { getHostLayoutTokens } from '../../../utils/layout';
import { useKeyboardInset } from '../../../hooks/useKeyboardInset';
import { useUnreadStore } from '../../store/useUnreadStore';
import { GlobalAlert } from '../../../components';
import TemporarySupramarkMockChat from './ChatMock';
import { CHAT_HEADER_HEIGHT } from '../../constants';
import { BookmarkRepository } from '../../../repository/BookmarkRepository';
import { AppRepository } from '../../../repository/AppRepository';
import {
  setTabBookmarkStatus,
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
import { useColors } from '../../../theme/useColors';
import type { ColorTokens } from '../../../theme/colors';
import ChatSharePreviewModal from './share/ChatSharePreviewModal';

const PAGE_SIZE = 20;
const EMPTY_MESSAGE_IDS: string[] = [];
const EMPTY_SELECTED_PARTS = new Set<'attachments' | 'question' | 'answer'>();

interface MessageRowProps {
  tabId: string;
  appId: string;
  messageId: string;
  layoutTokens: ReturnType<typeof getHostLayoutTokens>;
  selectionMode: boolean;
  selectedParts: Set<'attachments' | 'question' | 'answer'>;
  isLatest: boolean;
  toggleSelection: (
    messageId: string,
    part: 'attachments' | 'question' | 'answer',
  ) => void;
  handleEdit: (msg: IMessage) => void;
  handleCopy: (text: string) => void;
  handleFeedback?: (msgId: string) => void;
  handleShare?: (msg: IMessage) => void;
  styles: ReturnType<typeof makeStyles>;
}

const CHECKBOX_OFFSET = 30;
const SELECTION_ANIMATION_DURATION = 200;

const MessageRow = React.memo(function MessageRow({
  tabId,
  appId,
  messageId,
  layoutTokens,
  selectionMode,
  selectedParts,
  isLatest,
  toggleSelection,
  handleEdit,
  handleCopy,
  handleFeedback,
  handleShare,
  styles,
}: MessageRowProps) {
  const { t } = useTranslation();
  const item = useMessageStore(
    useCallback(
      (state) => state.messageById[tabId]?.[messageId],
      [tabId, messageId],
    ),
  );

  // 中文注释：选择模式的过渡动画，paddingLeft 控制气泡宽度缩减，translateX + opacity 控制多选框滑入。
  // 使用 RN Animated 替代 reanimated，避免 React.memo 内 useSharedValue 触发 useInsertionEffect 报错。
  const selectionProgress = React.useRef(new Animated.Value(0)).current;

  React.useEffect(() => {
    const animation = Animated.timing(selectionProgress, {
      toValue: selectionMode ? 1 : 0,
      duration: SELECTION_ANIMATION_DURATION,
      useNativeDriver: false,
    });
    animation.start();
    return () => {
      animation.stop();
    };
  }, [selectionMode]);

  const paddingLeftAnimated = selectionProgress.interpolate({
    inputRange: [0, 1],
    outputRange: [0, CHECKBOX_OFFSET],
  });

  const translateXAnimated = selectionProgress.interpolate({
    inputRange: [0, 1],
    outputRange: [-CHECKBOX_OFFSET, 0],
  });

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
      <Animated.View
        style={[styles.messageGroup, { paddingLeft: paddingLeftAnimated }]}
      >
        {item.attachments && (
          <View style={styles.bubbleWithCheckbox}>
            <Animated.View style={[styles.checkboxContainer, { transform: [{ translateX: translateXAnimated }], opacity: selectionProgress }]}>
              <Checkbox
                checked={selectedParts.has('attachments')}
                onPress={() =>
                  toggleSelection(item.message_id, 'attachments')
                }
              />
            </Animated.View>
            <View style={styles.questionBubbleWrapper}>
              <ChatBubble
                type='question'
                attachments={item.attachments}
                containerStyle={{ marginBottom: 4 }}
                onEdit={
                  isLatest && !selectionMode && !item.is_edited
                    ? () => handleEdit(item)
                    : undefined
                }
                onCopy={() => handleCopy(item.question_content)}
                onFeedback={() => handleFeedback?.(item.message_id)}
                msgId={item.message_id}
                tabId={tabId}
                appId={appId}
                selectionKey={`${item.message_id}:attachments`}
                isSelected={selectedParts.has('attachments')}
                onToggleSelection={() =>
                  toggleSelection(item.message_id, 'attachments')
                }
              />
            </View>
          </View>
        )}
        {item.question_content || item.question_type === 'voice' ? (
          <View style={styles.bubbleWithCheckbox}>
            <Animated.View style={[styles.checkboxContainer, { transform: [{ translateX: translateXAnimated }], opacity: selectionProgress }]}>
              <Checkbox
                checked={selectedParts.has('question')}
                onPress={() => toggleSelection(item.message_id, 'question')}
              />
            </Animated.View>
            <View style={styles.questionBubbleWrapper}>
              <ChatBubble
                type='question'
                msgId={item.message_id}
                tabId={tabId}
                appId={appId}
                content={item.question_content}
                voicePayload={item.question_voice_payload}
                onEdit={
                  isLatest && !selectionMode && item.question_type !== 'voice' && !item.is_edited
                    ? () => handleEdit(item)
                    : undefined
                }
                onCopy={
                  item.question_type !== 'voice'
                    ? () => handleCopy(item.question_content)
                    : undefined
                }
                onFeedback={() => handleFeedback?.(item.message_id)}
                onShare={
                  item.answer_content || item.answer_type === 'voice'
                    ? () => handleShare?.(item)
                    : undefined
                }
                selectionKey={`${item.message_id}:question`}
                isSelected={selectedParts.has('question')}
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
            <Animated.View style={[styles.checkboxContainer, { transform: [{ translateX: translateXAnimated }], opacity: selectionProgress }]}>
              <Checkbox
                checked={selectedParts.has('answer')}
                onPress={() => toggleSelection(item.message_id, 'answer')}
              />
            </Animated.View>
            <ChatBubble
              type='answer'
              msgId={item.message_id}
              tabId={tabId}
              appId={appId}
              content={item.answer_content}
              voicePayload={item.answer_voice_payload}
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
              onShare={() => handleShare?.(item)}
              selectionKey={`${item.message_id}:answer`}
              isSelected={selectedParts.has('answer')}
              onToggleSelection={() =>
                toggleSelection(item.message_id, 'answer')
              }
            />
          </View>
        ) : null}
      </Animated.View>
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

  const { t } = useTranslation();
  const { tab } = useTab();
  const { width } = useWindowDimensions();
  const appId = useTabIdMapStore((state) => state.tabIdMap[tab.id]);
  const layoutTokens = useMemo(() => getHostLayoutTokens(width), [width]);
  const colors = useColors();
  const styles = useMemo(() => makeStyles(colors), [colors]);

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
  const selectedItems = useMessageStore((state) => state.selectedItems);
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
  const [shareMessage, setShareMessage] = useState<IMessage | null>(null);
  const { keyboardInset, isKeyboardVisible, dismissKeyboard } =
    useKeyboardInset();
  const { height: keyboardHeight } = useReanimatedKeyboardAnimation();
  const toastRef = React.useRef<ToastRef>(null);
  const flatListRef = useRef<FlatList>(null);
  const isNearBottomRef = useRef(true);
  const previousMessageCountRef = useRef(messageIds.length);
  const forceScrollToBottomUntilRef = useRef(0);
  const firstMessageIdRef = useRef<string | undefined>(messageIds[0]);

  // 正序列表下通过 scrollToEnd 回到底部。
  const scrollToBottom = useCallback(() => {
    flatListRef.current?.scrollToEnd({ animated: true });
  }, []);

  const handleContentSizeChange = useCallback(() => {
    // 异步内容（如 Supramark 卡片）撑高时，若用户仍在底部附近，则保持贴底。
    const shouldKeepBottom =
      isNearBottomRef.current ||
      Date.now() < forceScrollToBottomUntilRef.current;
    if (!shouldKeepBottom) return;

    requestAnimationFrame(() => {
      flatListRef.current?.scrollToEnd({ animated: false });
    });
  }, []);

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
    });
  }, [appId, appInfo, setAppInfo, tab.appUrl, tab.id]);

  const syncInitialBookmarkStatus = useCallback(async () => {
    if (!appId) return;

    try {
      // 首次进入聊天页时，按 appId 回查收藏表并同步当前 tab 的原生收藏态。
      const bookmarkItem = await BookmarkRepository.getBookmarkByAppId(appId);
      
      await setTabBookmarkStatus(tab.id, !!bookmarkItem);
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
            type: att.type,
            name: att.name,
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
    return <ActrInput appId={appId} showTopToast={showTopToast} />;
  }, [appId, showTopToast]);

  // 中文注释：Android 宿主已经在 Native 层处理键盘抬升，
  // Chat 页只保留 iOS 的联动，避免 Android Pad 再叠加一层 keyboardInset。
  const shouldApplyKeyboardInset = Platform.OS === 'ios' && layoutTokens.isTablet;

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

  const renderItem = useCallback(
    ({ item: messageId, index }: { item: string; index: number }) => {
      return (
        <MessageRow
          tabId={tab.id}
          appId={appId}
          messageId={messageId}
          layoutTokens={layoutTokens}
          selectionMode={selectionMode}
          selectedParts={selectedItems[messageId] || EMPTY_SELECTED_PARTS}
          isLatest={index === messageIds.length - 1}
          toggleSelection={toggleSelection}
          handleEdit={handleEdit}
          handleCopy={handleCopy}
          handleFeedback={setFeedbackMessageId}
          handleShare={setShareMessage}
          styles={styles}
        />
      );
    },
    [
      tab.id,
      appId,
      layoutTokens,
      selectionMode,
      selectedItems,
      messageIds.length,
      toggleSelection,
      handleEdit,
      handleCopy,
      setShareMessage,
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

  // 计算选中的数量
  const selectedCount = useMemo(() => {
    let count = 0;
    for (const parts of Object.values(selectedItems)) {
      count += parts.size;
    }
    return count;
  }, [selectedItems]);

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

  return (
    <View style={styles.mainContainer}>
      <Reanimated.View style={[styles.contentArea, contentAreaAnimatedStyle]}>
        {hasLoadedInitialMessages && messageIds.length === 0 && !isLoadingMore ? (
          <View style={{ flex: 1 }}>
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
            onScrollBeginDrag={dismissKeyboard}
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
            showsHorizontalScrollIndicator={false}
            horizontal={false}
            // 让列表空白区域点击时由滚动容器负责收起键盘，同时不再用外层 Touchable 截断拖拽手势。
            keyboardShouldPersistTaps='never'
            keyboardDismissMode='on-drag'
            maintainVisibleContentPosition={{ minIndexForVisible: 0 }}
          />
        )}
      </Reanimated.View>

      {/* 编辑模式蒙版 - 点击退出编辑状态 */}
      {!!editingMessageId && (
        <Pressable
          style={styles.editModeOverlay}
          onPress={handleCancelEdit}
        />
      )}

      {/* 回到底部按钮 */}
      {!selectionMode && (
        <View
          style={[
            styles.scrollToBottomContainer,
            // Pad 端按钮额外上移 40，避免与更高的输入区视觉距离过近。
            layoutTokens.isTablet && styles.scrollToBottomContainerPad,
          ]}
        >
          <ScrollToBottomButton
            visible={showScrollToBottom}
            onPress={scrollToBottom}
          />
        </View>
      )}

      {/* 输入框 */}
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
        <View style={styles.inputInner}>{input}</View>
      </KeyboardStickyView>

      {/* 删除按钮 - 底部 footer 布局，参考 ChatSharePreviewModal */}
      {selectionMode && (
        <View
          style={[
            styles.deleteButtonContainer,
            { bottom: 0 },
          ]}
        >
          <DeleteButton
            selectedCount={selectedCount}
            onPress={handleDeletePress}
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
        visible={!!shareMessage}
        message={shareMessage}
        appName={appInfo?.instance_name || appInfo?.app_name}
        appUrl={appInfo?.url}
        onClose={() => setShareMessage(null)}
        onSaved={() => {
          toastRef.current?.show(t('actr.chat.shareSaveSuccess'));
        }}
      />

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
    inputInner: {
      width: '100%',
    },
    scrollToBottomContainer: {
      position: 'absolute',
      left: 0,
      right: 0,
      bottom: scrollToBottomButton.bottomOffset,
      alignItems: 'center',
      zIndex: 99,
      pointerEvents: 'box-none',
    },
    scrollToBottomContainerPad: {
      bottom: scrollToBottomButton.bottomOffsetPad,
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
      position: 'relative',
      marginBottom: spacing.xxs,
      width: '100%',
    },
    questionBubbleWrapper: {
      flex: 1,
      alignItems: 'flex-end',
      minWidth: 0,
    },
    checkboxContainer: {
      position: 'absolute',
      left: -30,
      top: 14,
      zIndex: 10,
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

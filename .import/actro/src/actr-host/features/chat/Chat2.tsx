import React, {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from 'react';
import {
  ActivityIndicator,
  Animated,
  GestureResponderEvent,
  LayoutChangeEvent,
  NativeScrollEvent,
  NativeSyntheticEvent,
  Platform,
  Pressable,
  StyleSheet,
  useWindowDimensions,
  View,
} from 'react-native';
import { LegendList, type LegendListRef } from '@legendapp/list/react-native';
import { KeyboardStickyView } from 'react-native-keyboard-controller';
import { useTranslation } from 'react-i18next';
import { useColors } from '../../../theme/useColors';
import type { ColorTokens } from '../../../theme/colors';
import {
  IMessage,
  MessageRepository,
  Progress,
} from '../../../repository/MessageRepository';
import { spacing } from '../../../theme/tokens';
import { getHostLayoutTokens } from '../../../utils/layout';
import { CHAT_HEADER_HEIGHT } from '../../constants';
import { useTab } from '../../contexts/TabContext';
import {
  useMessageStore,
  type MessagePart,
} from '../../store/useMessageStore';
import { useInputStore } from '../../store/useInputStore';
import { useAppStore } from '../../store/useAppStore';
import { useChatBottomInsets } from '../../hooks/useChatBottomInsets';
import ActrInput from '../input/ActrInput';
import ChatBubble from './ChatBubble';
import ChatEmpty from './ChatEmpty';
import MobileAttachmentPanel from '../input/MobileAttachmentPanel';
import LocationPicker from '../extension/LocationPicker';
import { usePhoneAttachmentPanelController } from './hooks/usePhoneAttachmentPanelController';
import { useChatSelectionActions } from './hooks/useChatSelectionActions';
import { useChatBubbleActions } from './hooks/useChatBubbleActions';
import { useChatInit } from './hooks/useChatInit';
import { useReadAloudPlaybackController } from './hooks/useReadAloudPlayback';
import { DeleteButton } from '../shared/DeleteButton';
import { ShareConfirmButton } from '../shared/ShareConfirmButton';
import { Checkbox } from '../shared/Checkbox';
import { ScrollToBottomButton } from '../shared/ScrollToBottomButton';
import { Toast, ToastRef } from '../shared/Toast';
import { FeedbackFormModal } from './FeedbackFormModal';
import ChatSharePreviewModal from './share/ChatSharePreviewModal';
import { estimateMessageRowHeight } from './estimateMessageRowHeight';

// ==================== 常量 ====================

// 中文注释：每页加载的消息数量。
const PAGE_SIZE = 20;
// 中文注释：空数组复用，避免 selector 每次返回新引用触发重渲染。
const EMPTY_MESSAGE_IDS: string[] = [];
// 中文注释：空消息映射复用，避免 selector 每次返回新对象触发重渲染。
const EMPTY_MESSAGES_BY_ID: Record<string, IMessage> = {};
// 中文注释：选中部分空集合复用，避免每个 MessageRow 返回新 Set 引用触发重渲染。
const EMPTY_SELECTED_PARTS = new Set<'attachments' | 'question' | 'answer'>();
// 中文注释：距底部小于该值视为"贴近底部"，键盘弹起或异步内容撑高时自动贴底。
const BOTTOM_THRESHOLD = 80;
// 中文注释：距底部超过该值时显示"滚动到底部"按钮。
const SCROLL_TO_BOTTOM_THRESHOLD = 100;
// 中文注释：列表兜底估高，仅在逐条估高函数未命中时使用。
const INITIAL_ESTIMATED_ITEM_SIZE = 160;
// 中文注释：附件面板展开/收起动画时长（毫秒）。
const ATTACHMENT_PANEL_OPEN_DURATION = 220;
const ATTACHMENT_PANEL_CLOSE_DURATION = 180;
// 中文注释：内容区短按关闭附件区时，允许识别为点击的最长按压时长。
const CONTENT_TAP_MAX_DURATION = 220;
// 中文注释：内容区短按关闭附件区时，允许识别为点击的最大位移。
const CONTENT_TAP_MAX_MOVE = 8;
// 中文注释：选择模式下左侧 checkbox 的槽位宽度。
const SELECTION_CHECKBOX_SLOT_WIDTH = 30;

// 中文注释：v3 onScroll 统一使用 RN 原生 NativeSyntheticEvent<NativeScrollEvent> 类型，
// 不再需要 v2 的 Reanimated 扁平化事件兼容。
type LegendScrollEvent = NativeSyntheticEvent<NativeScrollEvent>;

interface Chat2Props {
  appId: string;
}

export default function Chat2({ appId }: Chat2Props) {
  const { t } = useTranslation();
  const { tab } = useTab();
  const { width } = useWindowDimensions();
  const colors = useColors();
  const styles = useMemo(() => makeStyles(colors), [colors]);
  const layoutTokens = useMemo(() => getHostLayoutTokens(width), [width]);

  // ==================== Store ====================

  const messageIds = useMessageStore(
    useCallback(
      (state) => state.messageIds[tab.id] || EMPTY_MESSAGE_IDS,
      [tab.id],
    ),
  );
  const messagesById = useMessageStore(
    useCallback(
      (state) => state.messageById[tab.id] || EMPTY_MESSAGES_BY_ID,
      [tab.id],
    ),
  );
  const appendOldMessages = useMessageStore((state) => state.appendOldMessages);
  const editingMessageId = useInputStore(
    (state) => state.inputs[tab.id]?.editingMessageId,
  );
  const clearInput = useInputStore((state) => state.clearInput);
  // 中文注释：选择模式状态，'none' | 'delete' | 'share'，用于底部操作栏渲染判断。
  const selectionMode = useMessageStore((state) => state.selectionMode);
  // 中文注释：退出选择模式，清空所有选中项。
  const exitSelectionMode = useMessageStore((state) => state.exitSelectionMode);
  // 中文注释：app 信息，用于反馈弹窗判断 app 是否存活。
  const appInfo = useAppStore((state) => state.apps[appId]);

  // ==================== State ====================

  const [hasLoadedInitialMessages, setHasLoadedInitialMessages] = useState(false);
  const [isLoadingMore, setIsLoadingMore] = useState(false);
  const [hasMore, setHasMore] = useState(true);
  // 中文注释：反馈弹窗当前对应的消息 ID，非空时打开弹窗。
  const [feedbackMessageId, setFeedbackMessageId] = useState<string | null>(null);
  // 中文注释：滚动位置距底部超过阈值时为 true，控制"滚动到底部"按钮的显示。
  const [isScrollToBottomVisible, setIsScrollToBottomVisible] = useState(false);

  // ==================== Refs ====================

  const listRef = useRef<LegendListRef>(null);
  // 中文注释：标记当前滚动位置是否贴近底部。
  const isNearBottomRef = useRef(true);
  // 中文注释：缓存每条消息的行高，用于动态调整估算行高。
  const rowHeightCacheRef = useRef<Map<string, number>>(new Map());
  // 中文注释：标记首屏消息是否已做过贴底滚动，避免重复。
  const hasAppliedInitialBottomLockRef = useRef(false);
  // 中文注释：首屏贴底稳定期内暂时禁止更新"滚动到底部"按钮，
  // 避免估算高度阶段的 onScroll 中间态把已在底部的列表误判为"离底部较远"。
  const suppressScrollToBottomVisibilityRef = useRef(true);
  // 中文注释：记录内容区触摸起点，用于区分短按和拖动来决定是否关闭附件面板。
  const contentTouchStartRef = useRef<{
    time: number;
    x: number;
    y: number;
    moved: boolean;
  } | null>(null);
  // 中文注释：Toast 引用，用于复制/删除/分享等操作的提示。
  const toastRef = useRef<ToastRef>(null);

  // ==================== Hooks ====================

  // 中文注释：聊天页底部结构统一从 hook 读取，避免列表、输入栏、空态和附件面板各自重复计算平台补偿。
  const {
    keyboardInset,
    isKeyboardVisible,
    listKeyboardPadding,
    tabletKeyboardMarginBottom,
    emptyStateBottomCompensation,
    attachmentPanelHeight,
  } = useChatBottomInsets(layoutTokens.isTablet);

  // 中文注释：删除/分享选择操作 hook，封装删除确认弹窗、分享预览数据构建、延后退出选择模式。
  const {
    handleDeletePress,
    handleShareConfirm,
    shareMessages,
    setShareMessages,
  } = useChatSelectionActions({ tabId: tab.id, toastRef });

  // 中文注释：气泡操作 hook，封装编辑、复制、反馈等操作。
  const { handleEdit, handleCopy, handleFeedback } = useChatBubbleActions({
    tabId: tab.id,
    appId,
    toastRef,
    setFeedbackMessageId,
  });

  // ==================== Keyboard ====================

  const isEmptyState =
    hasLoadedInitialMessages && messageIds.length === 0 && !isLoadingMore;

  // ==================== 附件面板 ====================

  // 中文注释：附件面板控制器，管理面板开关状态、附件选中/移除、各类选取操作。
  const attachmentPanelController = usePhoneAttachmentPanelController(
    appId,
    tab.id,
  );

  // 中文注释：附件面板展开/收起时，内容区整体上移/回位的动画值。
  const attachmentPanelTranslateY = useRef(new Animated.Value(0)).current;

  // 中文注释：Phone 模式下，附件面板展开时整体内容上移，收起时回位。
  // Pad 模式不展示附件面板，不需要动画。
  useEffect(() => {
    if (layoutTokens.isTablet) {
      attachmentPanelTranslateY.setValue(0);
      return;
    }

    Animated.timing(attachmentPanelTranslateY, {
      toValue: attachmentPanelController.isAttachmentPanelOpen
        ? -attachmentPanelHeight
        : 0,
      duration: attachmentPanelController.isAttachmentPanelOpen
        ? ATTACHMENT_PANEL_OPEN_DURATION
        : ATTACHMENT_PANEL_CLOSE_DURATION,
      useNativeDriver: true,
    }).start();
  }, [
    attachmentPanelTranslateY,
    attachmentPanelHeight,
    layoutTokens.isTablet,
    attachmentPanelController.isAttachmentPanelOpen,
  ]);

  // ==================== 初始化生命周期 ====================

  // 中文注释：朗读控制器，注册原生 TTS 事件监听，仅在 iOS 生效。
  // ChatBubble 内部通过 useReadAloudPlayback 驱动朗读交互，
  // 此处调用确保原生事件回调在整个 Chat2 生命周期内生效。
  useReadAloudPlaybackController();

  // 中文注释：聊天页初始化生命周期 hook，统一管理四个职责：
  // 1. 加载首屏消息 + 强制最后一条消息完成态
  // 2. App 信息回退（接口失败时从 SQLite 恢复）
  // 3. 同步书签状态到 Native tab 收藏标识
  // 4. 卸载清理（非 DEV 环境下清理消息、输入、映射、未读等 store 数据）
  useChatInit({
    tab,
    appId,
    initialMessageLimit: PAGE_SIZE,
    onInitialMessagesLoaded: useCallback((data: IMessage[]) => {
      setHasMore(data.length === PAGE_SIZE);
      setHasLoadedInitialMessages(true);
    }, []),
  });

  // 中文注释：触顶时加载更早的历史消息，追加到 store 头部。
  const handleStartReached = useCallback(async (_info: { distanceFromStart: number }) => {
    if (isLoadingMore || !hasMore || messageIds.length === 0) return;

    setIsLoadingMore(true);
    try {
      const data = await MessageRepository.getAllMessages(
        tab.id,
        PAGE_SIZE,
        messageIds.length,
      );
      if (data.length > 0) {
        appendOldMessages(tab.id, data);
      }
      setHasMore(data.length === PAGE_SIZE);
    } catch (error) {
      console.error('[Chat2] Failed to load more messages:', error);
    } finally {
      setIsLoadingMore(false);
    }
  }, [hasMore, isLoadingMore, messageIds.length, tab.id, appendOldMessages]);

  // ==================== 滚动追踪 ====================

  // 中文注释：实时追踪滚动位置，判断是否贴近底部，用于键盘弹起和异步内容撑高时自动贴底；
  // 同时更新"滚动到底部"按钮的可见性。
  const handleScroll = useCallback((event: LegendScrollEvent) => {
    const { contentOffset, contentSize, layoutMeasurement } = event.nativeEvent;
    const distanceToBottom =
      contentSize.height - (contentOffset.y + layoutMeasurement.height);
    isNearBottomRef.current = distanceToBottom <= BOTTOM_THRESHOLD;
    // 中文注释：首屏贴底尚未稳定前，先跳过按钮显隐更新；
    // 否则 initialScrollAtEnd/scrollToEnd 基于估高触发的中间态 onScroll
    // 会让按钮在首次加载时错误出现。
    if (suppressScrollToBottomVisibilityRef.current) return;
    setIsScrollToBottomVisible(distanceToBottom > SCROLL_TO_BOTTOM_THRESHOLD);
  }, []);

  // 中文注释：用户开始拖拽滚动时，如果附件面板处于展开状态则自动收起，
  // 避免列表滚动和底部附件面板并存的不协调体验。
  const handleScrollBeginDrag = useCallback(() => {
    if (!layoutTokens.isTablet && attachmentPanelController.isAttachmentPanelOpen) {
      attachmentPanelController.closeAttachmentPanel();
    }
  }, [layoutTokens.isTablet, attachmentPanelController]);

  // ==================== 键盘处理 ====================

  // 中文注释：键盘弹起时，贴近底部则 scrollToEnd 贴底。
  // iOS：键盘 overlay，容器不变，scrollToEnd 补偿贴底。
  // Android：adjustResize 缩容器，KeyboardProvider 包裹后 LegendList 的 maintainScrollAtEnd
  // 应在 handleLayout 时自动贴底，这里兜底补一次。
  useEffect(() => {
    if (!isKeyboardVisible || !isNearBottomRef.current) return;

    requestAnimationFrame(() => {
      void listRef.current?.scrollToEnd({ animated: false });
    });
  }, [isKeyboardVisible, keyboardInset]);

  // 中文注释：消息行高度变化回调，缓存高度并更新估算值；
  // 贴近底部时重新贴底，避免异步内容撑高后留白。
  const handleRowHeightChange = useCallback(
    (messageId: string, height: number) => {
      const prev = rowHeightCacheRef.current.get(messageId) ?? 0;
      if (Math.abs(prev - height) <= 1) return;

      rowHeightCacheRef.current.set(messageId, height);

      if (isNearBottomRef.current) {
        requestAnimationFrame(() => {
          void listRef.current?.scrollToEnd({ animated: false });
        });
      }
    },
    [],
  );

  // 中文注释：逐条返回消息行估高；已测量的行直接复用真实高度，未测量的行按回答内容结构保守估算。
  const getEstimatedItemSize = useCallback(
    (messageId: string) => {
      const measuredHeight = rowHeightCacheRef.current.get(messageId);
      if (measuredHeight && measuredHeight > 0) {
        return measuredHeight;
      }

      const message = messagesById[messageId];
      if (!message) {
        return INITIAL_ESTIMATED_ITEM_SIZE;
      }

      return estimateMessageRowHeight(
        message,
        messageIds[messageIds.length - 1] === messageId,
        layoutTokens.isTablet,
      );
    },
    [layoutTokens.isTablet, messageIds, messagesById],
  );

  // ==================== 生命周期 ====================

  // 中文注释：tab 切换时重置所有滚动和估算状态。
  useEffect(() => {
    setHasLoadedInitialMessages(false);
    hasAppliedInitialBottomLockRef.current = false;
    isNearBottomRef.current = true;
    suppressScrollToBottomVisibilityRef.current = true;
    setIsScrollToBottomVisible(false);
    rowHeightCacheRef.current.clear();
  }, [tab.id]);

  // 中文注释：首屏消息到位后，强制保持 isNearBottomRef = true，
  // 让后续 handleRowHeightChange 持续贴底，直到真实布局稳定。
  // 原因：scrollToEnd 基于估算高度计算终点，onScroll 可能误判为"不在底部"，
  // 导致行高上报后不再触发贴底，首屏消息距输入框太近。
  useEffect(() => {
    if (!hasLoadedInitialMessages || hasAppliedInitialBottomLockRef.current) {
      return;
    }
    hasAppliedInitialBottomLockRef.current = true;

    // 中文注释：初始阶段强制标记为贴近底部，确保行高变化时能持续贴底。
    isNearBottomRef.current = true;

    requestAnimationFrame(() => {
      void listRef.current?.scrollToEnd({ animated: false });
    });

    // 中文注释：延迟 500ms 再补一次贴底，等大部分行完成真实布局后修正位置。
    const settleTimer = setTimeout(() => {
      isNearBottomRef.current = true;
      suppressScrollToBottomVisibilityRef.current = false;
      setIsScrollToBottomVisible(false);
      void listRef.current?.scrollToEnd({ animated: false });
    }, 500);

    return () => {
      clearTimeout(settleTimer);
    };
  }, [hasLoadedInitialMessages, messageIds.length]);

  // ==================== 编辑态 ====================

  // 中文注释：退出编辑模式，清空输入框文本、附件和编辑消息 ID。
  const handleCancelEdit = useCallback(() => {
    clearInput(tab.id);
  }, [tab.id, clearInput]);

  // ==================== 附件面板交互 ====================

  // 中文注释：点击"滚动到底部"按钮，平滑滚动到列表底部。
  const handleScrollToBottom = useCallback(() => {
    void listRef.current?.scrollToEnd({ animated: true });
  }, []);

  // 中文注释：记录内容区触摸起点，用于区分短按和拖动。
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

  // 中文注释：手指移动时更新触摸状态，避免把拖动误判成短按点击。
  const handleContentTouchMove = useCallback(
    (event: GestureResponderEvent) => {
      const touchState = contentTouchStartRef.current;
      if (!touchState || touchState.moved) return;

      const { pageX, pageY } = event.nativeEvent;
      const movedX = Math.abs(pageX - touchState.x);
      const movedY = Math.abs(pageY - touchState.y);

      if (movedX > CONTENT_TAP_MAX_MOVE || movedY > CONTENT_TAP_MAX_MOVE) {
        touchState.moved = true;
      }
    },
    [],
  );

  // 中文注释：内容区触摸结束时，只有短按动作才收起附件面板，
  // 长按和拖动都不触发，避免误伤消息长按能力。
  const handleContentTouchEnd = useCallback(() => {
    const touchState = contentTouchStartRef.current;
    contentTouchStartRef.current = null;

    if (!touchState) return;

    const duration = Date.now() - touchState.time;

    if (
      !layoutTokens.isTablet &&
      attachmentPanelController.isAttachmentPanelOpen &&
      !touchState.moved &&
      duration <= CONTENT_TAP_MAX_DURATION
    ) {
      attachmentPanelController.closeAttachmentPanel();
    }
  }, [layoutTokens.isTablet, attachmentPanelController]);

  // ==================== 渲染 ====================

  const renderItem = useCallback(
    ({ item: messageId }: { item: string; index: number }) => (
      <MessageRow
        tabId={tab.id}
        appId={appId}
        messageId={messageId}
        layoutTokens={layoutTokens}
        onEdit={handleEdit}
        onCopy={handleCopy}
        onFeedback={handleFeedback}
        onRowHeightChange={handleRowHeightChange}
        styles={styles}
      />
    ),
    [
      appId,
      handleCopy,
      handleEdit,
      handleFeedback,
      handleRowHeightChange,
      layoutTokens,
      styles,
      tab.id,
    ],
  );

  // 中文注释：空态组件，带底部补偿，用于 LegendList 的 ListEmptyComponent。
  const listEmptyComponent = useMemo(
    () => (
      <View
        style={{
          flex: 1,
          paddingBottom: emptyStateBottomCompensation,
        }}
      >
        <ChatEmpty />
      </View>
    ),
    [emptyStateBottomCompensation],
  );

  // 中文注释：消息列表区域，包含加载态和 LegendList。
  // 中文注释：始终渲染 LegendList，用 ListEmptyComponent 处理空态，
  // 避免 LegendList 卸载/重新挂载时原生 ScrollView 复用导致布局异常。
  const contentNode = (
    <View
      style={styles.listContainer}
      onTouchStart={handleContentTouchStart}
      onTouchMove={handleContentTouchMove}
      onTouchEnd={handleContentTouchEnd}
    >
      {!hasLoadedInitialMessages ? (
        <View style={styles.loadingContainer}>
          <ActivityIndicator size="small" color={colors.textHeading} />
        </View>
      ) : (
        <LegendList
          ref={listRef}
          data={messageIds}
          renderItem={renderItem}
          keyExtractor={(item) => item}
          style={styles.legendList}
          contentContainerStyle={[
            styles.chatContent,
            {
              paddingTop: layoutTokens.isTablet
                ? 12
                : CHAT_HEADER_HEIGHT + 12,
              paddingBottom:
                layoutTokens.chatListBottomPadding +
                24 + // 中文注释：补偿输入区 marginTop:-24 的向上重叠，确保消息与输入框有足够间距。
                listKeyboardPadding,
            },
          ]}
          ListHeaderComponent={
            isLoadingMore ? (
              <View style={styles.loadMoreHeader}>
                <ActivityIndicator size="small" color={colors.textTertiary} />
              </View>
            ) : null
          }
          ListEmptyComponent={listEmptyComponent}
          initialScrollAtEnd
          // 中文注释：尾部新增消息时只让 maintainScrollAtEnd 接管贴底，
          // 避免与 maintainVisibleContentPosition 的 data 锚定同时改 scroll 造成超长消息错位。
          maintainScrollAtEnd={{
            animated: false,
            on: {
              dataChange: true,
              itemLayout: true,
              layout: true,
            },
          }}
          // 中文注释：保留 size 稳定能力处理滚动中的高度变化，但关闭 data 锚定，
          // 避免在"回答已完成后再追加新消息"时对超长上一条消息做错误位置恢复。
          maintainVisibleContentPosition={{ size: true }}
          alignItemsAtEnd
          // 中文注释：当前消息行包含本地布局缓存和复杂交互状态，先显式关闭 recycleItems，
          // 避免 LegendList v3 beta 复用 row 时把旧实例状态带到新消息上。
          recycleItems={false}
          drawDistance={350}
          onStartReached={handleStartReached}
          onStartReachedThreshold={0.5}
          onScroll={handleScroll}
          onScrollBeginDrag={handleScrollBeginDrag}
          keyboardShouldPersistTaps="never"
          keyboardDismissMode="none"
          scrollEventThrottle={16}
          estimatedItemSize={INITIAL_ESTIMATED_ITEM_SIZE}
          // @ts-ignore
          getEstimatedItemSize={getEstimatedItemSize}
        />
      )}
      {!!editingMessageId && (
        <Pressable
          style={styles.editModeOverlay}
          onPress={handleCancelEdit}
        />
      )}
      {/* 中文注释：距底部超过阈值时显示"滚动到底部"按钮，选择模式下隐藏避免与底部操作栏冲突。 */}
      {selectionMode === 'none' && (
        <View style={styles.scrollToBottomContainer} pointerEvents="box-none">
          <ScrollToBottomButton
            visible={isScrollToBottomVisible}
            onPress={handleScrollToBottom}
          />
        </View>
      )}
    </View>
  );

  // 中文注释：输入框区域，Phone 模式传入附件面板控制器。
  const inputNode = (
    <KeyboardStickyView
      // 中文注释：iOS Phone 用 KeyboardStickyView 跟随键盘吸附；
      // Android Phone 由 adjustResize 缩容器，输入框自然在键盘上方，不需要额外推升；
      // Pad 用 marginBottom 推升。
      enabled={Platform.OS === 'ios' && !layoutTokens.isTablet}
      style={[
        styles.composerSlot,
        { marginBottom: tabletKeyboardMarginBottom },
      ]}
    >
      <View style={styles.inputInner}>
        <ActrInput
          appId={appId}
          mobileAttachmentController={
            !layoutTokens.isTablet ? attachmentPanelController : undefined
          }
        />
      </View>
    </KeyboardStickyView>
  );

  // ==================== Phone 布局 ====================

  // 中文注释：Phone 模式下，内容区 + 输入框 + 附件面板三个兄弟一起 translateY，
  // 展开附件面板时整体上移，收起时回位。
  if (!layoutTokens.isTablet) {
    return (
      <View style={styles.root}>
        <Animated.View
          style={[
            styles.phoneContentMotion,
            { transform: [{ translateY: attachmentPanelTranslateY }] },
          ]}
        >
          {contentNode}
        </Animated.View>

        <Animated.View
          style={[
            styles.phoneInputMotion,
            { transform: [{ translateY: attachmentPanelTranslateY }] },
          ]}
        >
          {inputNode}
        </Animated.View>

        <Animated.View
          pointerEvents={
            attachmentPanelController.isAttachmentPanelOpen ? 'auto' : 'none'
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
            expanded={attachmentPanelController.isAttachmentPanelOpen}
            showSearchEntry={attachmentPanelController.showSearchEntry}
            showNewlineEntry={attachmentPanelController.showNewlineEntry}
            isWebSearchActive={attachmentPanelController.isWebSearchActive}
            isEnterForNewLine={attachmentPanelController.isEnterForNewLine}
            onPickCamera={attachmentPanelController.takePhoto}
            onPickImage={attachmentPanelController.pickImage}
            onPickFile={attachmentPanelController.pickDocument}
            onPickLocation={attachmentPanelController.pickLocation}
            onToggleWebSearch={attachmentPanelController.toggleWebSearch}
            onToggleEnterForNewLine={attachmentPanelController.toggleEnterForNewLine}
          />
        </Animated.View>

        {/* 中文注释：选择模式底部操作栏，根据模式渲染删除或分享确认按钮。 */}
        {selectionMode === 'delete' && (
          <View style={styles.selectionActionBar}>
            <DeleteSelectionBar
              onPress={handleDeletePress}
              onCancel={exitSelectionMode}
            />
          </View>
        )}
        {selectionMode === 'share' && (
          <View style={styles.selectionActionBar}>
            <ShareConfirmButton
              onPress={handleShareConfirm}
              onCancel={exitSelectionMode}
            />
          </View>
        )}

        <LocationPicker
          visible={attachmentPanelController.showMapModal}
          onClose={() => attachmentPanelController.setShowMapModal(false)}
          // 中文注释：Phone 端附件定位选择器贴底展示，和底部附件面板保持同一交互方向。
          presentation='bottom'
          onSelect={(data) => {
            attachmentPanelController.handleConfirmLocation(data);
            attachmentPanelController.setShowMapModal(false);
          }}
        />

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

        <Toast ref={toastRef} />
      </View>
    );
  }

  // ==================== Pad 布局 ====================

  // 中文注释：Pad 模式无附件面板，直接排列内容区和输入框。
  return (
    <View style={styles.root}>
      {contentNode}
      {inputNode}

      {/* 中文注释：选择模式底部操作栏，根据模式渲染删除或分享确认按钮。 */}
      {selectionMode === 'delete' && (
        <View style={styles.selectionActionBar}>
          <DeleteSelectionBar
            onPress={handleDeletePress}
            onCancel={exitSelectionMode}
          />
        </View>
      )}
      {selectionMode === 'share' && (
        <View style={styles.selectionActionBar}>
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
          toastRef.current?.show('actr.chat.shareSaveSuccess');
        }}
      />

      <Toast ref={toastRef} />
    </View>
  );
}

// ==================== DeleteSelectionBar ====================

// 中文注释：删除确认栏组件，显示已选中数量和删除/取消按钮。
// 独立提取为 React.memo，避免父组件重渲染时不必要的刷新。
const DeleteSelectionBar = React.memo(function DeleteSelectionBar({
  onPress,
  onCancel,
}: {
  onPress: () => void;
  onCancel: () => void;
}) {
  // 中文注释：直接从 store 订阅选中项计数，不受父组件渲染频率影响。
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

// ==================== MessageRow ====================

interface MessageRowProps {
  tabId: string;
  appId: string;
  messageId: string;
  layoutTokens: ReturnType<typeof getHostLayoutTokens>;
  onEdit: (msg: IMessage) => void;
  onCopy: (text: string) => void;
  onFeedback: (msgId: string) => void;
  onRowHeightChange: (messageId: string, height: number) => void;
  styles: ReturnType<typeof makeStyles>;
}

// 中文注释：单条消息行组件，从 store 读取消息数据并渲染问/答气泡。
// 用 React.memo 避免无关消息更新时重渲染。
// 选择模式下，每个气泡左侧显示 Checkbox，气泡点击切换该部分选中状态。
const MessageRow = React.memo(function MessageRow({
  tabId,
  appId,
  messageId,
  layoutTokens,
  onEdit,
  onCopy,
  onFeedback,
  onRowHeightChange,
  styles,
}: MessageRowProps) {
  const item = useMessageStore(
    useCallback(
      (state) => state.messageById[tabId]?.[messageId],
      [messageId, tabId],
    ),
  );
  const isLatestInStore = useMessageStore(
    useCallback(
      (state) => {
        const ids = state.messageIds[tabId];
        return ids ? ids[ids.length - 1] === messageId : false;
      },
      [tabId, messageId],
    ),
  );
  // 中文注释：选择模式状态，直接从 store 读取，Zustand 细粒度订阅驱动重渲染，
  // 绕过 LegendList 的 renderItem 缓存，确保 Checkbox 即时显示/隐藏。
  const selectionMode = useMessageStore((state) => state.selectionMode);
  // 中文注释：切换某条消息某个部分的选中状态，直接从 store 读取，
  // 避免通过 prop 传递导致 renderItem 依赖项膨胀。
  const toggleSelection = useMessageStore((state) => state.toggleSelection);
  // 中文注释：当前消息各部分的选中状态，用于 Checkbox 勾选。
  const selectedParts = useMessageStore(
    useCallback(
      (state) => state.selectedItems[messageId] || EMPTY_SELECTED_PARTS,
      [messageId],
    ),
  );

  const lastReportedHeightRef = useRef(0);

  // 中文注释：行布局变化时上报高度，用于动态调整估算行高。
  const handleRowLayout = useCallback(
    (event: LayoutChangeEvent) => {
      const nextHeight = Math.round(event.nativeEvent.layout.height);
      if (nextHeight <= 0) return;
      if (Math.abs(lastReportedHeightRef.current - nextHeight) <= 1) return;
      lastReportedHeightRef.current = nextHeight;
      onRowHeightChange(messageId, nextHeight);
    },
    [messageId, onRowHeightChange],
  );

  if (!item) return null;

  const isAnswerLoading =
    isLatestInStore &&
    (item.progress === Progress.pending ||
      item.progress === Progress.processing);

  const isSelectionMode = selectionMode !== 'none';

  // 中文注释：渲染单个气泡（含 checkbox），选择模式下左侧显示 Checkbox，
  // 非选择模式下正常排列。每个气泡对应一个 MessagePart（attachments/question/answer）。
  // isQuestion 控制 bubbleContentWrapper 的对齐方向：question 右对齐，answer 左对齐。
  const renderBubbleWithCheckbox = (
    part: MessagePart,
    isQuestion: boolean,
    bubble: React.ReactNode,
  ) => (
    <View style={styles.bubbleWithCheckbox}>
      {isSelectionMode ? (
        <View style={styles.checkboxContainer}>
          <Checkbox
            checked={selectedParts.has(part)}
            onPress={() => toggleSelection(item.message_id, part)}
          />
        </View>
      ) : null}
      <View
        style={[
          styles.bubbleContentWrapper,
          isQuestion
            ? styles.bubbleAlignEnd
            : styles.bubbleAlignStart,
        ]}
      >
        {bubble}
      </View>
    </View>
  );

  return (
    <View
      style={[
        styles.messageListWrapper,
        styles.messageListWrapperGuard,
        {
          maxWidth: layoutTokens.contentMaxWidth,
          paddingHorizontal: layoutTokens.messageHorizontalPadding,
        },
      ]}
      onLayout={handleRowLayout}
    >
      <View style={[styles.messageGroup, styles.messageGroupGuard]}>
        {item.attachments &&
          renderBubbleWithCheckbox(
            'attachments',
            true,
            <ChatBubble
              type="question"
              attachments={item.attachments}
              containerStyle={{ marginBottom: 4 }}
              onEdit={
                isLatestInStore && selectionMode === 'none' && !item.is_edited
                  ? () => onEdit(item)
                  : undefined
              }
              onCopy={() => onCopy(item.question_content)}
              onFeedback={() => onFeedback(item.message_id)}
              msgId={item.message_id}
              tabId={tabId}
              appId={appId}
              selectionMode={selectionMode}
              onToggleSelection={() =>
                toggleSelection(item.message_id, 'attachments')
              }
            />,
          )}

        {(item.question_content || item.question_type === 'voice') &&
          renderBubbleWithCheckbox(
            'question',
            true,
            <ChatBubble
              type="question"
              msgId={item.message_id}
              tabId={tabId}
              appId={appId}
              content={item.question_content}
              voicePayload={item.question_voice_payload}
              selectionMode={selectionMode}
              onEdit={
                isLatestInStore &&
                selectionMode === 'none' &&
                item.question_type !== 'voice' &&
                !item.is_edited
                  ? () => onEdit(item)
                  : undefined
              }
              onCopy={
                item.question_type !== 'voice'
                  ? () => onCopy(item.question_content)
                  : undefined
              }
              onFeedback={() => onFeedback(item.message_id)}
              onToggleSelection={() =>
                toggleSelection(item.message_id, 'question')
              }
            />,
          )}

        {(item.answer_content ||
          item.answer_type === 'voice' ||
          isAnswerLoading) &&
          renderBubbleWithCheckbox(
            'answer',
            false,
            <ChatBubble
              type="answer"
              msgId={item.message_id}
              tabId={tabId}
              appId={appId}
              content={item.answer_content}
              voicePayload={item.answer_voice_payload}
              isLoading={isAnswerLoading}
              progress={item.progress}
              isLatest={isLatestInStore}
              selectionMode={selectionMode}
              onCopy={
                item.answer_type !== 'voice'
                  ? () => onCopy(item.answer_content)
                  : undefined
              }
              onFeedback={() => onFeedback(item.message_id)}
              onToggleSelection={() =>
                toggleSelection(item.message_id, 'answer')
              }
            />,
          )}
      </View>
    </View>
  );
});

// ==================== 样式 ====================

const makeStyles = (colors: ColorTokens) =>
  StyleSheet.create({
    root: {
      flex: 1,
      backgroundColor: colors.bgChatPage,
    },
    listContainer: {
      flex: 1,
      width: '100%',
    },
    legendList: {
      flex: 1,
      width: '100%',
    },
    chatContent: {
      width: '100%',
    },
    composerSlot: {
      width: '100%',
      zIndex: 100,
    },
    inputInner: {
      width: '100%',
      // 中文注释：输入区向上重叠 24px，让渐变遮罩与内容区无缝衔接；
      // 列表 paddingBottom 已补偿此值，确保消息与输入框之间有足够间距。
      marginTop: -24,
    },
    loadingContainer: {
      flex: 1,
      alignItems: 'center',
      justifyContent: 'center',
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
    // 中文注释："滚动到底部"按钮的定位容器，居底部居中，不拦截列表滚动事件。
    scrollToBottomContainer: {
      position: 'absolute',
      left: 0,
      right: 0,
      bottom: spacing.lg + 16,
      alignItems: 'center',
      zIndex: 40,
    },
    loadMoreHeader: {
      paddingVertical: spacing.lg,
      alignItems: 'center',
    },
    messageListWrapper: {
      width: '100%',
      alignSelf: 'center',
    },
    // 中文注释：必须保留这一层透明边框；移除后超长表格消息在列表里会出现气泡重叠，当前原因未定位。
    messageListWrapperGuard: {
      borderWidth: 1,
      borderColor: 'transparent',
      backgroundColor: 'transparent',
    },
    messageGroup: {
      marginBottom: spacing.md,
    },
    // 中文注释：必须保留这一层透明边框；单独去掉后重叠问题会复现，当前原因未定位。
    messageGroupGuard: {
      borderWidth: 1,
      borderColor: 'transparent',
      backgroundColor: 'transparent',
    },
    // 中文注释：选择模式下每个气泡行的布局，左侧 checkbox + 右侧气泡。
    bubbleWithCheckbox: {
      flexDirection: 'row',
      alignItems: 'flex-start',
      marginBottom: spacing.xxs,
      width: '100%',
    },
    // 中文注释：checkbox 槽位，固定宽度，与气泡顶部对齐。
    checkboxContainer: {
      width: SELECTION_CHECKBOX_SLOT_WIDTH,
      paddingTop: 14,
      alignItems: 'flex-start',
    },
    // 中文注释：气泡内容区域，flex:1 占满 checkbox 以外的剩余宽度，默认不设 alignItems。
    bubbleContentWrapper: {
      flex: 1,
      minWidth: 0,
    },
    // 中文注释：question/attachments 气泡右对齐。
    bubbleAlignEnd: {
      alignItems: 'flex-end',
    },
    // 中文注释：answer 气泡左对齐。
    bubbleAlignStart: {
      alignItems: 'flex-start',
    },
    // 中文注释：Phone 模式下内容区的动画容器，flex:1 撑满剩余空间。
    phoneContentMotion: {
      flex: 1,
    },
    // 中文注释：Phone 模式下输入框的动画容器，宽度撑满。
    phoneInputMotion: {
      width: '100%',
    },
    // 中文注释：附件面板浮层，默认隐藏在屏幕底部，展开时随 translateY 上移露出。
    attachmentPanelOverlay: {
      position: 'absolute',
      left: 0,
      right: 0,
      zIndex: 90,
    },
    // 中文注释：选择模式底部操作栏容器，固定在底部。
    selectionActionBar: {
      position: 'absolute',
      left: 0,
      right: 0,
      bottom: 0,
      alignItems: 'center',
      justifyContent: 'center',
      zIndex: 1001,
      pointerEvents: 'box-none',
    },
  });

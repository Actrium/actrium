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
import { useSettingsStore } from '../../../store/useSettingsStore';
import { DeleteButton } from '../shared/DeleteButton';
import { ShareConfirmButton } from '../shared/ShareConfirmButton';
import { Checkbox } from '../shared/Checkbox';
import { ScrollToBottomButton } from '../shared/ScrollToBottomButton';
import { Toast, ToastRef } from '../shared/Toast';
import { FeedbackFormModal } from './FeedbackFormModal';
import ChatSharePreviewModal from './share/ChatSharePreviewModal';
import { estimateMessageRowHeight } from './estimateMessageRowHeight';

// ==================== Constant ====================

// The number of messages per page loaded.
const PAGE_SIZE = 20;
// Empty array reuse, avoid selector Returns the new reference triggers the retrofit each time.
const EMPTY_MESSAGE_IDS: string[] = [];
// Empty Map Reuse, Avoid selector Returns a new object to trigger retrofitting each time.
const EMPTY_MESSAGES_BY_ID: Record<string, IMessage> = {};
// Select part empty to reuse, avoid each MessageRow Return New Set Reference triggers retrofit.
const EMPTY_SELECTED_PARTS = new Set<'attachments' | 'question' | 'answer'>();
// is considered to be less than that value at the bottom"Close to the bottom.", automatically bottoms when the keyboard is loaded or the stept content is high.
const BOTTOM_THRESHOLD = 80;
// Displays when the distance exceeds that value at the bottom"Scroll Bottom"button.
const SCROLL_TO_BOTTOM_THRESHOLD = 100;
// The button is displayed at a distance above the bottom during the first screen loading cooling period to prevent the layout correction from being triggered by error.
const SCROLL_TO_BOTTOM_COOLDOWN_THRESHOLD = 300;
// The cooling period (ms) after the first screen is loaded, during which the button is raised to show the threshold.
const SCROLL_TO_BOTTOM_COOLDOWN_MS = 1500;
// list, only if the article-by-article overvaluation function is not hit.
const INITIAL_ESTIMATED_ITEM_SIZE = 160;
// Attachment Panel Expand/(b) The length of the time to close animated (ms).
const ATTACHMENT_PANEL_OPEN_DURATION = 220;
const ATTACHMENT_PANEL_CLOSE_DURATION = 180;
// The maximum pressure time allowed to be recognized for a content area short of the attachment area closes.
const CONTENT_TAP_MAX_DURATION = 220;
// The maximum offset of the click is allowed when content area is short-clicked to close attachment area.
const CONTENT_TAP_MAX_MOVE = 8;
// Left-Left Selection Mode checkbox . The width of the slot.
const SELECTION_CHECKBOX_SLOT_WIDTH = 30;

// v3 onScroll Unique use RN Native NativeSyntheticEvent<NativeScrollEvent> Type,
// No more. v2 It's... Reanimated The flattening event compatible.
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
  // Select mode state,'none' | 'delete' | 'share', for bottom line rendering.
  const selectionMode = useMessageStore((state) => state.selectionMode);
  // Quits the selection mode, emptys all selections.
  const exitSelectionMode = useMessageStore((state) => state.exitSelectionMode);
  // app Information for feedback on bullet window judgement app Whether or not to survive.
  const appInfo = useAppStore((state) => state.apps[appId]);

  // ==================== State ====================

  const [hasLoadedInitialMessages, setHasLoadedInitialMessages] = useState(false);
  const [isLoadingMore, setIsLoadingMore] = useState(false);
  const [hasMore, setHasMore] = useState(true);
  // Feedback from current information on bullet windows ID, open the bullet window when it's not empty.
  const [feedbackMessageId, setFeedbackMessageId] = useState<string | null>(null);
  // Scroll position distance above threshold trueControl"Scroll Bottom"button.
  const [isScrollToBottomVisible, setIsScrollToBottomVisible] = useState(false);

  // ==================== Refs ====================

  const listRef = useRef<LegendListRef>(null);
  // Marks whether the current scroll position is close to the bottom.
  const isNearBottomRef = useRef(true);
  // Caches the height of each message to be used for dynamically adjusted estimates of the height of the line.
  const rowHeightCacheRef = useRef<Map<string, number>>(new Map());
  // Marks whether the front screen message has been rolled against the background to avoid repetition.
  const hasAppliedInitialBottomLockRef = useRef(false);
  // Temporary ban on updates during the first screen bottom-stabilization period"Scroll Bottom"button,
  // Avoiding the estimation of heights onScroll Intermediate misjudges the list already at the bottom as"Far from the bottom.".
  const suppressScrollToBottomVisibilityRef = useRef(true);
  // The cooling period is stamped with a maximum cooling time after the first screen is loaded, during which the button is raised to show the threshold,
  // Prevention of layout modifications during onScroll Middle-state error trigger button.
  const scrollToBottomCooldownUntilRef = useRef(0);
  // Records the starting point of the touch of the content area to distinguish between short-press and drag to decide whether to close the attachment panel.
  const contentTouchStartRef = useRef<{
    time: number;
    x: number;
    y: number;
    moved: boolean;
  } | null>(null);
  // Toast References, for copying/Delete/Shares the hints of the operation.
  const toastRef = useRef<ToastRef>(null);

  // ==================== Hooks ====================

  // & Bottom Structure of Chat Pages hook Read to avoid double counting of Platform compensations for lists, input columns, empty and attachment panels.
  const {
    keyboardInset,
    isKeyboardVisible,
    listKeyboardPadding,
    tabletKeyboardMarginBottom,
    emptyStateBottomCompensation,
    attachmentPanelHeight,
  } = useChatBottomInsets(layoutTokens.isTablet);

  // Delete/Share Selection Operations hook, remove confirmation windows, share preview data build, and delay the exit selection mode.
  const {
    handleDeletePress,
    handleShareConfirm,
    shareMessages,
    setShareMessages,
  } = useChatSelectionActions({ tabId: tab.id, toastRef });

  // Bubble Operations hook, encapsulating editing, copying, feedback, etc.
  const { handleEdit, handleCopy, handleFeedback } = useChatBubbleActions({
    tabId: tab.id,
    appId,
    toastRef,
    setFeedbackMessageId,
  });

  // Checks the notification rights on entering the chat page for the first time, and applies for a window without requesting the active-ball system.
  useEffect(() => {
    useSettingsStore.getState().syncNotificationPermission();
  }, []);

  // ==================== Keyboard ====================

  const isEmptyState =
    hasLoadedInitialMessages && messageIds.length === 0 && !isLoadingMore;

  // ==================== Attachment Panel ====================

  // Attachment Panel controller, manage panel switch status, attachment selection/Remove, select the type.
  const attachmentPanelController = usePhoneAttachmentPanelController(
    appId,
    tab.id,
  );

  // Attachment Panel Expand/Move the content area up when it closes/Return to the animation value.
  const attachmentPanelTranslateY = useRef(new Animated.Value(0)).current;

  // Phone in the mode, move the whole content up and back when the attachment panel is expanded.
  // Pad Mode does not display attachment panels and does not need animated panels.
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

  // ==================== Initialization life cycle ====================

  // Reader controller, registered native TTS The incident was wired, just... iOS Entry into force.
  // ChatBubble Internal passage useReadAloudPlayback Drive reading interactively,
  // Call here to ensure that the event returns to the whole area Chat2 Effective within life cycle.
  useReadAloudPlaybackController();

  // Initializing the life cycle of the chat page hook• Integrated management of four functions:
  // 1. Loading First Screen Message + Force last message to finish the status
  // 2. App Message Back (from the interface if failed) SQLite (Recovered)
  // 3. Sync Bookmark Status To Native tab Collection Identification
  // 4. Unmount Cleanup (N) DEV Clear messages, input, map, unread, etc. in an environment store Data)
  useChatInit({
    tab,
    appId,
    initialMessageLimit: PAGE_SIZE,
    onInitialMessagesLoaded: useCallback((data: IMessage[]) => {
      setHasMore(data.length === PAGE_SIZE);
      setHasLoadedInitialMessages(true);
    }, []),
  });

  // Loading earlier history messages when you touch them. Add to store Head.
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

  // ==================== Scroll Track ====================

  // (a) Real-time tracking of scroll positions and determination of whether they are close to the bottom and automatically bottomed when the keyboard is loaded and the stept content is high;
  // Update simultaneously"Scroll Bottom"button.
  const handleScroll = useCallback((event: LegendScrollEvent) => {
    const { contentOffset, contentSize, layoutMeasurement } = event.nativeEvent;
    const distanceToBottom =
      contentSize.height - (contentOffset.y + layoutMeasurement.height);
    isNearBottomRef.current = distanceToBottom <= BOTTOM_THRESHOLD;
    // Skip buttons to update them in a subtle manner before the first screen bottom is stabilized;
    // Otherwise... initialScrollAtEnd/scrollToEnd Intermediate based on an overestimation trigger onScroll
    // You can get a bug in the button when you first load it.
    if (suppressScrollToBottomVisibilityRef.current) return;
    // Use higher thresholds during cooling period to prevent layout revision period onScroll (a) an intermediate error trigger button;
    // When the user really scrolls up distanceToBottom It's gonna be big, and it's still gonna show right.
    const inCooldown = Date.now() < scrollToBottomCooldownUntilRef.current;
    const threshold = inCooldown
      ? SCROLL_TO_BOTTOM_COOLDOWN_THRESHOLD
      : SCROLL_TO_BOTTOM_THRESHOLD;
    setIsScrollToBottomVisible(distanceToBottom > threshold);
  }, []);

  // When the user starts to drag and roll, the attachment panel is automatically closed if it is in a state of expansion.
  // Avoids the uncoordinated experience of a list scrolling and bottom attachment panel.
  const handleScrollBeginDrag = useCallback(() => {
    if (!layoutTokens.isTablet && attachmentPanelController.isAttachmentPanelOpen) {
      attachmentPanelController.closeAttachmentPanel();
    }
  }, [layoutTokens.isTablet, attachmentPanelController]);

  // ==================== Keyboard Processing ====================

  // When the keyboard is up, close to the bottom scrollToEnd Bottom.
  // iOSKeyboard overlaythe packaging remains unchanged,scrollToEnd Compensatory bottom.
  // Android: adjustResize a condensed container,KeyboardProvider After the package. LegendList It's... maintainScrollAtEnd
  // Should handleLayout It's always a bottom, and here it is.
  useEffect(() => {
    if (!isKeyboardVisible || !isNearBottomRef.current) return;

    requestAnimationFrame(() => {
      void listRef.current?.scrollToEnd({ animated: false });
    });
  }, [isKeyboardVisible, keyboardInset]);

  // (a) The high change back of the message line, the cache height and the update of the estimate;
  // When you are close to the bottom, you re-trace it to avoid the high-level of the stairwell and leave it behind.
  const handleRowHeightChange = useCallback(
    (messageId: string, height: number) => {
      const prev = rowHeightCacheRef.current.get(messageId) ?? 0;
      if (Math.abs(prev - height) <= 1) return;

      rowHeightCacheRef.current.set(messageId, height);

      // No more manual scrollToEnd.
      // maintainScrollAtEnd.itemLayout Configured animated: true,
      // Automatically smooth bottoms when line changes; manual scrollToEnd I'm not sure I'm gonna be in conflict with it.
      // When it causes content to be dissatisfied"Jump and fall."The visual flash.
    },
    [],
  );

  // Returns the article by article information with an estimate; the measured line directly reuses the real height, and the unmeasured line is conservatively estimated by the structure of the answer content.
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

  // ==================== Life cycle ====================

  // tab Resets all scroll and estimate states when switching.
  useEffect(() => {
    setHasLoadedInitialMessages(false);
    hasAppliedInitialBottomLockRef.current = false;
    isNearBottomRef.current = true;
    suppressScrollToBottomVisibilityRef.current = true;
    scrollToBottomCooldownUntilRef.current = 0;
    setIsScrollToBottomVisible(false);
    rowHeightCacheRef.current.clear();
  }, [tab.id]);

  // Force maintenance when the first screen message is in place isNearBottomRef = true,
  // Let follow-up handleRowHeightChange Keep bottoming until the real layout stabilizes.
  // Reason:scrollToEnd The endpoint is calculated on the basis of the estimated height.onScroll May be miscalculated."Not at the bottom.",
  // The lead line is high and no more bottom triggers, and the front screen message is too close to the input box.
  useEffect(() => {
    if (!hasLoadedInitialMessages || hasAppliedInitialBottomLockRef.current) {
      return;
    }
    hasAppliedInitialBottomLockRef.current = true;

    // The initial phase is mandatory to close to the bottom to ensure that the bottom is maintained in high-variant times.
    isNearBottomRef.current = true;

    requestAnimationFrame(() => {
      void listRef.current?.scrollToEnd({ animated: false });
    });

    // Delay 500ms Fill the bottom again and fix the position after most lines have completed the real layout.
    const settleTimer = setTimeout(() => {
      isNearBottomRef.current = true;
      suppressScrollToBottomVisibilityRef.current = false;
      setIsScrollToBottomVisible(false);
      // Start cooling period, during which the buttons are raised to show the threshold to prevent layout correction onScroll Middle-state error trigger button.
      scrollToBottomCooldownUntilRef.current = Date.now() + SCROLL_TO_BOTTOM_COOLDOWN_MS;
      void listRef.current?.scrollToEnd({ animated: false });
    }, 500);

    return () => {
      clearTimeout(settleTimer);
    };
  }, [hasLoadedInitialMessages, messageIds.length]);

  // ==================== Edit State ====================

  // Exit edit mode, empty box text, attachments and edit messages ID.
  const handleCancelEdit = useCallback(() => {
    clearInput(tab.id);
  }, [tab.id, clearInput]);

  // ==================== Attachment Panel Interactive ====================

  // Click"Scroll Bottom"button, smooth scroll to the bottom of the list.
  const handleScrollToBottom = useCallback(() => {
    void listRef.current?.scrollToEnd({ animated: true });
  }, []);

  // Records the touch point of the content area, which is used to distinguish short-press and drag.
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

  // When the content area touches, only short-press action closes the attachment panel.
  // Neither the long press nor the drag triggers the error to the long message.
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

  // ==================== Render ====================

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

  // Empty-state components with base reimbursement for LegendList It's... ListEmptyComponent.
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

  // Message List Area, including Loading State and LegendList.
  // Always Render LegendList- Yes, I do. ListEmptyComponent The government has been working on the issue of the air.
  // Avoid LegendList Unmount/Original on remount ScrollView Reuse caused layout anomalies.
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
          // @ts-ignore
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
                24 + // Compensated input area marginTop:-24 , and ensure that the message is sufficiently spaced from the input box.
                listKeyboardPadding,
            },
            // When empty, the contents are full of eyes, let ChatEmpty It's... flex:1 + justifyContent:center (a) Is moderately mediated;
            // If you don't get enough, you can start at the top. alignItemsAtEnd It's... flex-end Could not close temporary folder: %s
            isEmptyState ? { flexGrow: 1 } : undefined,
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
          // Only allow for late messages maintainScrollAtEnd Take over the bottom.
          // Avoid with maintainVisibleContentPosition It's... data The anchors are changing at the same time. scroll The news is very long and it is not good.
          maintainScrollAtEnd={{
            animated: true,
            on: {
              dataChange: true,
              itemLayout: true,
              layout: true,
            },
          }}
          // Reservations size Stable capacity to handle high-level changes in rolling, but off data The anchor is in place.
          // Avoid in"Add new message after answer is finished"The message is returned in error.
          maintainVisibleContentPosition={{ size: true }}
          // Current line of messages contains local layout caches and complex interactions, pre-eminence closing recycleItems,
          // Avoid LegendList v3 beta Reuse row The old example is now being brought to news.
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
      {/* Comment in Chinese: Show at bottom above the threshold"Scroll Bottom"button, hide in the selection mode to avoid conflict with the bottom operating column. */}
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

  // Enter the box area,Phone Mode to attachment panel controller.
  const inputNode = (
    <KeyboardStickyView
      // iOS Phone Use KeyboardStickyView Following keyboard adsorption;
      // Android Phone By adjustResize (b) Indents, the input box is naturally above the keyboard and does not require additional lifting;
      // Pad Use marginBottom Push up.
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

  // ==================== Phone Layout ====================

  // Phone Under Mode, Content Area + Input Box + The three brothers on the attachment panel together. translateY,
  // Moves the whole up and back when you open the attachment panel.
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

        {/* Comment in Chinese: Select the bottom operating bar of the mode, remove or share confirmation buttons according to the mode. */}
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
          // Phone The end attachment locator bottomshows and the bottom attachment panel maintains the same interactive direction.
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

  // ==================== Pad Layout ====================

  // Pad mode without attachment panels, directly align content areas and input boxes.
  return (
    <View style={styles.root}>
      {contentNode}
      {inputNode}

      {/* Comment in Chinese: Select the bottom operating bar of the mode, remove or share confirmation buttons according to the mode. */}
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

// Delete confirmation column components, show the selected number and delete/Cancel the button.
// Independent extraction as React.memo, avoids unnecessary retrofitting of parent components when they are retrofitted.
const DeleteSelectionBar = React.memo(function DeleteSelectionBar({
  onPress,
  onCancel,
}: {
  onPress: () => void;
  onCancel: () => void;
}) {
  // Directly from store Subscription to the selected item count, independent of the frequency of the parent component rendering.
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

// Single message line component from store Read message data and render questions/Answer the bubble.
// Use React.memo Do not re-render messages when they are not relevant.
// Shows each bubble to the left in the selection mode Checkbox, the bubble click to switch the selected status of the part.
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
  // Select Mode Status From store Read,Zustand The Quasi Subscribe Drive Rendering,
  // Around LegendList It's... renderItem Cache, make sure. Checkbox Show on-time/Hide.
  const selectionMode = useMessageStore((state) => state.selectionMode);
  // Switch the selected status of a certain part of a message from store Read,
  // Avoid Passage prop Transfer caused renderItem Reliance on the swelling of the item.
  const toggleSelection = useMessageStore((state) => state.toggleSelection);
  // The selected status of the current message parts for Checkbox Check.
  const selectedParts = useMessageStore(
    useCallback(
      (state) => state.selectedItems[messageId] || EMPTY_SELECTED_PARTS,
      [messageId],
    ),
  );

  const lastReportedHeightRef = useRef(0);

  // The height is reported when the line layout changes and is used to adjust the dynamically the line height.
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

  // Rendering individual bubbles (incl. checkbox) and display on the left side of the selection mode Checkbox,
  // Normally arranged in non-selection mode. Each bubble corresponds to one MessagePart(attachments/question/answer).
  // isQuestion Control bubbleContentWrapper Other Organiserquestion Right-hand.answer Left alignment.
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

// ==================== Styles ====================

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
      // Input area up 24pxThe following is a list of the main areas of the project:
      // List paddingBottom This value is compensated to ensure sufficient space between the message and the input box.
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
    // "Scroll Bottom"button, centered at the bottom, does not intercept a scrolling list event.
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
    // This layer of transparency border must be maintained; the removal of the ultra-long table message will overlap in the list for which the current reason is not located.
    messageListWrapperGuard: {
      borderWidth: 1,
      borderColor: 'transparent',
      backgroundColor: 'transparent',
    },
    messageGroup: {
      marginBottom: spacing.md,
    },
    // This layer of transparency must be retained; the problem of overlaps will recur when removed alone, for reasons that are not in place.
    messageGroupGuard: {
      borderWidth: 1,
      borderColor: 'transparent',
      backgroundColor: 'transparent',
    },
    // Select the layout of each bubble line in mode, left checkbox + Right bubble.
    bubbleWithCheckbox: {
      flexDirection: 'row',
      alignItems: 'flex-start',
      marginBottom: spacing.xxs,
      width: '100%',
    },
    // checkbox Slot, fixed width, aligned to the top of the bubble.
    checkboxContainer: {
      width: SELECTION_CHECKBOX_SLOT_WIDTH,
      paddingTop: 14,
      alignItems: 'flex-start',
    },
    // The bubble content area.flex:1 Fill checkbox Other than the remaining width, default not set alignItems.
    bubbleContentWrapper: {
      flex: 1,
      minWidth: 0,
    },
    // question/attachments Bubbles right.
    bubbleAlignEnd: {
      alignItems: 'flex-end',
    },
    // answer Air bubble left alignment.
    bubbleAlignStart: {
      alignItems: 'flex-start',
    },
    // Phone Animated containers in the content area under mode.flex:1 Fill up the remaining space.
    phoneContentMotion: {
      flex: 1,
    },
    // Phone mode. The width is full.
    phoneInputMotion: {
      width: '100%',
    },
    // Attachment Panel Float, default hidden at the bottom of the screen, as it expands translateY Move out.
    attachmentPanelOverlay: {
      position: 'absolute',
      left: 0,
      right: 0,
      zIndex: 90,
    },
    // Select the bottom operating column container of the mode, which is fixed at the bottom.
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

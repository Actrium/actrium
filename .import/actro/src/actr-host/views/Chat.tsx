import React, { useEffect, useState, useCallback, useMemo, useRef } from 'react';
import {
  View,
  Text,
  StyleSheet,
  FlatList,
  ActivityIndicator,
  TouchableWithoutFeedback,
  useWindowDimensions,
  NativeSyntheticEvent,
  NativeScrollEvent,
} from 'react-native';
import { useTranslation } from 'react-i18next';
import Clipboard from '@react-native-clipboard/clipboard';
import ActrInput from '../components/ActrInput';
import {
  MessageRepository,
  IMessage,
  Progress,
} from '../../repository/MessageRepository';
import { useMessageStore } from '../store/useMessageStore';
import { useAppStore } from '../store/useAppStore';
import ChatBubble from '../components/ChatBubble';
import { Toast, ToastRef } from '../components/Toast';
import ChatEmpty from './ChatEmpty';
import { useTabIdMapStore } from '../store/useTabIdMapStore';
import { useTab } from '../contexts/TabContext';
import { useInputStore } from '../store/useInputStore';
import { IAttachmentItem } from '../types/attachment';
import { DeleteButton } from '../components/DeleteButton';
import { ScrollToBottomButton } from '../components/ScrollToBottomButton';
import { Checkbox } from '../components/Checkbox';
import { AppSessionService } from '../../services/AppSessionService';
import { getHostLayoutTokens } from '../../utils/layout';
import { useKeyboardInset } from '../../hooks/useKeyboardInset';
import { useUnreadStore } from '../store/useUnreadStore';
import { setTabUnreadStatus } from '../../../src/native-bridge/tab/TabModule';
import { GlobalAlert } from '../../../src/components';

const PAGE_SIZE = 20;
const EMPTY_MESSAGE_IDS: string[] = [];
const EMPTY_SELECTED_PARTS = new Set<'attachments' | 'question' | 'answer'>();

interface MessageRowProps {
  appId: string;
  messageId: string;
  index: number;
  layoutTokens: ReturnType<typeof getHostLayoutTokens>;
  selectionMode: boolean;
  selectedParts: Set<'attachments' | 'question' | 'answer'>;
  toggleSelection: (messageId: string, part: 'attachments' | 'question' | 'answer') => void;
  handleEdit: (msg: IMessage) => void;
  handleCopy: (text: string) => void;
}

const MessageRow = React.memo(function MessageRow({
  appId,
  messageId,
  index,
  layoutTokens,
  selectionMode,
  selectedParts,
  toggleSelection,
  handleEdit,
  handleCopy,
}: MessageRowProps) {
  const { t } = useTranslation();
  const item = useMessageStore(
    useCallback(
      (state) => state.messageById[appId]?.[messageId],
      [appId, messageId],
    ),
  );

  if (!item) return null;

  const isLatest = index === 0;

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
      <View
        style={[
          styles.messageGroup,
          selectionMode && styles.messageGroupWithCheckbox,
        ]}
      >
        {item.attachments && (
          <View style={styles.bubbleWithCheckbox}>
            {selectionMode && (
              <View style={styles.checkboxContainer}>
                <Checkbox
                  checked={selectedParts.has('attachments')}
                  onPress={() => toggleSelection(item.message_id, 'attachments')}
                />
              </View>
            )}
            <View style={styles.questionBubbleWrapper}>
              <ChatBubble
                type='question'
                attachments={item.attachments}
                containerStyle={{ marginBottom: 4 }}
                onEdit={isLatest && !selectionMode ? () => handleEdit(item) : undefined}
                onCopy={() => handleCopy(item.question_content)}
                msgId={item.message_id}
                appId={appId}
                selectionKey={`${item.message_id}:attachments`}
                isSelected={selectedParts.has('attachments')}
                onToggleSelection={() => toggleSelection(item.message_id, 'attachments')}
              />
            </View>
          </View>
        )}
        {item.question_content ? (
          <View style={styles.bubbleWithCheckbox}>
            {selectionMode && (
              <View style={styles.checkboxContainer}>
                <Checkbox
                  checked={selectedParts.has('question')}
                  onPress={() => toggleSelection(item.message_id, 'question')}
                />
              </View>
            )}
            <View style={styles.questionBubbleWrapper}>
              <ChatBubble
                type='question'
                msgId={item.message_id}
                appId={appId}
                content={item.question_content}
                onEdit={isLatest && !selectionMode ? () => handleEdit(item) : undefined}
                onCopy={() => handleCopy(item.question_content)}
                selectionKey={`${item.message_id}:question`}
                isSelected={selectedParts.has('question')}
                onToggleSelection={() => toggleSelection(item.message_id, 'question')}
              />
            </View>
          </View>
        ) : null}
        {item.answer_content ? (
          <View style={styles.bubbleWithCheckbox}>
            {selectionMode && (
              <View style={styles.checkboxContainer}>
                <Checkbox
                  checked={selectedParts.has('answer')}
                  onPress={() => toggleSelection(item.message_id, 'answer')}
                />
              </View>
            )}
            <ChatBubble
              type='answer'
              msgId={item.message_id}
              appId={appId}
              content={item.answer_content}
              isLatest={isLatest}
              onCopy={() => handleCopy(item.answer_content)}
              selectionKey={`${item.message_id}:answer`}
              isSelected={selectedParts.has('answer')}
              onToggleSelection={() => toggleSelection(item.message_id, 'answer')}
            />
          </View>
        ) : isLatest &&
          (item.progress === Progress.pending ||
            item.progress === Progress.processing) ? (
          <View style={styles.loadingContainer}>
            <ActivityIndicator size='small' color='#999' />
            <Text style={styles.loadingText}>{t('actr.chat.thinking')}</Text>
          </View>
        ) : null}
      </View>
    </View>
  );
});

export default function Chat() {
  const { t } = useTranslation();
  const { tab } = useTab();
  const { width } = useWindowDimensions();
  const appId = useTabIdMapStore((state) => state.tabIdMap[tab.id]);
  const layoutTokens = useMemo(() => getHostLayoutTokens(width), [width]);

  const messageIds = useMessageStore(
    useCallback(
      (state) => state.messageIds[appId] || EMPTY_MESSAGE_IDS,
      [appId],
    ),
  );
  const setMessagesInStore = useMessageStore((state) => state.setMessages);
  const appendOldMessages = useMessageStore((state) => state.appendOldMessages);
  const cleanMessage = useMessageStore((state) => state.cleanMessage);
  const selectionMode = useMessageStore((state) => state.selectionMode);
  const selectedItems = useMessageStore((state) => state.selectedItems);
  const toggleSelection = useMessageStore((state) => state.toggleSelection);
  const exitSelectionMode = useMessageStore((state) => state.exitSelectionMode);
  const deleteSelectedMessages = useMessageStore(
    (state) => state.deleteSelectedMessages,
  );

  const cleanAppInfo = useAppStore((state) => state.cleanAppInfo);
  const appInfo = useAppStore((state) => state.apps[appId]);
  const addMessage = useMessageStore((state) => state.addMessage);
  const removeTabIdMap = useTabIdMapStore((state) => state.removeTabIdMap);

  const [isLoadingMore, setIsLoadingMore] = useState(false);
  const [hasMore, setHasMore] = useState(true);
  const [showScrollToBottom, setShowScrollToBottom] = useState(false);
  const { keyboardInset, dismissKeyboard } = useKeyboardInset();
  const toastRef = React.useRef<ToastRef>(null);
  const flatListRef = useRef<FlatList>(null);

  // 滚动到底部（inverted 列表中 scrollToOffset 0 就是底部）
  const scrollToBottom = useCallback(() => {
    flatListRef.current?.scrollToOffset({ offset: 0, animated: true });
  }, []);

  // 监听滚动位置，判断是否显示回到底部按钮
  const handleScroll = useCallback(
    (event: NativeSyntheticEvent<NativeScrollEvent>) => {
      const offsetY = event.nativeEvent.contentOffset.y;
      // 当滚动超过 200 像素时显示按钮
      setShowScrollToBottom(offsetY > 200);
    },
    [],
  );

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

  const loadAppInfo = useCallback(async () => {
    if (!appId || appInfo) return;
    await AppSessionService.initSession(tab.id, appId);
  }, [appId, appInfo, tab.id]);

  // 初始化加载第一页
  const loadInitialMessages = useCallback(async () => {
    try {
      const data = await MessageRepository.getAllMessages(appId, PAGE_SIZE, 0);
      setMessagesInStore(appId, data);
      setHasMore(data.length === PAGE_SIZE);
    } catch (error) {
      console.error('[Main] Failed to load initial messages:', error);
      setMessagesInStore(appId, []);
    }
  }, [appId, setMessagesInStore]);

  // 加载更多
  const handleLoadMore = useCallback(async () => {
    // 防止在第一页加载完成前或 messages 为空时触发
    if (isLoadingMore || !hasMore || messageIds.length === 0) return;

    setIsLoadingMore(true);
    try {
      const offset = messageIds.length;
      const data = await MessageRepository.getAllMessages(
        appId,
        PAGE_SIZE,
        offset,
      );

      if (data.length > 0) {
        appendOldMessages(appId, data);
      }

      setHasMore(data.length === PAGE_SIZE);
    } catch (error) {
      console.error('[Main] Failed to load more messages:', error);
    } finally {
      setIsLoadingMore(false);
    }
  }, [appId, messageIds.length, isLoadingMore, hasMore, appendOldMessages]);

  useEffect(() => {
    // 🌟 无条件执行：确保每次挂载都能同步最新第一页数据，覆盖旧缓存导致的错乱
    loadInitialMessages();
    loadAppInfo();

    if (appId) {
      // 同步更新未读消息 store
      useUnreadStore.getState().clearUnread(appId, tab.id);
    }

    return () => {
      // TODO 是否已经保存到书签
      // cleanMessage(appId);
      // cleanAppInfo(appId);
      // removeTabIdMap(tab.id);
    };
  }, [appId, loadInitialMessages, cleanMessage, loadAppInfo, cleanAppInfo]);

  const setStoreText = useInputStore((state) => state.setInputText);
  const setStoreAttachments = useInputStore((state) => state.setAttachments);
  const setEditingMessageId = useInputStore(
    (state) => state.setEditingMessageId,
  );

  const handleEdit = useCallback(
    (msg: IMessage) => {
      // 填充文字
      setStoreText(appId, msg.question_content || '');

      // 设置正在编辑的消息 ID
      setEditingMessageId(appId, msg.message_id);

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
          setStoreAttachments(appId, mapped);
        } catch (e) {
          console.error('Failed to parse attachments for edit', e);
        }
      } else {
        setStoreAttachments(appId, []);
      }
    },
    [appId, setStoreText, setStoreAttachments, setEditingMessageId],
  );

  const input = useMemo(() => {
    return <ActrInput appId={appId} />;
  }, [appId]);

  const renderItem = useCallback(
    ({ item: messageId, index }: { item: string; index: number }) => {
      return (
        <MessageRow
          appId={appId}
          messageId={messageId}
          index={index}
          layoutTokens={layoutTokens}
          selectionMode={selectionMode}
          selectedParts={selectedItems[messageId] || EMPTY_SELECTED_PARTS}
          toggleSelection={toggleSelection}
          handleEdit={handleEdit}
          handleCopy={handleCopy}
        />
      );
    },
    [appId, layoutTokens, selectionMode, selectedItems, toggleSelection, handleEdit, handleCopy],
  );

  const ListHeaderComponent = useMemo(() => {
    if (!hasMore) return null;
    return (
      <View style={{ paddingVertical: 20 }}>
        {isLoadingMore ? (
          <ActivityIndicator size='small' color='#999' />
        ) : (
          <Text style={{ textAlign: 'center', color: '#999', fontSize: 12 }}>
            {t('actr.chat.loadMore')}
          </Text>
        )}
      </View>
    );
  }, [hasMore, isLoadingMore]);

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
    GlobalAlert.alert(t('actr.chat.deleteConfirmTitle'), t('actr.chat.deleteConfirmMsg'), [
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
            deleteSelectedMessages(appId).catch((error) => {
              console.error(
                '[Chat] Failed to delete selected messages:',
                error,
              );
              toastRef.current?.show(t('actr.chat.deleteFailed'));
            });
          }, 0);
        },
      },
    ]);
  }, [appId, deleteSelectedMessages, exitSelectionMode, t]);

  return (
    <View
      style={[
        styles.mainContainer,
        { backgroundColor: layoutTokens.isTablet ? '#ffffff' : '#F2F2F6' },
      ]}
    >
      <View style={styles.contentArea}>
        {messageIds.length === 0 && !isLoadingMore ? (
          <TouchableWithoutFeedback onPress={dismissKeyboard}>
            <View style={{ flex: 1 }}>
              <ChatEmpty />
            </View>
          </TouchableWithoutFeedback>
        ) : (
          <FlatList
            ref={flatListRef}
            data={messageIds}
            renderItem={renderItem}
            keyExtractor={(item) => item}
            inverted={true}
            onEndReached={handleLoadMore}
            onEndReachedThreshold={0.1}
            onScroll={handleScroll}
            scrollEventThrottle={16}
            style={styles.chatScrollView}
            contentContainerStyle={[
              styles.chatContent,
              { paddingBottom: layoutTokens.chatListBottomPadding },
            ]}
            ListFooterComponent={ListHeaderComponent}
            showsHorizontalScrollIndicator={false}
            horizontal={false}
            keyboardShouldPersistTaps='handled'
            keyboardDismissMode='on-drag'
          />
        )}
      </View>

      {/* 回到底部按钮 */}
      {!selectionMode && (
        <View style={styles.scrollToBottomContainer}>
          <ScrollToBottomButton
            visible={showScrollToBottom}
            onPress={scrollToBottom}
          />
        </View>
      )}

      {/* 输入框 */}
      <View
        style={[
          styles.inputContainer,
          {
            backgroundColor: layoutTokens.isTablet ? '#fff' : '#F2F2F6',
            marginBottom: layoutTokens.isTablet ? keyboardInset : 0,
          },
        ]}
      >
        <View style={styles.inputInner}>{input}</View>
      </View>

      {/* 删除按钮 - 绝对定位在输入框中心 */}
      {selectionMode && (
        <View
          style={[
            styles.deleteButtonContainer,
            { bottom: layoutTokens.deleteButtonBottom },
          ]}
        >
          <DeleteButton
            selectedCount={selectedCount}
            onPress={handleDeletePress}
            onCancel={exitSelectionMode}
          />
        </View>
      )}

      <Toast ref={toastRef} />
    </View>
  );
}

const styles = StyleSheet.create({
  mainContainer: {
    flex: 1,
    backgroundColor: '#ffffff',
    // 移除 paddingBottom，由内部组件处理
  },
  contentArea: {
    flex: 1,
    width: '100%',
  },
  inputContainer: {
    width: '100%',
    backgroundColor: '#fff',
  },
  inputInner: {
    width: '100%', // 移除 maxWidth，让 ActrInput 容器占满全宽
  },
  scrollToBottomContainer: {
    position: 'absolute',
    left: 0,
    right: 0,
    bottom: 90, // 在输入框上方
    alignItems: 'center',
    zIndex: 99,
    pointerEvents: 'box-none',
  },
  chatScrollView: {
    flex: 1,
    width: '100%', // 确保滚动条在窗口最边缘
  },
  chatContent: {
    paddingTop: 24,
    width: '100%',
    alignItems: 'stretch', // 允许子项宽度自动扩展，然后由子项自己的 alignSelf: 'center' 处理居中
  },
  messageListWrapper: {
    width: '100%',
    alignSelf: 'center', // 确保在 FlatList 内部居中
  },
  messageGroup: {
    width: '100%',
    marginBottom: 12,
  },
  messageGroupWithCheckbox: {
    paddingLeft: 30, // 为复选框留出空间
  },
  bubbleWithCheckbox: {
    flexDirection: 'row',
    alignItems: 'flex-start',
    position: 'relative',
    marginBottom: 4,
    width: '100%',
  },
  questionBubbleWrapper: {
    flex: 1,
    alignItems: 'flex-end',
    minWidth: 0, // 允许 flex 子元素收缩
  },
  checkboxContainer: {
    position: 'absolute',
    left: -30,
    top: 14, // 与气泡框第一行的 paddingVertical 对齐
    zIndex: 10,
  },
  loadingContainer: {
    flexDirection: 'row',
    alignItems: 'center',
    paddingVertical: 12,
    paddingHorizontal: 4,
  },
  loadingText: {
    marginLeft: 8,
    fontSize: 14,
    color: '#999',
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
});

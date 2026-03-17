import React, { useCallback, useRef, useState } from 'react';
import {
  View,
  StyleSheet,
  NativeModules,
  useWindowDimensions,
} from 'react-native';
import { GlobalAlert } from '../../../../src/components';
import { useTranslation } from 'react-i18next';
import { useMessageStore } from '../../store/useMessageStore';
import { useAppStore } from '../../store/useAppStore';
import LocationPicker from '../LocationPicker';
import { useAttachmentManager } from '../../hooks/useAttachmentManager';
import { useInputToolboxState } from '../../hooks/useInputToolboxState';
import { useInputStore } from '../../store/useInputStore';
import { IMessage } from '../../../repository/MessageRepository';
import { VoiceOverlay } from '../VoiceOverlay';
import { useTab } from '../../contexts/TabContext';
import { useActrConnection } from '../../../hooks/useActrConnection';
import {
  TActrMessageResponse,
  TLocation,
} from '../../../native-bridge/actr/ActrConnection.type';
import { useAuth, AuthStatus } from '../../../hooks/auth';
import { useAuthTaskStore } from '../../store/useAuthTaskStore';
import { Utils } from '../../../utils/utils';
import { getHostLayoutTokens } from '../../../utils/layout';
import ActrInputMobileView from './ActrInputMobileView';
import ActrInputPadView from './ActrInputPadView';
import { StableTextInput } from './StableTextInput';
import { useActrVoiceController } from './hooks/useActrVoiceController';
import { LoginModal, AuthorizeModal } from '../../../system-apps/auth';
import { useUnreadStore } from '../../store/useUnreadStore';
import { getCurrentFullscreenTab, openTab } from '../../../../src/native-bridge/tab/TabModule';

const { NativeLogger } = NativeModules;

interface ActrInputProps {
  appId: string;
  variant?: 'pad' | 'mobile';
}

export default function ActrInput({ appId, variant }: ActrInputProps) {
  const { t } = useTranslation();
  const { tab } = useTab();
  const { width } = useWindowDimensions();
  const layoutTokens = React.useMemo(() => getHostLayoutTokens(width), [width]);
  const isMobileVariant =
    variant != null ? variant === 'mobile' : !layoutTokens.isTablet;

  // 登录弹窗状态
  const [showLoginModal, setShowLoginModal] = useState(false);
  const [authParams, setAuthParams] = useState<{
    clientId: string;
    appId: string;
  } | null>(null);

  // 授权弹窗状态（已登录未授权时使用）
  const [showAuthorizeModal, setShowAuthorizeModal] = useState(false);
  // 记录“这次弹登录/授权是由什么动作触发的”：
  // - text_send: 登录成功后可自动续发文本
  // - voice_start: 仅完成鉴权，不自动发送文本（等待用户再次长按）
  const [pendingAuthAction, setPendingAuthAction] = useState<
    'text_send' | 'voice_start' | null
  >(null);

  const inputText = useInputStore((state) => state.inputs[tab.id]?.text || '');
  const editingMessageId = useInputStore(
    (state) => state.inputs[tab.id]?.editingMessageId,
  );
  const clearInputInStore = useInputStore((state) => state.clearInput);
  const setInputText = useInputStore((state) => state.setInputText);
  const setEditingMessageId = useInputStore(
    (state) => state.setEditingMessageId,
  );

  const handleCancelEdit = () => {
    setEditingMessageId(tab.id, null);
    clearInputInStore(tab.id);
  };

  const deleteMessageFromStore = useMessageStore(
    (state) => state.deleteMessage,
  );

  const [showMapModal, setShowMapModal] = useState(false);
  // Hook for attachment logic
  const {
    attachments,
    removeAttachment,
    handleConfirmLocation,
    takePhoto,
    pickImage,
    pickDocument,
  } = useAttachmentManager(tab.id);

  // Hook for toolbox states
  const {
    isWebSearchActive,
    setIsWebSearchActive,
    isLocationActive,
    setIsLocationActive,
    isSendOnEnter,
    setIsSendOnEnter,
    showSearchEntry,
    showLocationEntry,
    showNewlineEntry,
    toolboxProps,
  } = useInputToolboxState(appId);

  const addMessage = useMessageStore((state) => state.addMessage);
  const updateMessage = useMessageStore((state) => state.updateMessage);
  const appInfo = useAppStore((state) => state.apps[appId]);

  const { checkAuth } = useAuth(appId);

  /**
   * 处理授权状态，根据设备类型和状态决定行为
   * @returns true 表示已授权可继续，false 表示需要登录/授权
   */
  const handleAuthStatus = useCallback(
    (status: AuthStatus, clientId: string): boolean => {
      if (status === 'authorized') {
        return true;
      }

      if (status === 'not_logged_in') {
        if (layoutTokens.isTablet) {
          // iPad：打开登录 Tab
          openTab?.({ systemAppType: 'login' })
            .then((newTabId: string) => {
              if (newTabId) {
                useAuthTaskStore.getState().regTask(newTabId, clientId, appId);
              }
            })
            .catch((err: any) => {
              console.error('[ActrInput] Failed to open login tab:', err);
            });
        } else {
          // 手机：弹登录窗口
          setAuthParams({ clientId, appId });
          setShowLoginModal(true);
        }
        return false;
      }

      if (status === 'not_authorized') {
        // 两端均弹授权窗口
        setShowAuthorizeModal(true);
        return false;
      }

      return false;
    },
    [appId, layoutTokens.isTablet],
  );

  // 文本消息接收回调
  const onReceiveMessage = useCallback(
    (data: TActrMessageResponse) => {
      const message_id = data.message_id;
      // 1. 获取当前 Store 中的消息进行累加
      const currentMsg = useMessageStore.getState().getMessage(tab.id, message_id);

      if (!currentMsg) {
        console.warn('[ActrInput] No messages in store to update');
        return;
      }

      const isFinished = data.isEnd || data.status === 2;
      const newContent = (currentMsg.answer_content || '') + data.message;

      const updates = {
        answer_time: Date.now(),
        answer_content: newContent,
        progress: isFinished ? 2 : 1, // 2: 已完成, 1: 处理中
      };

      // 2. 更新 Store，如果是结束状态则触发持久化
      updateMessage(tab.id, currentMsg.message_id, updates, isFinished);

      // 3. 消息接收完成时，判断当前是否为活跃 tab
      if (data.isEnd) {
        getCurrentFullscreenTab().then((currentTabId) => {
          if (currentTabId === tab.id) {
            // 当前 tab 正在前台，直接标为已读，无需增加未读数
            updateMessage(tab.id, currentMsg.message_id, { is_read: 1 }, true);
          } else {
            // 后台 tab，增加未读计数并通知 native
            useUnreadStore.getState().incrementUnread(tab.id);
          }
        });
      }
    },
    [tab.id, updateMessage],
  );

  // 语音消息接收回调
  const onReceiveVoice = useCallback(
    (data: TActrMessageResponse) => {
      const messageId = data.message_id;

      const currentMsg = useMessageStore.getState().getMessage(tab.id, messageId);

      if (!currentMsg) return;

      const isFinished = data.status === 2;
      const newContent =
        currentMsg.answer_content + t('actr.voice.voiceMessageReceived');

      const updates = {
        answer_time: Date.now(),
        answer_content: newContent,
        progress: isFinished ? 2 : 1,
      };

      updateMessage(tab.id, messageId, updates, isFinished);
    },
    [tab.id, updateMessage],
  );

  // 使用多实例连接 Hook
  const {
    sendMessage,
    sendVoiceChunk,
    startVoice,
    endVoice,
    session_id,
    connection,
  } = useActrConnection({
    appId,
    tabId: tab.id,
    config: appInfo
      ? {
          app_id: appInfo.app_id,
          key_id: appInfo.key_id,
          authority_services: appInfo.authority_services,
          signaling_services: appInfo.signaling_services,
          stun_services: appInfo.stun_services,
          turn_services: appInfo.turn_services,
          realm_id: appInfo.realm_id,
          server_actor_type: appInfo.server_actor_type,
        }
      : undefined,
    callbacks: {
      onMessage: onReceiveMessage,
      onVoice: onReceiveVoice,
    },
  });

  const processMessageSend = async (
    content: string,
    options?: { isVoice?: boolean; skipAuthCheck?: boolean },
  ) => {
    const { isVoice, skipAuthCheck } = options || {};

    if (connection?.reason) {
      GlobalAlert.alert(t('actr.auth.serviceInitFailed'), connection.reason);
      return;
    }

    const trimmedContent = content.trim();
    if (!trimmedContent && !isVoice) {
      GlobalAlert.alert(t('common.hint'), t('actr.input.emptyTextAlert'));
      return;
    }

    // 1. 权限校验（登录/授权成功后的回调可跳过）
    if (!skipAuthCheck) {
      const { status, clientId } = await checkAuth();
      if (!handleAuthStatus(status, clientId)) {
        // 记录触发鉴权的来源，供登录/授权成功后的回调决定是否续发文本。
        setPendingAuthAction(isVoice ? 'voice_start' : 'text_send');
        return;
      }
      setPendingAuthAction(null);
    }

    // 2. 处理编辑模式：若是在修改旧消息，发送前先从库中移除旧版本
    if (editingMessageId) {
      const messageIdToDelete = editingMessageId;
      setEditingMessageId(tab.id, null);
      try {
        deleteMessageFromStore(tab.id, messageIdToDelete);
      } catch (e) {
        console.error(
          '[ActrInput] Failed to delete message during re-edit:',
          e,
        );
      }
    }

    // 3. 处理定位 从附件数据拿出
    let location: TLocation | undefined;
    if (attachments.length && attachments[0].attachment_type === 'location') {
      const local = attachments[0];
      location = {
        latitude: local.latitude!,
        longitude: local.longitude!,
        address: '',
        placeName: '',
      };
    }

    const newMessage: IMessage = {
      message_id: Utils.generateId(),
      app_id: appId,
      tab_id: tab.id,
      session_id: session_id,
      is_preset: 0,
      question_time: Date.now(),
      question_content: trimmedContent,
      answer_time: 0,
      answer_content: '',
      attachments:
        attachments.length > 0 ? JSON.stringify(attachments) : undefined,
      is_edited: editingMessageId ? 1 : 0,
      progress: 0,
    };

    // 4. 同步 UI 状态（Store 内部会自动持久化到本地）
    addMessage(tab.id, newMessage);

    const reqMsg = {
      message_id: newMessage.message_id,
      session_id: newMessage.session_id,
      message: trimmedContent,
      attachments: attachments?.filter(
        (att) => att.attachment_type !== 'location',
      ),
      location: location,
    };

    // 6. 重置输入框状态
    clearInputInStore(tab.id);

    // 如果不是语音，直接发送
    if (!isVoice) {
      await sendMessage(reqMsg);
    }

    return reqMsg;
  };

  const handleVoiceAuthPrecheck = useCallback(async (): Promise<boolean> => {
    if (connection?.reason) {
      GlobalAlert.alert(t('actr.auth.serviceInitFailed'), connection.reason);
      return false;
    }

    const { status, clientId } = await checkAuth();
    if (!handleAuthStatus(status, clientId)) {
      // 语音在“开始录音前”被鉴权拦截时，仅标记为 voice_start，不做自动文本续发。
      setPendingAuthAction('voice_start');
      return false;
    }

    setPendingAuthAction(null);
    return true;
  }, [checkAuth, connection?.reason, handleAuthStatus, t]);

  const handleSend = () => {
    processMessageSend(inputText);
  };
  const handleClearInput = () => {
    setInputText(tab.id, '');
  };

  // 稳定的 handleSend ref，传给 StableTextInput 避免因回调变化触发子组件重渲染
  const handleSendRef = useRef(handleSend);
  handleSendRef.current = handleSend;

  const handleAudio = () => {
    NativeLogger.log('切换语音显示，暂时忽略');
  };

  const {
    isVoiceRecording,
    isCancellingVoice,
    metering,
    handleVoiceStart,
    handleVoiceEnd,
    handleVoiceChangeState,
  } = useActrVoiceController({
    processVoiceMessageSend: processMessageSend,
    sendVoiceChunk,
    startVoice,
    endVoice,
    beforeVoiceStart: handleVoiceAuthPrecheck,
  });

  return (
    <View
      style={[
        styles.rootContainer,
        {
          paddingBottom: layoutTokens.inputBottomPadding,
          backgroundColor: isMobileVariant ? '#F2F2F6' : '#fff',
        },
      ]}
    >
      {isVoiceRecording && (
        <VoiceOverlay isCancelling={isCancellingVoice} metering={metering} />
      )}

      {isMobileVariant ? (
        <ActrInputMobileView
          appId={appId}
          // Mobile view owns its own layout styles; shared 仅透传输入框通用文本样式。
          sharedStyles={{
            input: styles.input,
            mobileInputText: styles.mobileInputText,
          }}
          attachments={attachments}
          removeAttachment={removeAttachment}
          setShowMapModal={setShowMapModal}
          showSearchEntry={showSearchEntry}
          showLocationEntry={showLocationEntry}
          showNewlineEntry={showNewlineEntry}
          isWebSearchActive={isWebSearchActive}
          setIsWebSearchActive={setIsWebSearchActive}
          isLocationActive={isLocationActive}
          setIsLocationActive={setIsLocationActive}
          isSendOnEnter={isSendOnEnter}
          setIsSendOnEnter={setIsSendOnEnter}
          takePhoto={takePhoto}
          pickImage={pickImage}
          pickDocument={pickDocument}
          handleVoiceStart={handleVoiceStart}
          handleVoiceChangeState={handleVoiceChangeState}
          handleVoiceEnd={handleVoiceEnd}
          isVoiceRecording={isVoiceRecording}
          isCancellingVoice={isCancellingVoice}
          metering={metering}
          inputText={inputText}
          onClearInput={handleClearInput}
          handleSendRef={handleSendRef}
          editingMessageId={editingMessageId}
          handleCancelEdit={handleCancelEdit}
        />
      ) : (
        <ActrInputPadView
          styles={styles}
          layoutTokens={layoutTokens}
          attachments={attachments}
          removeAttachment={removeAttachment}
          editingMessageId={editingMessageId}
          handleCancelEdit={handleCancelEdit}
          textInputNode={
            <StableTextInput
              tabId={tab.id}
              hasAttachments={attachments.length > 0}
              isSendOnEnter={isSendOnEnter}
              handleSendRef={handleSendRef}
              desktopInputStyle={styles.input}
              mobileInputStyle={styles.mobileInputText}
              autoGrowMaxLines={5}
              autoGrowLineHeight={22}
            />
          }
          takePhoto={takePhoto}
          pickImage={pickImage}
          pickDocument={pickDocument}
          setShowMapModal={setShowMapModal}
          handleSend={handleSend}
          handleAudio={handleAudio}
          inputText={inputText}
          handleVoiceStart={handleVoiceStart}
          handleVoiceChangeState={handleVoiceChangeState}
          handleVoiceEnd={handleVoiceEnd}
          toolboxProps={toolboxProps}
        />
      )}

      <LocationPicker
        visible={showMapModal}
        onClose={() => setShowMapModal(false)}
        onSelect={(data) => {
          handleConfirmLocation(data);
          setShowMapModal(false);
        }}
      />

      {/* Mobile 登录弹窗 */}
      {isMobileVariant && showLoginModal && authParams && (
        <LoginModal
          visible
          onClose={() => {
            setShowLoginModal(false);
            setAuthParams(null);
            setPendingAuthAction(null);
          }}
          clientId={authParams.clientId}
          appId={authParams.appId}
          onSuccess={() => {
            // 仅文本发送流程在登录后自动续发；语音长按流程只完成鉴权，不自动发送文本。
            if (pendingAuthAction === 'text_send') {
              processMessageSend(inputText, { skipAuthCheck: true });
            }
            setPendingAuthAction(null);
            setAuthParams(null);
          }}
        />
      )}

      {/* 已登录未授权时弹出授权窗口（两端均支持） */}
      <AuthorizeModal
        visible={showAuthorizeModal}
        onClose={() => setShowAuthorizeModal(false)}
        appId={appId}
        onSuccess={() => {
          // 仅文本发送流程在授权后自动续发；语音长按流程只完成鉴权，不自动发送文本。
          if (pendingAuthAction === 'text_send') {
            processMessageSend(inputText, { skipAuthCheck: true });
          }
          setPendingAuthAction(null);
        }}
      />
    </View>
  );
}

const styles = StyleSheet.create({
  rootContainer: {
    backgroundColor: '#fff',
    width: '100%', // 确保绝对全宽
  },
  container: {
    paddingHorizontal: 16,
    paddingTop: 8,
    paddingBottom: 20,
    width: '100%',
    maxWidth: 760,
    alignSelf: 'center',
  },
  inputWrapper: {
    backgroundColor: '#FFFFFF',
    borderRadius: 20,
    paddingHorizontal: 13,
    paddingTop: 18,
    paddingBottom: 9,
    // 移除边框，保留纯阴影
    shadowColor: '#000',
    shadowOffset: { width: 0, height: 4 },
    shadowOpacity: 0.08,
    shadowRadius: 12,
    elevation: 5, // 兼容 Android 的阴影
  },
  input: {
    fontSize: 17,
    lineHeight: 24,
    color: '#333',
    minHeight: 24,
    textAlignVertical: 'top',
    padding: 0,
    marginBottom: 0, // 移动到底部容器
  },
  inputRow: {
    flexDirection: 'row',
    alignItems: 'flex-end',
    marginBottom: 12,
    position: 'relative',
  },
  cancelEditButton: {
    position: 'absolute',
    left: -55,
    top: -6,
    width: 34,
    height: 34,
    borderRadius: 17,
    backgroundColor: '#fff',
    justifyContent: 'center',
    alignItems: 'center',
    zIndex: 100,
    // Shadow
    shadowColor: '#000',
    shadowOffset: { width: 0, height: 2 },
    shadowOpacity: 0.15,
    shadowRadius: 4,
    elevation: 4,
  },
  mobileInputText: {
    fontSize: 17,
    lineHeight: 22,
    color: '#333',
    padding: 0,
    textAlignVertical: 'center',
  },
});

export type ActrInputStyles = typeof styles;

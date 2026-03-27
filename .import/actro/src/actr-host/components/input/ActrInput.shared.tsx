import React, { useRef, useState } from 'react';
import { View, StyleSheet, NativeModules, useWindowDimensions } from 'react-native';
import LocationPicker from '../LocationPicker';
import { useAttachmentManager } from '../../hooks/useAttachmentManager';
import { useInputToolboxState } from '../../hooks/useInputToolboxState';
import { useInputStore } from '../../store/useInputStore';
import { VoiceRecordingOverlay } from '../VoiceRecordingOverlay';
import { useTab } from '../../contexts/TabContext';
import { getHostLayoutTokens } from '../../../utils/layout';
import ActrInputMobileView from './ActrInputMobileView';
import ActrInputPadView from './ActrInputPadView';
import { StableTextInput } from './StableTextInput';
import { useActrVoiceController } from './hooks/useActrVoiceController';
import { useActrMessageFlow } from './hooks/useActrMessageFlow';
import { LoginModal, AuthorizeModal } from '../../../system-apps/auth';

const { NativeLogger } = NativeModules;

interface ActrInputProps {
  appId: string;
  variant?: 'pad' | 'mobile';
  showTopToast?: (message: string) => void;
}

export default function ActrInput({
  appId,
  variant,
  showTopToast,
}: ActrInputProps) {
  const { tab } = useTab();
  const { width } = useWindowDimensions();
  const layoutTokens = React.useMemo(() => getHostLayoutTokens(width), [width]);
  const isMobileVariant =
    variant != null ? variant === 'mobile' : !layoutTokens.isTablet;

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

  const [showMapModal, setShowMapModal] = useState(false);
  const {
    attachments,
    removeAttachment,
    handleConfirmLocation,
    takePhoto,
    pickImage,
    pickDocument,
  } = useAttachmentManager(tab.id);

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

  const {
    showLoginModal,
    authParams,
    showAuthorizeModal,
    checkInputAuth,
    processMessageSend,
    handleSend,
    handleLoginModalClose,
    handleLoginSuccess,
    handleAuthorizeClose,
    handleAuthorizeSuccess,
    sendVoiceChunk,
    startVoice,
    endVoice,
    voiceMessageCallbacks,
  } = useActrMessageFlow({
    appId,
    tabId: tab.id,
    inputText,
    editingMessageId,
    attachments,
    isTablet: layoutTokens.isTablet,
    showTopToast,
  });

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
    onVoiceMessageReady: voiceMessageCallbacks.onVoiceMessageReady,
    onVoiceMessageCanceled: voiceMessageCallbacks.onVoiceMessageCanceled,
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
        <VoiceRecordingOverlay
          isCancelling={isCancellingVoice}
          metering={metering}
        />
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
          checkInputAuth={checkInputAuth}
        />
      ) : (
        <ActrInputPadView
          appId={appId}
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
          checkInputAuth={checkInputAuth}
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

      {isMobileVariant && showLoginModal && authParams && (
        <LoginModal
          visible
          onClose={handleLoginModalClose}
          clientId={authParams.clientId}
          appId={authParams.appId}
          onSuccess={handleLoginSuccess}
        />
      )}

      <AuthorizeModal
        visible={showAuthorizeModal}
        onClose={handleAuthorizeClose}
        appId={appId}
        onSuccess={handleAuthorizeSuccess}
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

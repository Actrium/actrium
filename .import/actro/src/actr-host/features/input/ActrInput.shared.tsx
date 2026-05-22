import React, { useRef, useState } from 'react';
import {
  View,
  StyleSheet,
  NativeModules,
  useWindowDimensions,
  type LayoutChangeEvent,
} from 'react-native';
import LocationPicker from '../extension/LocationPicker';
import { useAttachmentManager } from '../../hooks/useAttachmentManager';
import { useInputToolboxState } from '../../hooks/useInputToolboxState';
import { useInputStore } from '../../store/useInputStore';
import { VoiceRecordingOverlay } from '../shared/VoiceRecordingOverlay';
import { useTab } from '../../contexts/TabContext';
import { getHostLayoutTokens } from '../../../utils/layout';
import ActrInputMobileView from './ActrInputMobileView';
import ActrInputPadView from './ActrInputPadView';
import { StableTextInput } from './StableTextInput';
import { useActrSpeechRecognitionController } from './hooks/useActrSpeechRecognitionController';
import { useActrMessageFlow } from './hooks/useActrMessageFlow';
import { LoginModal, AuthorizeModal } from '../../../system-apps/auth';
import { getTypographyStyle } from '../../../theme/tokens';
import { getHostShadows } from '../../styles/shadows';
import { useColors } from '../../../theme/useColors';
import type { ColorTokens } from '../../../theme/colors';
import type { PhoneAttachmentPanelController } from '../chat/hooks/usePhoneAttachmentPanelController';

const { NativeLogger } = NativeModules;

interface ActrInputProps {
  appId: string;
  variant?: 'pad' | 'mobile';
  showTopToast?: (message: string) => void;
  mobileAttachmentController?: PhoneAttachmentPanelController;
  onLayout?: (event: LayoutChangeEvent) => void;
}

export default function ActrInput({
  appId,
  variant,
  showTopToast,
  mobileAttachmentController,
  onLayout,
}: ActrInputProps) {
  const { tab } = useTab();
  const { width } = useWindowDimensions();
  const layoutTokens = React.useMemo(() => getHostLayoutTokens(width), [width]);
  const isMobileVariant =
    variant != null ? variant === 'mobile' : !layoutTokens.isTablet;
  const colors = useColors();
  const styles = React.useMemo(() => makeStyles(colors), [colors]);

  const inputText = useInputStore((state) => state.inputs[tab.id]?.text || '');
  const editingMessageId = useInputStore(
    (state) => state.inputs[tab.id]?.editingMessageId,
  );
  const clearInputInStore = useInputStore((state) => state.clearInput);
  const setInputText = useInputStore((state) => state.setInputText);

  const handleCancelEdit = () => {
    // 中文注释：一次性清空文本、附件和编辑 ID，避免 Android 先退出编辑态再清文本造成焦点中间态。
    clearInputInStore(tab.id);
  };

  const [showMapModalLocal, setShowMapModalLocal] = useState(false);
  const attachmentManager = useAttachmentManager(tab.id);
  const toolboxState = useInputToolboxState(appId);

  const attachments = mobileAttachmentController?.attachments ?? attachmentManager.attachments;
  const removeAttachment = mobileAttachmentController?.removeAttachment ?? attachmentManager.removeAttachment;
  const handleConfirmLocation =
    mobileAttachmentController?.handleConfirmLocation ?? attachmentManager.handleConfirmLocation;
  const takePhoto = mobileAttachmentController?.takePhoto ?? attachmentManager.takePhoto;
  const pickImage = mobileAttachmentController?.pickImage ?? attachmentManager.pickImage;
  const pickDocument = mobileAttachmentController?.pickDocument ?? attachmentManager.pickDocument;
  const showMapModal = mobileAttachmentController?.showMapModal ?? showMapModalLocal;
  const setShowMapModal =
    mobileAttachmentController?.setShowMapModal ?? setShowMapModalLocal;

  const isWebSearchActive =
    mobileAttachmentController?.isWebSearchActive ?? toolboxState.isWebSearchActive;
  const isEnterForNewLine =
    mobileAttachmentController?.isEnterForNewLine ?? toolboxState.isEnterForNewLine;
  const showSearchEntry =
    mobileAttachmentController?.showSearchEntry ?? toolboxState.showSearchEntry;
  const showNewlineEntry =
    mobileAttachmentController?.showNewlineEntry ?? toolboxState.showNewlineEntry;
  const toolboxProps = toolboxState.toolboxProps;

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
  } = useActrMessageFlow({
    appId,
    tabId: tab.id,
    inputText,
    editingMessageId,
    attachments,
    showTopToast,
  });

  const handleClearInput = () => {
    setInputText(tab.id, '');
  };

  // 中文注释：Pad 端位置入口统一走这里，保持“打开位置选择器”是单一动作语义。
  const handleOpenPadLocationPicker = React.useCallback(() => {
    setShowMapModal(true);
  }, [setShowMapModal]);

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
  } = useActrSpeechRecognitionController({
    processMessageSend,
  });

  return (
    <View style={styles.rootContainer} onLayout={onLayout}>
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
          isAttachmentPanelOpen={
            mobileAttachmentController?.isAttachmentPanelOpen
          }
          focusInputRequestId={mobileAttachmentController?.focusInputRequestId}
          isEnterForNewLine={isEnterForNewLine}
          onToggleAttachmentPanel={
            mobileAttachmentController?.toggleAttachmentPanel
          }
          onCloseAttachmentPanel={
            mobileAttachmentController?.closeAttachmentPanel
          }
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
              isEnterForNewLine={isEnterForNewLine}
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
          onPickLocation={handleOpenPadLocationPicker}
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

      {!mobileAttachmentController && (
        <LocationPicker
          visible={showMapModal}
          onClose={() => setShowMapModal(false)}
          onSelect={(data) => {
            handleConfirmLocation(data);
            setShowMapModal(false);
          }}
        />
      )}

      {showLoginModal && authParams && (
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

const makeStyles = (colors: ColorTokens) => {
  const hostShadows = getHostShadows(colors);

  return StyleSheet.create({
  rootContainer: {
    width: '100%',
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
    backgroundColor: colors.bgInputPadPill,
    borderRadius: 20,
    paddingHorizontal: 13,
    paddingTop: 18,
    paddingBottom: 9,
    ...hostShadows.inputPill,
  },
  input: {
    ...getTypographyStyle('editorInput'),
    color: colors.textInput,
    minHeight: 24,
    textAlignVertical: 'top',
    padding: 0,
    marginBottom: 0,
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
    backgroundColor: colors.bgInputPill,
    justifyContent: 'center',
    alignItems: 'center',
    zIndex: 100,
    ...hostShadows.compactFloatingButton,
  },
  mobileInputText: {
    ...getTypographyStyle('editorInputCompact'),
    color: colors.textInput,
    padding: 0,
    textAlignVertical: 'top',
  },
});
};

export type ActrInputStyles = ReturnType<typeof makeStyles>;

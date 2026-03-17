import React from 'react';
import { Pressable, View } from 'react-native';
import { AttachmentPreviewBar } from '../AttachmentPreviewBar';
import { Icon } from '../Icon';
import { ActrInputPadToolbox } from './ActrInputPadToolbox';
import type { ActrInputStyles } from './ActrInput.shared';

interface ActrInputPadViewProps {
  styles: ActrInputStyles;
  layoutTokens: any;
  attachments: any[];
  removeAttachment: (id: string) => void;
  editingMessageId?: string | null;
  handleCancelEdit: () => void;
  textInputNode: React.ReactNode;
  takePhoto: () => void;
  pickImage: () => void;
  pickDocument: () => void;
  setShowMapModal: (visible: boolean) => void;
  handleSend: () => void;
  handleAudio: () => void;
  inputText: string;
  handleVoiceStart: () => Promise<void>;
  handleVoiceChangeState: (isCancelling: boolean) => void;
  handleVoiceEnd: (isCancelled: boolean) => Promise<void>;
  toolboxProps: Record<string, any>;
}

export default function ActrInputPadView(props: ActrInputPadViewProps) {
  const {
    styles,
    layoutTokens,
    attachments,
    removeAttachment,
    editingMessageId,
    handleCancelEdit,
    textInputNode,
    takePhoto,
    pickImage,
    pickDocument,
    setShowMapModal,
    handleSend,
    handleAudio,
    inputText,
    handleVoiceStart,
    handleVoiceChangeState,
    handleVoiceEnd,
    toolboxProps,
  } = props;

  return (
    <View
      style={[
        styles.container,
        {
          paddingHorizontal: layoutTokens.inputHorizontalPadding,
          paddingTop: layoutTokens.isTablet ? 8 : 8,
          paddingBottom: layoutTokens.inputVerticalPadding,
          maxWidth: layoutTokens.contentMaxWidth,
        },
      ]}
    >
      <View
        style={[
          styles.inputWrapper,
          {
            borderRadius: layoutTokens.isTablet ? 20 : 16,
            paddingHorizontal: layoutTokens.isTablet ? 13 : 11,
            paddingTop: layoutTokens.isTablet ? 16 : 14,
            paddingBottom: layoutTokens.isTablet ? 12 : 9,
          },
        ]}
      >
        <AttachmentPreviewBar
          attachments={attachments}
          removeAttachment={removeAttachment}
        />

        <View
          style={[
            styles.inputRow,
            {
              marginBottom: layoutTokens.isTablet ? 14 : 12,
            },
          ]}
        >
          {editingMessageId && (
            <Pressable
              onPress={handleCancelEdit}
              style={[
                styles.cancelEditButton,
                { left: layoutTokens.isTablet ? -55 : -45 },
              ]}
            >
              <Icon type='close' size={16} color='#999' />
            </Pressable>
          )}
          <View style={{ flex: 1 }}>
            {textInputNode}
          </View>
        </View>

        {/* Pad 端工具条（含长按语音按钮手势）单独收敛在 ActrInputPadToolbox。 */}
        <ActrInputPadToolbox
          onPickCamera={takePhoto}
          onPickImage={pickImage}
          onPickFile={pickDocument}
          onPickLocation={() => setShowMapModal(true)}
          onSend={handleSend}
          onAudio={handleAudio}
          showAudioButton={!inputText}
          onVoiceStart={handleVoiceStart}
          onVoiceChangeState={handleVoiceChangeState}
          onVoiceEnd={handleVoiceEnd}
          {...(toolboxProps as any)}
        />
      </View>
    </View>
  );
}

import React from 'react';
import { Pressable, View } from 'react-native';
import { AttachmentPreviewBar } from './AttachmentPreviewBar';
import { Icon } from '../../../components';
import { ActrInputPadToolbox } from './ActrInputPadToolbox';
import { AuthGateOverlay } from './AuthGateOverlay';
import type { ActrInputStyles } from './ActrInput.shared';
import { useColors } from '../../../theme/useColors';

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
  onPickLocation: () => void;
  handleSend: () => void;
  handleAudio: () => void;
  inputText: string;
  handleVoiceStart: () => Promise<void>;
  handleVoiceChangeState: (isCancelling: boolean) => void;
  handleVoiceEnd: (isCancelled: boolean) => Promise<void>;
  toolboxProps: Record<string, any>;
  appId: string;
  /** Enter pre-checking for authorization before focus, not authorized to return false */
  checkInputAuth?: () => Promise<boolean>;
}

export default function ActrInputPadView(props: ActrInputPadViewProps) {
  const colors = useColors();
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
    onPickLocation,
    handleSend,
    handleAudio,
    inputText,
    handleVoiceStart,
    handleVoiceChangeState,
    handleVoiceEnd,
    toolboxProps,
    appId,
    checkInputAuth,
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
          // backgroundColor: colors.bgInputPadPill,
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
            backgroundColor: colors.bgInputPill
          },
        ]}
      >
        {/* Comment in Chinese:Pad The whole area of the end block (including attachments, input boxes, toolbars, hold-downs) is covered by the authorized door. */}
        <AuthGateOverlay
          appId={appId}
          checkInputAuth={checkInputAuth}
        />

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
              <Icon type='close' size={16} color={colors.textTertiary} />
            </Pressable>
          )}
          <View style={{ flex: 1 }}>
            {textInputNode}
          </View>
        </View>

        {/* Pad End toolbar (with long voice button gestures) condense in a separate position ActrInputPadToolbox. */}
        <ActrInputPadToolbox
          onPickCamera={takePhoto}
          onPickImage={pickImage}
          onPickFile={pickDocument}
          onPickLocation={onPickLocation}
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

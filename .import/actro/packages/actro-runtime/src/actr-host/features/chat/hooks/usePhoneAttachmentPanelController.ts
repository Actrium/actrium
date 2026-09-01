import React from 'react';
import { useAttachmentManager } from '../../../hooks/useAttachmentManager';
import { useInputToolboxState } from '../../../hooks/useInputToolboxState';

export interface PhoneAttachmentPanelController {
  attachments: ReturnType<typeof useAttachmentManager>['attachments'];
  removeAttachment: ReturnType<typeof useAttachmentManager>['removeAttachment'];
  takePhoto: () => void;
  pickImage: () => void;
  pickDocument: () => void;
  pickLocation: () => void;
  handleConfirmLocation: ReturnType<typeof useAttachmentManager>['handleConfirmLocation'];
  showMapModal: boolean;
  setShowMapModal: React.Dispatch<React.SetStateAction<boolean>>;
  isAttachmentPanelOpen: boolean;
  closeAttachmentPanel: () => void;
  toggleAttachmentPanel: () => void;
  isWebSearchActive: boolean;
  isEnterForNewLine: boolean;
  showSearchEntry: boolean;
  showNewlineEntry: boolean;
  toggleWebSearch: () => void;
  toggleEnterForNewLine: () => void;
  focusInputRequestId: number;
}

export const usePhoneAttachmentPanelController = (
  appId: string,
  tabId: string,
): PhoneAttachmentPanelController => {
  const [showMapModal, setShowMapModal] = React.useState(false);
  const [isAttachmentPanelOpen, setIsAttachmentPanelOpen] = React.useState(false);
  const [focusInputRequestId, setFocusInputRequestId] = React.useState(0);
  const {
    attachments,
    removeAttachment,
    handleConfirmLocation,
    takePhoto,
    pickImage,
    pickDocument,
  } = useAttachmentManager(tabId);
  const {
    isWebSearchActive,
    isEnterForNewLine,
    showSearchEntry,
    showNewlineEntry,
    toggleWebSearch,
    toggleEnterForNewLine,
  } = useInputToolboxState(appId);

  const prevAttachmentsRef = React.useRef(attachments);

  const closeAttachmentPanel = React.useCallback(() => {
    setIsAttachmentPanelOpen(false);
  }, []);

  // 选中附件后自动关闭面板，取消选择则保持打开
  React.useEffect(() => {
    if (!isAttachmentPanelOpen) {
      prevAttachmentsRef.current = attachments;
      return;
    }
    if (prevAttachmentsRef.current !== attachments) {
      prevAttachmentsRef.current = attachments;
      closeAttachmentPanel();
      setFocusInputRequestId((value) => value + 1);
    }
  }, [attachments, isAttachmentPanelOpen, closeAttachmentPanel]);

  const toggleAttachmentPanel = React.useCallback(() => {
    setIsAttachmentPanelOpen((prev) => !prev);
  }, []);

  const handlePickCamera = React.useCallback(() => {
    takePhoto();
  }, [takePhoto]);

  const handlePickImage = React.useCallback(() => {
    pickImage();
  }, [pickImage]);

  const handlePickDocument = React.useCallback(() => {
    pickDocument();
  }, [pickDocument]);

  const handlePickLocation = React.useCallback(() => {
    setShowMapModal(true);
  }, []);

  const handleToggleWebSearch = React.useCallback(() => {
    closeAttachmentPanel();
    toggleWebSearch();
  }, [closeAttachmentPanel, toggleWebSearch]);

  const handleToggleEnterForNewLine = React.useCallback(() => {
    closeAttachmentPanel();
    toggleEnterForNewLine();
  }, [closeAttachmentPanel, toggleEnterForNewLine]);

  return {
    attachments,
    removeAttachment,
    takePhoto: handlePickCamera,
    pickImage: handlePickImage,
    pickDocument: handlePickDocument,
    pickLocation: handlePickLocation,
    handleConfirmLocation,
    showMapModal,
    setShowMapModal,
    isAttachmentPanelOpen,
    closeAttachmentPanel,
    toggleAttachmentPanel,
    isWebSearchActive,
    isEnterForNewLine,
    showSearchEntry,
    showNewlineEntry,
    toggleWebSearch: handleToggleWebSearch,
    toggleEnterForNewLine: handleToggleEnterForNewLine,
    focusInputRequestId,
  };
};

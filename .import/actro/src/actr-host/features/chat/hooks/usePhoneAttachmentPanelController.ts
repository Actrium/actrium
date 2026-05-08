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
}

export const usePhoneAttachmentPanelController = (
  appId: string,
  tabId: string,
): PhoneAttachmentPanelController => {
  const [showMapModal, setShowMapModal] = React.useState(false);
  const [isAttachmentPanelOpen, setIsAttachmentPanelOpen] = React.useState(false);
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

  const closeAttachmentPanel = React.useCallback(() => {
    setIsAttachmentPanelOpen(false);
  }, []);

  const toggleAttachmentPanel = React.useCallback(() => {
    setIsAttachmentPanelOpen((prev) => !prev);
  }, []);

  const handlePickCamera = React.useCallback(() => {
    closeAttachmentPanel();
    takePhoto();
  }, [closeAttachmentPanel, takePhoto]);

  const handlePickImage = React.useCallback(() => {
    closeAttachmentPanel();
    pickImage();
  }, [closeAttachmentPanel, pickImage]);

  const handlePickDocument = React.useCallback(() => {
    closeAttachmentPanel();
    pickDocument();
  }, [closeAttachmentPanel, pickDocument]);

  const handlePickLocation = React.useCallback(() => {
    closeAttachmentPanel();
    setShowMapModal(true);
  }, [closeAttachmentPanel]);

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
  };
};

import { useCallback, useEffect, useState } from 'react';
import {
  ExtensionEvents,
  type ExtensionPanelVisibleChangeEvent,
} from '../../native-bridge';
import { isAndroid, isTablet } from '../../utils/device';

interface UseExtensionPanelVisibilityParams {
  tabId: string;
  isLeftPanelVisible: boolean;
}

/**
 * Integrated management of the source of the hidden status of the extended panel.
 * - Phone: Inner state drive, response button/Gestures/Mask Click
 * - iOS Pad: Re-semble from parent component props
 * - Android Pad: changed to listen to the hidden event of the native extension panel
 */
export function useExtensionPanelVisibility({
  tabId,
  isLeftPanelVisible,
}: UseExtensionPanelVisibilityParams) {
  // Phone The extension panel of the end is self-controlled by the current page.
  const [phonePanelVisible, setPhonePanelVisible] = useState(false);
  // Android Pad First Screen First props Align the current status, follow up native event.
  const [androidTabletPanelVisible, setAndroidTabletPanelVisible] =
    useState(isLeftPanelVisible);

  useEffect(() => {
    if (!isAndroid || !isTablet) {
      return;
    }

    // Android Pad The extension panel switch is dominated by the original side, which is responsible for only receiving events and synchronizing them. React Status.
    return ExtensionEvents.onExtensionPanelVisibleChange(
      ({ visible, tabId: eventTabId }: ExtensionPanelVisibleChangeEvent) => {
        // The original event will be broadcast to all. tab, only current tab Consuming their own hidden changes.
        if (eventTabId !== tabId) {
          return;
        }
        setAndroidTabletPanelVisible(visible);
      },
    );
  }, [tabId]);

  useEffect(() => {
    if (!isAndroid || !isTablet) {
      return;
    }

    // Before first mount native The event may not have been sent, keep it. props Synchronization avoids flashing of the front frame.
    setAndroidTabletPanelVisible(isLeftPanelVisible);
  }, [isLeftPanelVisible]);

  const handlePhonePanelVisibleChange = useCallback((visible: boolean) => {
    // Phone The end buttons and gestures are eventually constricted to this point and multiple decentralization is avoided.
    setPhonePanelVisible(visible);
  }, []);

  const togglePhonePanelVisible = useCallback(() => {
    setPhonePanelVisible((prev) => !prev);
  }, []);

  const closePhonePanel = useCallback(() => {
    setPhonePanelVisible(false);
  }, []);

  let panelVisible = isLeftPanelVisible;
  if (!isTablet) {
    panelVisible = phonePanelVisible;
  } else if (isAndroid) {
    panelVisible = androidTabletPanelVisible;
  }

  return {
    panelVisible,
    handlePhonePanelVisibleChange,
    togglePhonePanelVisible,
    closePhonePanel,
  };
}

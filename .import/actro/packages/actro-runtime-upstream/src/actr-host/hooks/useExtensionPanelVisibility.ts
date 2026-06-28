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
 * 统一管理扩展面板显隐状态来源。
 * - Phone：组件内状态驱动，响应按钮/手势/遮罩点击
 * - iOS Pad：沿用父组件下发的 props
 * - Android Pad：改为监听原生扩展面板显隐事件
 */
export function useExtensionPanelVisibility({
  tabId,
  isLeftPanelVisible,
}: UseExtensionPanelVisibilityParams) {
  // Phone 端的扩展面板由当前页面自己控制显隐。
  const [phonePanelVisible, setPhonePanelVisible] = useState(false);
  // Android Pad 首屏先使用 props 对齐当前状态，后续以 native 事件为准同步变化。
  const [androidTabletPanelVisible, setAndroidTabletPanelVisible] =
    useState(isLeftPanelVisible);

  useEffect(() => {
    if (!isAndroid || !isTablet) {
      return;
    }

    // Android Pad 的扩展面板开关由原生侧主导，这里只负责接收事件并同步到 React 状态。
    return ExtensionEvents.onExtensionPanelVisibleChange(
      ({ visible, tabId: eventTabId }: ExtensionPanelVisibleChangeEvent) => {
        // 原生事件会广播给所有 tab，只允许当前 tab 消费自己的显隐变化。
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

    // 首次挂载前 native 事件可能尚未派发，保留 props 同步可避免首帧状态闪动。
    setAndroidTabletPanelVisible(isLeftPanelVisible);
  }, [isLeftPanelVisible]);

  const handlePhonePanelVisibleChange = useCallback((visible: boolean) => {
    // Phone 端的按钮和手势最终都收敛到这一处，避免多处分散写状态切换。
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

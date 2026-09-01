import React from 'react';
import { isTablet } from '../../../../utils/device';
import TabletExtensionPanel from './TabletExtensionPanel';
import PhoneExtensionPanel from './PhoneExtensionPanel';
import type { ExtensionPanelProps } from './types';

/**
 * 扩展面板 - 根据设备类型自动选择实现
 * - Tablet: 右侧滑入，覆盖在上层（无手势）
 * - Phone: 左侧滑入，推动中间窗口（手势由父组件 GestureDetector 控制）
 */
const ExtensionPanel: React.FC<ExtensionPanelProps> = (props) => {
  if (isTablet) {
    return (
      <TabletExtensionPanel
        visible={props.visible}
        content={props.content}
        width={props.width}
        centerAnimValue={props.centerAnimValue}
      />
    );
  }

  return (
    <PhoneExtensionPanel
      visible={props.visible}
      content={props.content}
      width={props.width}
      centerAnimValue={props.centerAnimValue}
      slideAnim={props.slideAnim}
      isGesturing={props.isGesturing}
    />
  );
};

export default ExtensionPanel;

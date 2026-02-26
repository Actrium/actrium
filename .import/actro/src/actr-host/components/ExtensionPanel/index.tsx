import React from 'react';
import { useWindowDimensions } from 'react-native';
import { isTabletWidth } from '../../../utils/device';
import TabletExtensionPanel from './TabletExtensionPanel';
import PhoneExtensionPanel from './PhoneExtensionPanel';
import type { ExtensionPanelProps } from './types';

/**
 * 扩展面板 - 根据设备类型自动选择实现
 * - Tablet: 右侧滑入，覆盖在上层
 * - Phone: 左侧滑入，推动中间窗口
 */
const ExtensionPanel: React.FC<ExtensionPanelProps> = (props) => {
  const { width } = useWindowDimensions();
  const isTablet = isTabletWidth(width);

  return isTablet ? (
    <TabletExtensionPanel {...props} />
  ) : (
    <PhoneExtensionPanel {...props} />
  );
};

export default ExtensionPanel;

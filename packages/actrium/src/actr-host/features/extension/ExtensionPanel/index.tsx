import React from 'react';
import { isTablet } from '../../../../utils/device';
import TabletExtensionPanel from './TabletExtensionPanel';
import PhoneExtensionPanel from './PhoneExtensionPanel';
import type { ExtensionPanelProps } from './types';

/**
 * Extension Panel - Autoselect to achieve according to the type of device
 * - Tablet: Right slide, overlay upper layer (no gesture)
 * - Phone: Slide left, push the middle window (hand gestures are parent-based) GestureDetector Control)
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

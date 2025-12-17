import React from 'react';
import { View } from 'react-native';

export type PanelId = 'left' | 'right';

export type PanelMarkerProps = {
  panelId: PanelId;
  children?: React.ReactNode;
};

export const PanelMarker: React.FC<PanelMarkerProps> = ({ panelId, children }) => (
  // Pass panelId to host element for extractPanels to identify left/right panels
  <View style={{ flex: 1 }} panelId={panelId}>
    {children}
  </View>
);

PanelMarker.displayName = 'PanelMarker';

import React from 'react';
import { View } from 'react-native';

export type PanelId = 'left' | 'right';

export type PanelMarkerProps = {
  panelId: PanelId;
  children?: React.ReactNode;
};

export const PanelMarker: React.FC<PanelMarkerProps> = ({ children }) => (
  <View style={{ flex: 1 }}>{children}</View>
);

PanelMarker.displayName = 'PanelMarker';


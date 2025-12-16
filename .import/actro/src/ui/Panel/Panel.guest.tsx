import React from 'react';

type PanelSide = 'left' | 'right';

export type PanelProps = {
  children?: React.ReactNode;
};

type PanelMarkerProps = {
  panelId: PanelSide;
  children?: React.ReactNode;
};

const PanelMarker = 'PanelMarker' as unknown as React.ElementType<PanelMarkerProps>;

function createPanel(side: PanelSide): React.FC<PanelProps> {
  const PanelSideComponent: React.FC<PanelProps> = ({ children }) => (
    <PanelMarker panelId={side}>{children}</PanelMarker>
  );
  PanelSideComponent.displayName = `Panel.${side === 'left' ? 'Left' : 'Right'}`;
  return PanelSideComponent;
}

export const Panel = {
  Left: createPanel('left'),
  Right: createPanel('right'),
};


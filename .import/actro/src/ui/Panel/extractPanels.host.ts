import React from 'react';

export type ExtractedPanels = {
  left: React.ReactElement | null;
  right: React.ReactElement | null;
};

function isPanelMarkerElement(element: React.ReactElement): boolean {
  if (element.type === 'PanelMarker') return true;

  const type = element.type as unknown as { displayName?: string; name?: string };
  const name = type?.displayName ?? type?.name;
  return name === 'PanelMarker';
}

function findPanel(
  element: React.ReactElement,
  targetPanelId: 'left' | 'right'
): React.ReactElement | null {
  if (isPanelMarkerElement(element)) {
    const props = element.props as { panelId?: string; children?: React.ReactNode } | null;
    if (props?.panelId === targetPanelId) {
      return (props.children as React.ReactElement) ?? null;
    }
  }

  const props = element.props as { children?: React.ReactNode } | null;
  const children = props?.children;
  if (!children) return null;

  for (const child of React.Children.toArray(children)) {
    if (React.isValidElement(child)) {
      const found = findPanel(child, targetPanelId);
      if (found) return found;
    }
  }

  return null;
}

export function extractPanels(tree: React.ReactElement | string | null): ExtractedPanels {
  if (!tree || typeof tree === 'string') return { left: null, right: null };
  return {
    left: findPanel(tree, 'left'),
    right: findPanel(tree, 'right'),
  };
}


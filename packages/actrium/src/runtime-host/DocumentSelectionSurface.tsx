import React, { createContext, useEffect, useMemo } from 'react';
import { DeviceEventEmitter, NativeModules, View, type GestureResponderEvent } from 'react-native';
import { createDocumentSelection } from './document-selection';

export const DocumentSelectionContext = createContext<ReturnType<typeof createDocumentSelection> | null>(null);

export function DocumentSelectionSurface({ tabId, writeText, children }: {
  tabId: string;
  writeText(text: string): void;
  children: React.ReactNode;
}) {
  const selection = useMemo(() => createDocumentSelection(available => {
    NativeModules.DocumentSelection?.setAvailable?.(tabId, available);
  }), [tabId]);

  useEffect(() => {
    const sub = DeviceEventEmitter.addListener('actriumDocumentSelection', event => {
      if (event?.tabId !== tabId) return;
      if (event.command === 'clear') selection.clear();
      if (event.command === 'copy') selection.copy(writeText);
    });
    return () => {
      sub.remove();
      selection.clear();
    };
  }, [selection, tabId, writeText]);

  // Capture runs before a sibling document can start its next drag.
  const capture = {
    onTouchStartCapture(event: GestureResponderEvent) {
      if ((event.nativeEvent as { button?: number }).button === 0) selection.clear();
    },
  };
  return (
    <DocumentSelectionContext.Provider value={selection}>
      <View collapsable={false} style={{ flex: 1 }} {...capture}>{children}</View>
    </DocumentSelectionContext.Provider>
  );
}

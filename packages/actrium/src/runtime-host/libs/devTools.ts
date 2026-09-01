import { NativeModules } from 'react-native';
import { Engine } from 'keel/host';

export type BoxModelData = {
  frame: { x: number; y: number; width: number; height: number };
  margin: { top: number; right: number; bottom: number; left: number };
  padding: { top: number; right: number; bottom: number; left: number };
  border?: { top: number; right: number; bottom: number; left: number };
  style?: {
    position?: string;
    backgroundColor?: string;
    top?: number;
    left?: number;
    right?: number;
    bottom?: number;
    display?: string;
    flexDirection?: string;
    alignItems?: string;
    justifyContent?: string;
    flex?: number;
    opacity?: number;
    overflow?: string;
  };
};

export const DevToolsBridge = NativeModules.DevToolsBridge as
  | {
      logMessage?: (
        direction: string,
        event: string,
        payload: string | null,
        tabId: string | null
      ) => void;
      logRenderOp?: (
        operationType: string,
        componentType: string | null,
        nodeId: string | null,
        payload: string | null,
        tabId: string | null
      ) => void;
      setEnginePaused?: (isPaused: boolean) => void;
      registerPauseHandler?: () => void;
      updateComponentTree?: (
        treeJson: string | null,
        tabId: string | null
      ) => void;
      updateEngineMonitor?: (snapshotJson: string | null) => void;
      setInspectMode?: (enabled: boolean) => void;
      setRNInspectorActive?: (active: boolean) => void;
      highlightNode?: (
        nodeId: string | null,
        frame: { x: number; y: number; width: number; height: number } | null
      ) => void;
      clearHighlight?: () => void;
      selectNodeFromInspect?: (nodeId: string) => void;
      updateBoxModel?: (data: BoxModelData) => void;
      addListener?: (eventName: string) => void;
      removeListeners?: (count: number) => void;
    }
  | undefined;

export function logDevToolsMessage(
  direction: 'guestToHost' | 'hostToGuest',
  event: string,
  payload: unknown,
  tabId?: string
) {
  try {
    DevToolsBridge?.logMessage?.(
      direction,
      event,
      payload ? JSON.stringify(payload) : null,
      tabId ?? null
    );
  } catch {
    // Ignore DevTools logging errors
  }
}

export function logDevToolsRenderOp(
  operationType: string,
  componentType: string | undefined,
  nodeId: string | undefined,
  props: unknown,
  tabId?: string
) {
  try {
    DevToolsBridge?.logRenderOp?.(
      operationType,
      componentType ?? null,
      nodeId ?? null,
      props ? JSON.stringify(props) : null,
      tabId ?? null
    );
  } catch {
    // Ignore DevTools logging errors
  }
}

export function updateDevToolsComponentTree(engine: Engine | null, tabId?: string) {
  try {
    if (!engine) {
      DevToolsBridge?.updateComponentTree?.(null, tabId ?? null);
      return;
    }

    const receiver = engine.getReceiver();
    if (!receiver) {
      DevToolsBridge?.updateComponentTree?.(null, tabId ?? null);
      return;
    }

    // Get the component tree from Receiver
    const tree = receiver.getComponentTree?.();
    if (!tree) {
      DevToolsBridge?.updateComponentTree?.(null, tabId ?? null);
      return;
    }

    // Serialize and send to DevTools
    const treeJson = JSON.stringify(tree);
    DevToolsBridge?.updateComponentTree?.(treeJson, tabId ?? null);
  } catch {
    // Ignore DevTools errors
  }
}

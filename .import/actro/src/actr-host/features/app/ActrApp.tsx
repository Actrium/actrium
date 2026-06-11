/**
 * ActrApp v8 - Single Engine Architecture with usePanels Hook Pattern
 * 单 Engine 架构 + usePanels hook 模式
 *
 * 改进：
 * - ✅ 单个 Engine 加载统一 bundle
 * - ✅ Guest 导出 usePanels hook，AUTO_RENDER_FOOTER 调用并渲染
 * - ✅ Host 通过 __panelId 属性识别面板
 * - ✅ Host 控制布局（三栏固定-flex-固定）
 * - ✅ 与 CounterMacOS 模式一致
 *
 * @version 8.0.0 - Single Engine + usePanels Hook Pattern
 */
import React, { useCallback, useRef, useState, useEffect } from 'react';
import {
  View,
  NativeModules,
  NativeEventEmitter,
  StyleSheet,
  findNodeHandle,
  UIManager,
  DeviceEventEmitter,
  Platform,
} from 'react-native';
import { useUnreadStore } from '../../store/useUnreadStore';
import { AppText } from '../../../components';
import { getTypographyStyle, radius, spacing } from '../../../theme/tokens';
import { useColors } from '../../../theme/useColors';
import type { ColorTokens } from '../../../theme/colors';

// Import RN's Inspector API for accurate view hit testing
// This uses React's internal renderer to find views at a point
const getInspectorDataForViewAtPoint =
  require('react-native/src/private/inspector/getInspectorDataForViewAtPoint')
    .default as (
    inspectedView: unknown,
    locationX: number,
    locationY: number,
    callback: (viewData: {
      hierarchy: Array<{
        name: string;
        getInspectorData: (
          findNodeHandle: (component: unknown) => number | null,
        ) => {
          measure: (
            callback: (
              x: number,
              y: number,
              width: number,
              height: number,
              left: number,
              top: number,
            ) => void,
          ) => void;
          props: Record<string, unknown>;
        };
      }>;
      props: Record<string, unknown>;
      selectedIndex: number;
      frame: { left: number; top: number; width: number; height: number };
      pointerY: number;
      touchedViewTag: number;
      closestInstance: unknown;
    }) => boolean,
  ) => void;
import { logger } from '../../../logger';
import { installConsoleInterceptor } from '../../../devtools-console';
import { Engine } from 'rill';

// Install console interceptor to forward logs to DevTools Console panel
installConsoleInterceptor();
import type { ContractViolation } from 'askit/contracts';
import { createContractViolationCollector } from 'askit/contracts';
import type { ActrProps, TabConfig } from '../../types';
import { DevToolsBridge } from '../../libs/devTools';
import ActrTabView from './ActrTabView';
import { ContainerEvents } from '../../../native-bridge';
import { LAYOUT } from '../../constants';
import { evaluateTabGate, extractBoxModelFromStyle } from '../../utils';
import { isAndroid } from '../../../utils/device';

// 模块级别的 props 缓存
let cachedValidProps: ActrProps | null = null;

const SUPPORTED_CONTRACT = { name: 'ask', version: 1 } as const;

export const ActrApp: React.FC<ActrProps> = (props) => {
  // Props 稳定化
  const hasValidTabs = props?.tabs && props.tabs.length > 0;
  let effectiveProps = props;

  if (hasValidTabs) {
    cachedValidProps = { ...props };
  } else if (cachedValidProps) {
    effectiveProps = cachedValidProps;
  }

  const {
    tabs = [],
    currentTabId,
    isLeftPanelVisible = false,
    containerWidth: propsContainerWidth,
    containerHeight: propsContainerHeight,
  } = effectiveProps || {};

  const colors = useColors();
  const styles = React.useMemo(() => makeStyles(colors), [colors]);

  if (!tabs.length) {
    return <View style={styles.empty} />;
  }

  const activeTabId = currentTabId ?? tabs[0]!.id;
  const hasReceivedContainerSizeEventRef = useRef(false);
  // 安卓需要自己维护容器尺寸 通过 onContainerSizeChange 监听 native 回调
  // ios 直接使用 props.containerWidth/Height
  const [androidContainerSize, setAndroidContainerSize] = useState(() => ({
    containerWidth: propsContainerWidth,
    containerHeight: propsContainerHeight,
  }));

  useEffect(() => {
    if (Platform.OS !== 'android') {
      return;
    }
    if (hasReceivedContainerSizeEventRef.current) {
      return;
    }
    setAndroidContainerSize({
      containerWidth: propsContainerWidth,
      containerHeight: propsContainerHeight,
    });
  }, [propsContainerWidth, propsContainerHeight]);

  useEffect(() => {
    if (!isAndroid) {
      return;
    }
    // Android 先吃首屏 props，后续完全以宿主尺寸变更事件为准，
    // 避免宿主 RootView 改变后 RN 仍停留在旧宽度。
    const unScribe = ContainerEvents.onContainerSizeChange(
      ({ containerWidth, containerHeight }) => {
        console.log('收到容器尺寸变化', containerWidth, containerHeight);
        hasReceivedContainerSizeEventRef.current = true;
        setAndroidContainerSize({ containerWidth, containerHeight });
      },
    );
    return unScribe;
  }, []);

  const containerWidth =
    Platform.OS === 'android'
      ? androidContainerSize.containerWidth
      : propsContainerWidth;
  const containerHeight =
    Platform.OS === 'android'
      ? androidContainerSize.containerHeight
      : propsContainerHeight;

  const monitorTabs = React.useMemo(
    () =>
      tabs.map((t) => {
        const gate = evaluateTabGate(t);
        return { ...t, status: gate.status, blockedReason: gate.blockedReason };
      }),
    [tabs],
  );

  const engineMapRef = useRef<Map<string, Engine>>(new Map());
  const contractCollectorsRef = useRef<
    Map<string, ReturnType<typeof createContractViolationCollector>>
  >(new Map());

  const recordContractViolation = useCallback(
    (tabId: string, violation: ContractViolation) => {
      let collector = contractCollectorsRef.current.get(tabId);
      if (!collector) {
        collector = createContractViolationCollector({ max: 50 });
        contractCollectorsRef.current.set(tabId, collector);
      }
      collector.record(violation);
    },
    [],
  );

  // FIXME 应用启动时从数据库同步未读消息数量
  // useEffect(() => {
  //   useUnreadStore.getState().syncFromDatabase();
  // }, []);

  const handleEngineChange = useCallback(
    (tabId: string, next: Engine | null) => {
      if (next) {
        engineMapRef.current.set(tabId, next);
      } else {
        engineMapRef.current.delete(tabId);
        contractCollectorsRef.current.delete(tabId);
      }
    },
    [],
  );

  // Inspect mode state
  const [isInspectMode, setIsInspectMode] = useState(false);

  // Highlighted node frame for bidirectional linking
  const [highlightedFrame, setHighlightedFrame] = useState<{
    x: number;
    y: number;
    width: number;
    height: number;
    nodeId: string;
    componentName?: string;
  } | null>(null);

  // Node frames cache for hit testing
  const nodeFramesRef = useRef<
    Map<string, { x: number; y: number; width: number; height: number }>
  >(new Map());

  // Bidirectional mapping: viewTag <-> nodeId
  const viewTagToNodeIdRef = useRef<Map<number, string>>(new Map());
  const nodeIdToViewTagRef = useRef<Map<string, number>>(new Map());

  // Build viewTag <-> nodeId mapping and measure all frames
  const measureAllNodeFrames = useCallback((engine: Engine | null) => {
    console.log(
      '[DevTools] measureAllNodeFrames called, engine:',
      engine ? 'exists' : 'null',
    );
    if (!engine) {
      console.log('[DevTools] No engine for measureAllNodeFrames');
      return;
    }

    const receiver = engine.getReceiver();
    console.log('[DevTools] receiver:', receiver ? 'exists' : 'null');
    if (!receiver) {
      console.log('[DevTools] No receiver for measureAllNodeFrames');
      return;
    }

    const allRefs = receiver.getAllNodeRefs?.();
    console.log(
      '[DevTools] getAllNodeRefs available:',
      !!receiver.getAllNodeRefs,
    );
    if (!allRefs) {
      console.log('[DevTools] No refs from getAllNodeRefs');
      return;
    }

    console.log(
      '[DevTools] Building viewTag mapping for',
      allRefs.size,
      'nodes',
    );
    const frames = new Map<
      string,
      { x: number; y: number; width: number; height: number }
    >();
    const viewTagToNodeId = new Map<number, string>();
    const nodeIdToViewTag = new Map<string, number>();

    let refCount = 0;
    let viewTagCount = 0;
    allRefs.forEach((ref: React.RefObject<unknown>, nodeId: number) => {
      if (!ref.current) {
        return;
      }
      refCount++;

      // Get native view tag for this node
      const viewTag = findNodeHandle(ref.current as React.Component);
      if (viewTag) {
        viewTagCount++;
        viewTagToNodeId.set(viewTag, String(nodeId));
        nodeIdToViewTag.set(String(nodeId), viewTag);
        console.log('[DevTools] Mapped viewTag', viewTag, '-> nodeId', nodeId);
      }

      // Try measuring the node
      const component = ref.current as {
        measure?: (
          callback: (
            x: number,
            y: number,
            width: number,
            height: number,
            pageX: number,
            pageY: number,
          ) => void,
        ) => void;
      };
      if (typeof component.measure === 'function') {
        component.measure((x, y, width, height, pageX, pageY) => {
          if (width > 0 && height > 0) {
            frames.set(String(nodeId), { x: pageX, y: pageY, width, height });
          }
        });
      }
    });

    console.log(
      '[DevTools] Refs with .current:',
      refCount,
      'ViewTags found:',
      viewTagCount,
    );

    // Update mappings
    viewTagToNodeIdRef.current = viewTagToNodeId;
    nodeIdToViewTagRef.current = nodeIdToViewTag;
    console.log(
      '[DevTools] Built viewTag mapping:',
      viewTagToNodeId.size,
      'entries',
    );

    // Update frames after a short delay to let measurements complete
    setTimeout(() => {
      nodeFramesRef.current = frames;
      console.log('[DevTools] Frames cached:', frames.size, 'entries');
    }, 100);
  }, []);

  // Measure single node frame and highlight it
  const measureAndHighlightNode = useCallback(
    (
      nodeId: string,
      engine: Engine | null,
      onMeasured?: (frame: {
        x: number;
        y: number;
        width: number;
        height: number;
      }) => void,
    ) => {
      console.log(
        '[DevTools] measureAndHighlightNode called with nodeId:',
        nodeId,
      );
      if (!engine) {
        console.log('[DevTools] No engine available');
        return;
      }

      const receiver = engine.getReceiver();
      if (!receiver) {
        console.log('[DevTools] No receiver available');
        return;
      }

      const ref = receiver.getNodeRef?.(Number(nodeId));
      console.log(
        '[DevTools] Got ref for node:',
        nodeId,
        'ref:',
        ref,
        'current:',
        ref?.current,
      );
      if (!ref?.current) {
        console.log('[DevTools] No ref.current, clearing highlight');
        setHighlightedFrame(null);
        return;
      }

      const handleMeasureResult = (
        x: number,
        y: number,
        width: number,
        height: number,
        pageX: number,
        pageY: number,
      ) => {
        if (width > 0 && height > 0) {
          const frame = { x: pageX, y: pageY, width, height };
          setHighlightedFrame({ ...frame, nodeId });
          DevToolsBridge?.highlightNode?.(nodeId, frame);
          // Store in nodeFramesRef for future use
          nodeFramesRef.current.set(nodeId, frame);
          // Call callback with measured frame
          onMeasured?.(frame);
        } else {
          console.log('[DevTools] Invalid dimensions, clearing highlight');
          setHighlightedFrame(null);
          DevToolsBridge?.clearHighlight?.();
        }
      };

      // Try using component's measure method directly (more reliable on macOS)
      const component = ref.current as {
        measure?: (
          callback: (
            x: number,
            y: number,
            width: number,
            height: number,
            pageX: number,
            pageY: number,
          ) => void,
        ) => void;
      };
      if (typeof component.measure === 'function') {
        console.log('[DevTools] Using component.measure directly');
        component.measure((x, y, width, height, pageX, pageY) => {
          console.log('[DevTools] component.measure result:', {
            x,
            y,
            width,
            height,
            pageX,
            pageY,
          });
          handleMeasureResult(x, y, width, height, pageX, pageY);
        });
        return;
      }

      // Fallback to UIManager.measure
      console.log('[DevTools] Fallback to UIManager.measure');
      const handle = findNodeHandle(ref.current as React.Component);
      console.log('[DevTools] findNodeHandle result:', handle);
      if (!handle) {
        console.log('[DevTools] No handle, clearing highlight');
        setHighlightedFrame(null);
        return;
      }

      UIManager.measure(handle, (x, y, width, height, pageX, pageY) => {
        console.log('[DevTools] UIManager.measure result:', {
          x,
          y,
          width,
          height,
          pageX,
          pageY,
        });
        handleMeasureResult(x, y, width, height, pageX, pageY);
      });
    },
    [],
  );

  // Find deepest node at given position
  const findNodeAtPosition = useCallback(
    (x: number, y: number, engine: Engine | null): string | null => {
      console.log('[DevTools] findNodeAtPosition called at:', x, y);
      if (!engine) {
        console.log('[DevTools] No engine');
        return null;
      }

      const receiver = engine.getReceiver();
      if (!receiver) {
        console.log('[DevTools] No receiver');
        return null;
      }

      const tree = receiver.getComponentTree?.();
      if (!tree) {
        console.log('[DevTools] No tree');
        return null;
      }

      // Build frames map synchronously from cached values
      const frames = nodeFramesRef.current;
      console.log('[DevTools] Cached frames count:', frames.size);

      // Find deepest node containing the point
      let deepestNode: string | null = null;
      let smallestArea = Infinity;

      function traverse(node: {
        id: string;
        children: Array<{ id: string; children: unknown[] }>;
      }) {
        const frame = frames.get(node.id);
        if (frame) {
          const { x: fx, y: fy, width, height } = frame;
          // Check if point is inside this node's frame
          if (x >= fx && x <= fx + width && y >= fy && y <= fy + height) {
            const area = width * height;
            console.log(
              '[DevTools] Node',
              node.id,
              'contains point, area:',
              area,
            );
            // Prefer smaller (deeper) nodes
            if (area < smallestArea) {
              smallestArea = area;
              deepestNode = node.id;
            }
          }
        }

        // Traverse children
        for (const child of node.children) {
          traverse(
            child as {
              id: string;
              children: Array<{ id: string; children: unknown[] }>;
            },
          );
        }
      }

      traverse(
        tree as {
          id: string;
          children: Array<{ id: string; children: unknown[] }>;
        },
      );
      console.log('[DevTools] findNodeAtPosition result:', deepestNode);
      return deepestNode;
    },
    [],
  );

  // DevTools pause handler - toggle pause for active engine
  useEffect(() => {
    if (!DevToolsBridge) return;

    // Register pause handler with native side
    DevToolsBridge.registerPauseHandler?.();

    // Set up event listener for pause toggle from DevTools UI
    const eventEmitter = new NativeEventEmitter(NativeModules.DevToolsBridge);

    const pauseSubscription = eventEmitter.addListener('onTogglePause', () => {
      // Get the active engine
      const activeEngine = engineMapRef.current.get(activeTabId);
      if (!activeEngine) return;

      // Toggle pause state
      if (activeEngine.isPaused) {
        activeEngine.resume();
        DevToolsBridge?.setEnginePaused?.(false);
      } else {
        activeEngine.pause();
        DevToolsBridge?.setEnginePaused?.(true);
      }
    });

    // Set up event listener for inspect mode toggle
    const inspectSubscription = eventEmitter.addListener(
      'onToggleInspect',
      () => {
        setIsInspectMode((prev) => {
          const next = !prev;
          DevToolsBridge?.setInspectMode?.(next);
          console.log(
            '[DevTools] Inspect mode toggled to:',
            next,
            'activeTabId:',
            activeTabId,
          );
          console.log(
            '[DevTools] engineMapRef keys:',
            Array.from(engineMapRef.current.keys()),
          );

          if (next) {
            // Entering inspect mode - measure all node frames
            const activeEngine = engineMapRef.current.get(activeTabId);
            console.log(
              '[DevTools] Active engine for inspect:',
              activeEngine ? 'exists' : 'null',
            );
            measureAllNodeFrames(activeEngine ?? null);
          } else {
            // Exiting inspect mode - clear highlight
            setHighlightedFrame(null);
            DevToolsBridge?.clearHighlight?.();
          }

          return next;
        });
      },
    );

    // Set up event listener for node selection from DevTools tree
    const selectSubscription = eventEmitter.addListener(
      'onSelectNode',
      (data: { nodeId: string }) => {
        // When user selects a node in the DevTools tree, highlight it in the UI
        console.log('[DevTools] onSelectNode event received:', data);
        const activeEngine = engineMapRef.current.get(activeTabId);
        console.log(
          '[DevTools] Active engine:',
          activeEngine ? 'exists' : 'null',
          'tabId:',
          activeTabId,
        );

        // Get node props first (for style extraction)
        let nodeProps: Record<string, unknown> | null = null;
        if (activeEngine) {
          const receiver = activeEngine.getReceiver();
          const tree = receiver?.getComponentTree?.();
          if (tree) {
            const findNode = (node: {
              id: string;
              props?: Record<string, unknown>;
              children: unknown[];
            }): Record<string, unknown> | null => {
              if (String(node.id) === data.nodeId) {
                return node.props ?? null;
              }
              for (const child of node.children) {
                const found = findNode(
                  child as {
                    id: string;
                    props?: Record<string, unknown>;
                    children: unknown[];
                  },
                );
                if (found !== null) return found;
              }
              return null;
            };
            nodeProps = findNode(
              tree as {
                id: string;
                props?: Record<string, unknown>;
                children: unknown[];
              },
            );
          }
        }

        // Measure and highlight, then update box model with measured frame
        measureAndHighlightNode(data.nodeId, activeEngine ?? null, (frame) => {
          // Update box model with the freshly measured frame
          const style = nodeProps?.style as Record<string, unknown> | undefined;
          const boxModelData = extractBoxModelFromStyle(style, {
            left: frame.x,
            top: frame.y,
            width: frame.width,
            height: frame.height,
          });
          DevToolsBridge?.updateBoxModel?.(boxModelData);
        });
      },
    );

    // Set up event listener to toggle RN's built-in Element Inspector
    const rnInspectorSubscription = eventEmitter.addListener(
      'onToggleRNInspector',
      () => {
        // Emit the event that RN's AppContainer listens to
        DeviceEventEmitter.emit('toggleElementInspector');
      },
    );

    // Track RN Inspector state (listen to our own toggle events for state sync)
    let rnInspectorActive = false;
    const rnInspectorStateSubscription = DeviceEventEmitter.addListener(
      'toggleElementInspector',
      () => {
        rnInspectorActive = !rnInspectorActive;
        DevToolsBridge?.setRNInspectorActive?.(rnInspectorActive);
      },
    );

    return () => {
      pauseSubscription.remove();
      inspectSubscription.remove();
      selectSubscription.remove();
      rnInspectorSubscription.remove();
      rnInspectorStateSubscription.remove();
    };
  }, [activeTabId, measureAndHighlightNode, measureAllNodeFrames]);

  const getEngine = useCallback((tabId: string) => {
    const engine = engineMapRef.current.get(tabId);
    if (!engine) return null;

    const collector = contractCollectorsRef.current.get(tabId);
    return {
      id: engine.id,
      getDiagnostics: () => {
        const diagnostics = engine.getDiagnostics?.() ?? null;
        if (!diagnostics) return null;

        const summary = collector?.summary() ?? null;
        if (!summary) return diagnostics;

        const last = summary.last;
        return {
          ...diagnostics,
          contracts: {
            violations: summary.total,
            lastViolationAt: last?.at ?? null,
            lastViolation: last
              ? `${last.direction}:${last.kind}:${last.eventName}`
              : null,
          },
        };
      },
    };
  }, []);

  // 推送引擎监控快照到原生 DevTools（用于 Monitor 面板）
  useEffect(() => {
    if (!DevToolsBridge?.updateEngineMonitor) return;

    let cancelled = false;
    const pollIntervalMs = 1000;

    const tick = () => {
      if (cancelled) return;
      try {
        // DevTools Monitor 只针对当前 Tab（多 Tab 总览由独立 Task Manager 承载）
        const activeTab =
          monitorTabs.find((t) => t.id === activeTabId) ?? monitorTabs[0];
        if (!activeTab) return;

        const engine = getEngine(activeTab.id);
        const rawDiagnostics = engine
          ? (engine.getDiagnostics?.() as any)
          : null;

        // 收敛 timeline points 体积，避免桥接 payload 过大
        const timeline = rawDiagnostics?.activity?.timeline;
        const points: any[] | undefined = Array.isArray(timeline?.points)
          ? timeline.points
          : undefined;
        const diagnostics =
          points && points.length > 60
            ? {
                ...rawDiagnostics,
                activity: {
                  ...rawDiagnostics.activity,
                  timeline: {
                    ...timeline,
                    points: points.slice(-60),
                  },
                },
              }
            : rawDiagnostics;

        const row = {
          tabId: activeTab.id,
          title: activeTab.title || 'Untitled',
          isActive: true,
          engineId: engine?.id ?? null,
          status: (activeTab as any).status ?? null,
          blockedReason: (activeTab as any).blockedReason ?? null,
          contract: (activeTab as any).contract ?? null,
          permissions: (activeTab as any).permissions ?? null,
          diagnostics,
        };

        DevToolsBridge!.updateEngineMonitor?.(
          JSON.stringify({
            timestamp: Date.now(),
            activeTabId,
            rows: [row],
          }),
        );
      } catch {
        // Ignore DevTools monitor update errors
      }
    };

    tick();
    const timer = setInterval(tick, pollIntervalMs);
    return () => {
      cancelled = true;
      clearInterval(timer);
    };
  }, [activeTabId, getEngine, monitorTabs]);

  // 使用显式尺寸替代 absoluteFill，确保容器尺寸正确
  const hasExplicitSize =
    containerWidth &&
    containerWidth > 0 &&
    containerHeight &&
    containerHeight > 0;

  // 当有显式尺寸时，完全不使用 flex，避免布局冲突
  const tabHostStyle = React.useMemo(() => {
    if (hasExplicitSize) {
      return {
        width: containerWidth,
        // height: containerHeight,
        flex: 1,
        position: 'relative' as const,
      };
    }
    return styles.tabHost;
  }, [hasExplicitSize, containerWidth, containerHeight]);

  // Ref to the main content view for Inspector API
  const inspectedViewRef = useRef<View>(null);

  // Handle inspect hover - highlight element without selecting
  const handleInspectHover = useCallback((x: number, y: number) => {
    if (!inspectedViewRef.current) return;

    try {
      getInspectorDataForViewAtPoint(
        inspectedViewRef.current,
        x,
        y,
        (viewData) => {
          try {
            if (viewData.hierarchy && viewData.hierarchy.length > 0) {
              const { frame, selectedIndex, hierarchy } = viewData;
              // Get component name from hierarchy
              const componentName = hierarchy[selectedIndex]?.name || 'Unknown';

              // Try to find Rill node ID - first direct match
              let nodeId = viewTagToNodeIdRef.current.get(
                viewData.touchedViewTag,
              );

              // If no direct match, find via frame matching
              if (!nodeId) {
                for (const [
                  ,
                  rillNodeId,
                ] of viewTagToNodeIdRef.current.entries()) {
                  const nodeFrame = nodeFramesRef.current.get(rillNodeId);
                  if (nodeFrame) {
                    if (
                      x >= nodeFrame.x &&
                      x <= nodeFrame.x + nodeFrame.width &&
                      y >= nodeFrame.y &&
                      y <= nodeFrame.y + nodeFrame.height
                    ) {
                      if (!nodeId) {
                        nodeId = rillNodeId;
                      } else {
                        const currentFrame = nodeFramesRef.current.get(nodeId);
                        if (
                          currentFrame &&
                          nodeFrame.width * nodeFrame.height <
                            currentFrame.width * currentFrame.height
                        ) {
                          nodeId = rillNodeId;
                        }
                      }
                    }
                  }
                }
              }

              if (frame && frame.width > 0 && frame.height > 0) {
                setHighlightedFrame({
                  x: frame.left,
                  y: frame.top,
                  width: frame.width,
                  height: frame.height,
                  nodeId: nodeId || String(viewData.touchedViewTag),
                  componentName,
                });
              }
            }
          } catch {
            // Silently ignore hover errors to prevent noise
          }
          return false;
        },
      );
    } catch {
      // Silently ignore hover errors
    }
  }, []);

  // Handle inspect click - use RN's Inspector API for accurate view detection
  const handleInspectClick = useCallback(
    (x: number, y: number) => {
      console.log('[DevTools] handleInspectClick at:', x, y);

      if (!inspectedViewRef.current) {
        console.log('[DevTools] No inspected view ref');
        return;
      }

      // Use RN's getInspectorDataForViewAtPoint API
      // This is much more reliable than our manual frame calculation
      try {
        getInspectorDataForViewAtPoint(
          inspectedViewRef.current,
          x,
          y,
          (viewData) => {
            try {
              console.log('[DevTools] Inspector API result:', {
                hierarchyLength: viewData.hierarchy?.length ?? 0,
                selectedIndex: viewData.selectedIndex,
                frame: viewData.frame,
                touchedViewTag: viewData.touchedViewTag,
              });

              if (viewData.hierarchy && viewData.hierarchy.length > 0) {
                // Get the frame from the API response - this is accurate!
                const { frame } = viewData;
                console.log('[DevTools] Using Inspector frame:', frame);

                // Update highlight with accurate frame data
                if (frame && frame.width > 0 && frame.height > 0) {
                  setHighlightedFrame({
                    x: frame.left,
                    y: frame.top,
                    width: frame.width,
                    height: frame.height,
                    nodeId: String(viewData.touchedViewTag),
                  });
                  DevToolsBridge?.highlightNode?.(
                    String(viewData.touchedViewTag),
                    {
                      x: frame.left,
                      y: frame.top,
                      width: frame.width,
                      height: frame.height,
                    },
                  );
                }

                // Find the corresponding Rill node ID using our mapping
                // First try direct match
                console.log(
                  '[DevTools] viewTagToNodeId map size:',
                  viewTagToNodeIdRef.current.size,
                );
                console.log(
                  '[DevTools] nodeFrames map size:',
                  nodeFramesRef.current.size,
                );
                let nodeId = viewTagToNodeIdRef.current.get(
                  viewData.touchedViewTag,
                );
                console.log(
                  '[DevTools] Direct viewTag lookup:',
                  viewData.touchedViewTag,
                  '->',
                  nodeId,
                );

                // If no direct match, walk up the hierarchy to find a Rill node
                if (!nodeId && viewData.hierarchy) {
                  console.log(
                    '[DevTools] Walking hierarchy to find Rill node...',
                  );
                  for (
                    let i = viewData.selectedIndex;
                    i < viewData.hierarchy.length;
                    i++
                  ) {
                    const item = viewData.hierarchy[i];
                    if (!item) continue;

                    try {
                      const inspectorData = item.getInspectorData(
                        findNodeHandle as (component: unknown) => number | null,
                      );
                      if (
                        inspectorData &&
                        typeof inspectorData.measure === 'function'
                      ) {
                        // Get the view tag for this hierarchy item by measuring it
                        inspectorData.measure(
                          (mx, my, mw, mh, pageX, pageY) => {
                            // Check if this hierarchy item's view tag is in our mapping
                            // We can't easily get the view tag here, but we can check by component name
                          },
                        );
                      }
                    } catch {
                      // Ignore errors
                    }

                    // Check if the component name looks like a Rill component
                    const componentName = item.name;
                    console.log('[DevTools] Hierarchy item:', i, componentName);
                  }

                  // Alternative: scan all mapped view tags and find one whose frame contains the click point
                  const clickX = x;
                  const clickY = y;
                  for (const [
                    tag,
                    rillNodeId,
                  ] of viewTagToNodeIdRef.current.entries()) {
                    const frame = nodeFramesRef.current.get(rillNodeId);
                    if (frame) {
                      if (
                        clickX >= frame.x &&
                        clickX <= frame.x + frame.width &&
                        clickY >= frame.y &&
                        clickY <= frame.y + frame.height
                      ) {
                        // This Rill node contains the click point
                        // Use the smallest (most specific) matching node
                        if (!nodeId) {
                          nodeId = rillNodeId;
                        } else {
                          const currentFrame =
                            nodeFramesRef.current.get(nodeId);
                          if (
                            currentFrame &&
                            frame.width * frame.height <
                              currentFrame.width * currentFrame.height
                          ) {
                            nodeId = rillNodeId;
                          }
                        }
                      }
                    }
                  }
                  console.log('[DevTools] Found via frame matching:', nodeId);
                }

                if (nodeId) {
                  console.log('[DevTools] Found Rill nodeId:', nodeId);
                  DevToolsBridge?.selectNodeFromInspect?.(nodeId);

                  // Send box model data to DevTools
                  if (frame && frame.width > 0 && frame.height > 0) {
                    // Try to get style from viewData.props or hierarchy
                    const style =
                      (viewData.props?.style as Record<string, unknown>) ??
                      (viewData.hierarchy?.[
                        viewData.selectedIndex
                      ]?.getInspectorData(
                        findNodeHandle as (component: unknown) => number | null,
                      )?.props?.style as Record<string, unknown>);
                    const boxModelData = extractBoxModelFromStyle(style, frame);
                    DevToolsBridge?.updateBoxModel?.(boxModelData);
                  }
                } else {
                  console.log(
                    '[DevTools] No Rill node found for viewTag:',
                    viewData.touchedViewTag,
                  );
                }
              }
            } catch (callbackError) {
              console.log(
                '[DevTools] Inspector callback error:',
                callbackError,
              );
            }
            // Note: Don't exit inspect mode - let user manually exit

            return false; // Don't continue traversing
          },
        );
      } catch (error) {
        console.log('[DevTools] Inspector API error:', error);

        // Fallback to our manual implementation
        const activeEngine = engineMapRef.current.get(activeTabId);
        if (activeEngine) {
          const nodeId = findNodeAtPosition(x, y, activeEngine);
          console.log('[DevTools] Fallback findNodeAtPosition result:', nodeId);

          if (nodeId) {
            DevToolsBridge?.selectNodeFromInspect?.(nodeId);
            measureAndHighlightNode(nodeId, activeEngine);
          }
        }
        // Don't exit inspect mode on error - let user manually exit
      }
    },
    [activeTabId, findNodeAtPosition, measureAndHighlightNode],
  );

  // Handle touch for inspect mode (like RN's InspectorOverlay)
  const findViewForTouchEvent = useCallback(
    (e: { nativeEvent: { locationX: number; locationY: number } }) => {
      const { locationX, locationY } = e.nativeEvent;
      handleInspectHover(locationX, locationY);
    },
    [handleInspectHover],
  );

  const handleStartShouldSetResponder = useCallback(
    (e: { nativeEvent: { locationX: number; locationY: number } }) => {
      findViewForTouchEvent(e);
      return true;
    },
    [findViewForTouchEvent],
  );

  const handleResponderRelease = useCallback(
    (e: { nativeEvent: { locationX: number; locationY: number } }) => {
      const { locationX, locationY } = e.nativeEvent;
      handleInspectClick(locationX, locationY);
    },
    [handleInspectClick],
  );

  return (
    <View style={tabHostStyle} pointerEvents='box-none'>
      {/* Content container - ref used for Inspector API */}
      <View
        style={styles.contentContainer}
        ref={inspectedViewRef}
        pointerEvents='box-none'
        collapsable={false}
      >
        {tabs.map((tab) => {
          const isActive = tab.id === activeTabId;
          // 重要：显式尺寸模式下不使用 flex:1，完全依赖父容器的宽高
          const displayStyle = {
            display: isActive ? ('flex' as const) : ('none' as const),
          };
          const tabContainerStyle = hasExplicitSize
            ? {
                width: containerWidth,
                // height: containerHeight,
                flex: 1,
                ...displayStyle,
              }
            : [StyleSheet.absoluteFill, displayStyle];
          return (
            <View
              key={`${tab.id}-${tab.reloadKey ?? 0}`}
              style={tabContainerStyle}
            >
              <ActrTabView
                tab={tab}
                isActive={isActive}
                isLeftPanelVisible={isLeftPanelVisible}
                containerWidth={containerWidth}
                containerHeight={containerHeight}
                onContractViolation={recordContractViolation}
                onEngineChange={handleEngineChange}
              />
            </View>
          );
        })}
      </View>

      {/* Inspect mode overlay - like RN's InspectorOverlay */}
      {isInspectMode && (
        <View
          style={styles.inspectOverlay}
          onStartShouldSetResponder={handleStartShouldSetResponder}
          onResponderMove={findViewForTouchEvent}
          onResponderRelease={handleResponderRelease}
        >
          {/* Highlight box for selected element */}
          {highlightedFrame && (
            <View
              style={[
                styles.highlightOverlay,
                {
                  left: highlightedFrame.x,
                  top: highlightedFrame.y,
                  width: highlightedFrame.width,
                  height: highlightedFrame.height,
                },
              ]}
              pointerEvents='none'
            >
              <View style={styles.highlightLabel}>
                <AppText style={styles.highlightLabelText}>
                  {highlightedFrame.componentName ||
                    `#${highlightedFrame.nodeId}`}
                </AppText>
              </View>
            </View>
          )}
        </View>
      )}

      {/* Highlight when not in inspect mode (from tree selection) */}
      {!isInspectMode && highlightedFrame && (
        <View
          style={[
            styles.highlightOverlay,
            {
              left: highlightedFrame.x,
              top: highlightedFrame.y,
              width: highlightedFrame.width,
              height: highlightedFrame.height,
            },
          ]}
          pointerEvents='none'
        >
          <View style={styles.highlightLabel}>
            <AppText style={styles.highlightLabelText}>
              {highlightedFrame.componentName || `#${highlightedFrame.nodeId}`}
            </AppText>
          </View>
        </View>
      )}
    </View>
  );
};

const makeStyles = (colors: ColorTokens) => StyleSheet.create({
  tabHost: {
    flex: 1,
    position: 'relative',
  },
  contentContainer: {
    flex: 1,
  },
  empty: {
    flex: 1,
    backgroundColor: colors.bgEmpty,
  },
  inspectOverlay: {
    position: 'absolute',
    backgroundColor: 'transparent',
    top: 0,
    left: 0,
    right: 0,
    bottom: 0,
  },
  // DevTools inspect overlay 使用固定蓝色，不参与主题切换
  highlightOverlay: {
    position: 'absolute',
    borderWidth: 2,
    borderColor: 'rgba(0, 122, 255, 0.9)',
    backgroundColor: 'rgba(0, 122, 255, 0.15)',
    zIndex: 9998,
  },
  highlightLabel: {
    position: 'absolute',
    top: -20,
    left: 0,
    backgroundColor: 'rgba(0, 122, 255, 0.9)',
    paddingHorizontal: spacing.xs,
    paddingVertical: spacing.xxxs,
    borderRadius: radius.xs,
  },
  highlightLabelText: {
    color: '#fff',
    ...getTypographyStyle('caption2'),
    fontWeight: '600',
  },
});

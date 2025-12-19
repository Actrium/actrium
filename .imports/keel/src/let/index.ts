/**
 * @rill/let
 *
 * Guest-side SDK for rill - runs inside sandbox
 * Provides virtual components, hooks, and React reconciler for Guest development
 * Named after applet, servlet, pagelet - a small, embeddable unit
 */

// Virtual components (string identifiers)
export {
  View,
  Text,
  Image,
  ScrollView,
  TouchableOpacity,
  FlatList,
  TextInput,
  Button,
  Switch,
  ActivityIndicator,
} from './sdk';

// Hooks
export { useHostEvent, useConfig, useSendToHost } from './sdk';

// Error boundary
export type { ErrorInfo, RillErrorBoundaryProps } from './sdk';
export { RillErrorBoundary } from './sdk';

// Component prop types
export type {
  ReactNode,
  ComponentType,
  BaseProps,
  ViewProps,
  TextProps,
  ImageProps,
  ScrollViewProps,
  TouchableOpacityProps,
  FlatListProps,
  TextInputProps,
  ButtonProps,
  SwitchProps,
  ActivityIndicatorProps,
} from './sdk';

// Event and style types
export type { ImageSource, LayoutEvent, ScrollEvent, StyleObject, StyleProp } from './sdk';

// Reconciler
export { render, unmount, unmountAll, invokeCallback, hasCallback } from './reconciler';
export type { SendToHost, Operation, OperationBatch, VNode } from './reconciler';

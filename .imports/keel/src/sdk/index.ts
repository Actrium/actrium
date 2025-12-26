/**
 * @rill/let
 *
 * Guest-side SDK for rill - runs inside sandbox
 * Provides virtual components, hooks for Guest development
 * Named after applet, servlet, pagelet - a small, embeddable unit
 *
 * Note: Runtime internals (render, unmount, CallbackRegistry) are in src/guest-bundle/
 */

// Component prop types and event types
export type {
  ActivityIndicatorProps,
  BaseProps,
  ButtonProps,
  ComponentType,
  ErrorInfo,
  FlatListProps,
  ImageProps,
  ImageSource,
  LayoutEvent,
  ReactNode,
  RillErrorBoundaryProps,
  ScrollEvent,
  ScrollViewProps,
  StyleObject,
  StyleProp,
  SwitchProps,
  TextInputProps,
  TextProps,
  TouchableOpacityProps,
  ViewProps,
} from './sdk';
// Virtual components (string identifiers)
// Hooks
export {
  ActivityIndicator,
  Button,
  FlatList,
  Image,
  RillErrorBoundary,
  ScrollView,
  Switch,
  Text,
  TextInput,
  TouchableOpacity,
  useConfig,
  useHostEvent,
  useRemoteRef,
  useSendToHost,
  View,
} from './sdk';
// Remote ref types
export type {
  FlatListRef,
  MeasurableRef,
  MeasureResult,
  RemoteRef,
  RemoteRefCallback,
  ScrollViewRef,
  TextInputRef,
} from './types';

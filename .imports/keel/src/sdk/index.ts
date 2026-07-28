/**
 * rill/guest
 *
 * Guest-side SDK for rill - runs inside sandbox
 * Provides virtual components, hooks for Guest development
 *
 * Note: Guest runtime internals (reconciler/bridge) are in src/guest/runtime/
 */

// Sandbox-safe utilities (issue #68). Guest sandboxes provide only core
// ECMAScript intrinsics — web platform globals like `TextEncoder` and
// `performance` are NOT guaranteed (they type-check under a DOM lib but throw in
// the sandbox) — so guest code that needs them must go through these call-time-
// checked helpers instead of touching the globals directly. `createUtf8Encoder`
// is byte-identical to `TextEncoder.encode` (incl. lone surrogate → U+FFFD) with
// a pure-JS fallback; `nowMs` uses `performance.now()` when present, else
// `Date.now()`.
export { createUtf8Encoder, nowMs } from '../guest/runtime/reconciler/sandbox-compat';
// Web keyboard bridge (issue #19, L3)
export type { RillKeyEvent } from '../shared/keyboard';
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
  RillPlatformInfo,
  ScrollEvent,
  ScrollViewProps,
  StyleObject,
  StyleProp,
  SwitchProps,
  TextInputProps,
  TextProps,
  TouchableOpacityProps,
  UseKeyboardSpec,
  ViewProps,
} from './sdk';
// Components - Core
// Components - Scrolling
// Components - Input
// Components - Touchables
// Components - Feedback
// Components - Layout
// APIs - Pure JS
// APIs - Platform Info
// APIs - Event Subscription
// APIs - Host Capability
// Hooks - React Native
// Hooks - Rill
// Error Boundary
export {
  ActivityIndicator,
  Alert,
  Appearance,
  AppState,
  Button,
  Dimensions,
  Easing,
  FlatList,
  I18nManager,
  Image,
  ImageBackground,
  Keyboard,
  KeyboardAvoidingView,
  Linking,
  Modal,
  PixelRatio,
  Platform,
  Pressable,
  RefreshControl,
  RillErrorBoundary,
  SafeAreaView,
  ScrollView,
  SectionList,
  Share,
  StatusBar,
  StyleSheet,
  Switch,
  Text,
  TextInput,
  TouchableHighlight,
  TouchableOpacity,
  TouchableWithoutFeedback,
  useColorScheme,
  useConfig,
  useHostEvent,
  useKeyboard,
  useRemoteRef,
  useSendToHost,
  useWindowDimensions,
  Vibration,
  View,
  VirtualizedList,
} from './sdk';
// Remote ref types
// Style types
// Event types
export type {
  ColorValue,
  DimensionValue,
  FlatListRef,
  FlexStyle,
  GestureResponderEvent,
  ImageStyle,
  LayoutChangeEvent,
  MeasurableRef,
  MeasureResult,
  NativeSyntheticEvent,
  RemoteRef,
  RemoteRefCallback,
  ScrollViewRef,
  TextInputRef,
  TextLayoutEvent,
  TextLayoutLine,
  TextStyle,
  ViewStyle,
} from './types';

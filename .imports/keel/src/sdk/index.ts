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
// checked helpers instead of touching the globals directly. Measured on
// QuickJS-WASM, `TextEncoder`, `TextDecoder`, `performance`, `btoa`, `atob`,
// `Buffer` and `crypto` are ALL absent, so this is a category, not a caveat.
//   - `createUtf8Encoder` — byte-identical to `TextEncoder.encode` (incl. lone
//     surrogate → U+FFFD), which the host's fatal wire decoders depend on;
//   - `createUtf8Decoder` — agrees with a NON-fatal `TextDecoder` (WHATWG
//     replacement rules), for bytes a host capability hands back;
//   - `encodeBase64` / `decodeBase64` — standard RFC 4648, for moving bytes
//     through a text-shaped interface;
//   - `nowMs` — `performance.now()` when present, else `Date.now()`.
export {
  createUtf8Decoder,
  createUtf8Encoder,
  decodeBase64,
  encodeBase64,
  nowMs,
} from '../guest/runtime/reconciler/sandbox-compat';
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
  GuestComponent,
  ImageBackgroundProps,
  ImageProps,
  ImageSource,
  KeyboardAvoidingViewProps,
  LayoutEvent,
  ModalProps,
  PressableProps,
  ReactNode,
  RefreshControlProps,
  RillErrorBoundaryProps,
  RillPlatformInfo,
  SafeAreaViewProps,
  ScrollEvent,
  ScrollViewProps,
  SectionListProps,
  StatusBarProps,
  StyleObject,
  StyleProp,
  SwitchProps,
  TextInputProps,
  TextProps,
  TouchableHighlightProps,
  TouchableOpacityProps,
  TouchableWithoutFeedbackProps,
  UseKeyboardSpec,
  ViewProps,
  VirtualizedListProps,
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
  declareHostComponent,
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

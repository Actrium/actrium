/**
 * @rill/host/preset
 *
 * Default components and EngineView for Host-side rendering
 *
 * Works for both React Native and Web (via react-native-web).
 * For web builds, configure bundler alias: 'react-native' → 'react-native-web'
 */

// Re-export runtime types for convenience
export type {
  ComponentMap,
  ComponentRegistry,
  ComponentType,
  Engine,
  EngineOptions,
} from '..';
// Default components
export type {
  ActivityIndicatorProps,
  ButtonProps,
  ClickableViewProps,
  DefaultComponentName,
  FlatListProps,
  ImageProps,
  ImageSource,
  ScrollEvent,
  ScrollViewProps,
  SwitchProps,
  TextInputProps,
  TextProps,
  TouchableOpacityProps,
  ViewProps,
} from './components';
export {
  ActivityIndicator,
  Button,
  ClickableView,
  DefaultComponents,
  FlatList,
  Image,
  ScrollView,
  Switch,
  Text,
  TextInput,
  TouchableOpacity,
  View,
} from './components';
// EngineView component
export type { EngineViewProps } from './EngineView';
export { EngineView } from './EngineView';

/**
 * @rill/preset-react-native
 *
 * React Native preset for rill - default components and EngineView
 */

// Re-export runtime types for convenience
export type {
  ComponentMap,
  ComponentRegistry,
  ComponentType,
  Engine,
  EngineOptions,
} from '@rill/runtime';
// Default components for React Native
// Individual component exports
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

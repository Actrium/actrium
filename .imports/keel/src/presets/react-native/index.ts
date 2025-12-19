/**
 * @rill/preset-react-native
 *
 * React Native preset for rill - default components and EngineView
 */

// EngineView component
export type { EngineViewProps } from './EngineView';
export { EngineView } from './EngineView';

// Default components for React Native
export type { DefaultComponentName } from './components';
export { DefaultComponents } from './components';

// Individual component exports
export type {
  ActivityIndicatorProps,
  ButtonProps,
  ClickableViewProps,
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
  FlatList,
  Image,
  ScrollView,
  Switch,
  Text,
  TextInput,
  TouchableOpacity,
  View,
} from './components';

// Re-export runtime types for convenience
export type {
  Engine,
  EngineOptions,
  ComponentMap,
  ComponentRegistry,
  ComponentType,
} from '@rill/runtime';

/**
 * Default component exports
 */

export { View } from './View';
export type { ViewProps } from './View';

export { Text } from './Text';
export type { TextProps } from './Text';

export { Image } from './Image';
export type { ImageProps, ImageSource } from './Image';

export { TouchableOpacity } from './TouchableOpacity';
export type { TouchableOpacityProps } from './TouchableOpacity';

export { ScrollView } from './ScrollView';
export type { ScrollViewProps, ScrollEvent } from './ScrollView';

/**
 * Default component mapping
 * For registering with Engine
 */
import { View } from './View';
import { Text } from './Text';
import { Image } from './Image';
import { TouchableOpacity } from './TouchableOpacity';
import { ScrollView } from './ScrollView';

export const DefaultComponents = {
  View,
  Text,
  Image,
  TouchableOpacity,
  ScrollView,
} as const;

export type DefaultComponentName = keyof typeof DefaultComponents;

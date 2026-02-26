import { Animated } from 'react-native';

export interface ExtensionPanelProps {
  visible: boolean;
  content: React.ReactElement | null;
  width: number;
  centerAnimValue: Animated.Value;
}

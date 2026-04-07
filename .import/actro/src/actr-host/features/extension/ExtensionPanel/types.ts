import type { SharedValue } from 'react-native-reanimated';

/**
 * Phone 扩展面板 props
 * - 使用 reanimated SharedValue 驱动手势跟手动画
 * - slideAnim 由父组件（hook）创建并共享，手势和按钮都通过它驱动面板位移
 */
export interface PhoneExtensionPanelProps {
  visible: boolean;
  content: React.ReactElement | null;
  width: number;
  /** 中间对话区域偏移动画值 */
  centerAnimValue: SharedValue<number>;
  /** 面板滑动位置（0 = 关闭，maxSlide = 打开），由 hook 创建 */
  slideAnim: SharedValue<number>;
  /** 手势进行中标记，为 true 时跳过按钮触发的动画 */
  isGesturing: SharedValue<boolean>;
}

/**
 * Tablet 扩展面板 props
 * - 无手势交互，centerAnimValue 始终设为 0
 */
export interface TabletExtensionPanelProps {
  visible: boolean;
  content: React.ReactElement | null;
  width: number;
  centerAnimValue: SharedValue<number>;
}

/**
 * 对外统一 props（由 index.tsx 路由分发）
 */
export interface ExtensionPanelProps {
  visible: boolean;
  content: React.ReactElement | null;
  width: number;
  centerAnimValue: SharedValue<number>;
  /** Phone 模式需要的面板滑动动画值 */
  slideAnim: SharedValue<number>;
  /** 手势进行中标记 */
  isGesturing: SharedValue<boolean>;
}

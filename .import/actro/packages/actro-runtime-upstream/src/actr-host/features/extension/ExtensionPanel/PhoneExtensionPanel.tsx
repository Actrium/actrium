import React, { useEffect, useState } from 'react';
import { View, StyleSheet } from 'react-native';
import Animated, {
  useAnimatedStyle,
  withTiming,
  Easing,
  runOnJS,
} from 'react-native-reanimated';
import { LAYOUT, CHAT_HEADER_HEIGHT } from '../../../constants';
import type { PhoneExtensionPanelProps } from './types';
import { extensionPanel } from '../../../../theme/tokens';
import { useColors } from '../../../../theme/useColors';
import type { ColorTokens } from '../../../../theme/colors';

/** 收尾动画时长 */
const ANIM_DURATION = 250;

/**
 * 骨架屏组件 - 内容加载前的占位
 */
const SkeletonPlaceholder: React.FC<{ colors: ColorTokens }> = ({ colors }) => {
  const styles = React.useMemo(() => makeStyles(colors), [colors]);
  return (
    <View style={styles.skeletonContainer}>
      <View style={styles.skeletonHeader}>
        <View style={[styles.skeletonBlock, { width: 120, height: 24 }]} />
      </View>
      <View style={styles.skeletonContent}>
        <View
          style={[
            styles.skeletonBlock,
            { width: '100%', height: 60, marginBottom: 12 },
          ]}
        />
        <View
          style={[
            styles.skeletonBlock,
            { width: '80%', height: 40, marginBottom: 12 },
          ]}
        />
        <View style={[styles.skeletonBlock, { width: '90%', height: 40 }]} />
      </View>
    </View>
  );
};

/**
 * Phone 扩展面板
 * - 手势由父组件通过 GestureDetector 控制（useExtensionPanelGesture hook）
 * - 本组件只负责：渲染面板内容 + 响应按钮触发的 visible prop 变化
 * - 推动中间对话窗口 80%，右侧保留 20%
 */
const PhoneExtensionPanel: React.FC<PhoneExtensionPanelProps> = ({
  visible,
  content,
  width,
  centerAnimValue,
  slideAnim,
  isGesturing,
}) => {
  const colors = useColors();
  const styles = React.useMemo(() => makeStyles(colors), [colors]);

  // 面板最大滑动距离
  const maxSlide = width * extensionPanel.phoneWidthRatio;

  // 控制面板是否渲染（动画结束后再移除，避免闪烁）
  const [shouldRender, setShouldRender] = useState(visible);

  // ─── visible 变化时确保渲染状态同步 ────────────────────
  // 无论是按钮触发还是手势触发，visible=true 时都要渲染面板
  useEffect(() => {
    if (visible) {
      setShouldRender(true);
    }
  }, [visible]);

  // ─── 按钮触发的动画同步（visible prop 变化时） ────────
  // 手势进行中时跳过动画，避免和手势的 onUpdate 冲突
  useEffect(() => {
    if (isGesturing.value) return;

    const target = visible ? maxSlide : 0;
    const timingConfig = {
      duration: ANIM_DURATION,
      easing: Easing.out(Easing.cubic),
    };

    slideAnim.value = withTiming(target, timingConfig);
    centerAnimValue.value = withTiming(target, timingConfig, (finished) => {
      // 关闭动画结束后移除渲染
      if (finished && !visible) {
        runOnJS(setShouldRender)(false);
      }
    });
  }, [visible, maxSlide, slideAnim, centerAnimValue]);

  // ─── 面板动画样式 ─────────────────────────────────────
  const panelAnimStyle = useAnimatedStyle(() => ({
    transform: [
      {
        // slideAnim [0, maxSlide] → translateX [-maxSlide, 0]
        // 0 → 面板完全隐藏在左侧屏幕外
        // maxSlide → 面板完全可见
        translateX: slideAnim.value - maxSlide,
      },
    ],
  }));

  // 面板不渲染时直接返回 null
  if (!shouldRender) {
    return null;
  }

  return (
    <Animated.View
      style={[
        styles.panel,
        { width: maxSlide, paddingTop: CHAT_HEADER_HEIGHT },
        panelAnimStyle,
      ]}
    >
      {content || <SkeletonPlaceholder colors={colors} />}
    </Animated.View>
  );
};

export default PhoneExtensionPanel;

const makeStyles = (colors: ColorTokens) => StyleSheet.create({
  panel: {
    position: 'absolute',
    left: 0,
    height: '100%',
    flexShrink: 1,
    backgroundColor: colors.bgChatPage,
    borderRightWidth: 1,
    borderRightColor: colors.extensionPanelBorder,
    overflow: 'hidden',
  },
  skeletonContainer: {
    flex: 1,
    backgroundColor: colors.bgChatPage,
    paddingTop: 80,
  },
  skeletonHeader: {
    paddingHorizontal: 20,
    marginBottom: 24,
  },
  skeletonContent: {
    paddingHorizontal: 20,
  },
  skeletonBlock: {
    backgroundColor: colors.extensionPanelSkeletonBlock,
    borderRadius: 8,
  },
});

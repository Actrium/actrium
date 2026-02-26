import React, { useEffect, useRef, useState } from 'react';
import { Animated, StyleSheet, Easing } from 'react-native';
import type { ExtensionPanelProps } from './types';

/**
 * Tablet 扩展面板
 * - 从右侧滑入
 * - 覆盖在对话页面上层
 * - 不推动中间窗口
 */
const TabletExtensionPanel: React.FC<ExtensionPanelProps> = ({
  visible,
  content,
  width,
  centerAnimValue,
}) => {
  // 动画值：控制滑动距离
  // 初始值：隐藏时在屏幕外（width），显示时在可见位置（0）
  const slideAnim = useRef(
    new Animated.Value(visible && content ? 0 : width)
  ).current;

  // 控制面板的实际渲染（动画结束后再移除）
  const [shouldRender, setShouldRender] = useState(visible && !!content);

  useEffect(() => {
    const shouldShow = visible && !!content;
    const targetValue = shouldShow ? 0 : width;

    // 如果要显示，先渲染组件再播放动画
    if (shouldShow) {
      setShouldRender(true);
    }

    // 中间窗口不移动
    Animated.timing(centerAnimValue, {
      toValue: 0,
      duration: 0,
      useNativeDriver: true,
    }).start();

    // 面板从右侧滑入
    Animated.timing(slideAnim, {
      toValue: targetValue,
      duration: 300,
      easing: Easing.out(Easing.cubic),
      useNativeDriver: true,
    }).start(({ finished }) => {
      // 动画结束后，如果是隐藏状态，才移除组件
      if (finished && !shouldShow) {
        setShouldRender(false);
      }
    });
  }, [visible, content, width, slideAnim, centerAnimValue]);

  // 不渲染
  if (!shouldRender || !content) {
    return null;
  }

  return (
    <Animated.View
      style={[
        styles.panel,
        {
          width,
          transform: [{ translateX: slideAnim }],
        },
      ]}
    >
      {content}
    </Animated.View>
  );
};

export default TabletExtensionPanel;

const styles = StyleSheet.create({
  panel: {
    position: 'absolute',
    right: 0,
    height: '100%',
    flexShrink: 1,
    zIndex: 10, // 覆盖在上层
  },
});

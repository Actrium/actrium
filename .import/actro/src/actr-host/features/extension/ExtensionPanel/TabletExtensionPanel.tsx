import React, { useEffect, useRef, useState } from 'react';
import { Animated, StyleSheet, Easing } from 'react-native';
import type { TabletExtensionPanelProps } from './types';
import { useColors } from '../../../../theme/useColors';
import type { ColorTokens } from '../../../../theme/colors';

/**
 * Tablet 扩展面板
 * - 从右侧滑入，覆盖在对话页面上层
 * - 不推动中间窗口（centerAnimValue 始终为 0）
 * - 无手势交互，仅按钮控制
 */
const TabletExtensionPanel: React.FC<TabletExtensionPanelProps> = ({
  visible,
  content,
  width,
  centerAnimValue,
}) => {
  const colors = useColors();
  const styles = React.useMemo(() => makeStyles(colors), [colors]);

  // 面板滑动动画值（RN Animated，仅控制自身位移）
  const slideAnim = useRef(
    new Animated.Value(visible && content ? 0 : width),
  ).current;

  // 控制面板的实际渲染（动画结束后再移除）
  const [shouldRender, setShouldRender] = useState(visible && !!content);

  useEffect(() => {
    const shouldShow = visible && !!content;
    const targetValue = shouldShow ? 0 : width;

    if (shouldShow) {
      setShouldRender(true);
    }

    // Tablet 模式下中间窗口不移动
    centerAnimValue.value = 0;

    // 面板从右侧滑入
    Animated.timing(slideAnim, {
      toValue: targetValue,
      duration: 300,
      easing: Easing.out(Easing.cubic),
      useNativeDriver: true,
    }).start(({ finished }) => {
      if (finished && !shouldShow) {
        setShouldRender(false);
      }
    });
  }, [visible, content, width, slideAnim, centerAnimValue]);

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

const makeStyles = (colors: ColorTokens) => StyleSheet.create({
  panel: {
    position: 'absolute',
    right: 0,
    height: '100%',
    flexShrink: 1,
    zIndex: 10, // 覆盖在上层
    backgroundColor: colors.bgChatTablet,
  },
});

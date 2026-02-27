import React, { useEffect, useRef, useState } from 'react';
import { View, Animated, StyleSheet, Easing } from 'react-native';
import { LAYOUT } from '../../constants';
import type { ExtensionPanelProps } from './types';

/**
 * Phone 扩展面板
 * - 从左侧滑入
 * - 推动中间窗口 80%
 * - 右侧保留 20% 显示对话页面
 * - 有右侧分割线
 */
const PhoneExtensionPanel: React.FC<ExtensionPanelProps> = ({
  visible,
  content,
  width,
  centerAnimValue,
}) => {
  // 动画值：控制滑动距离
  const slideAnim = useRef(new Animated.Value(0)).current;

  // 控制面板的实际渲染（动画结束后再移除）
  const [shouldRender, setShouldRender] = useState(visible && !!content);

  useEffect(() => {
    const shouldShow = visible && !!content;
    const targetValue = shouldShow ? width * 0.8 : 0;

    // 如果要显示，先渲染组件再播放动画
    if (shouldShow) {
      setShouldRender(true);
    }

    // 同时动画：面板滑入 + 中间窗口移动
    Animated.parallel([
      Animated.timing(slideAnim, {
        toValue: targetValue,
        duration: 300,
        easing: Easing.out(Easing.cubic),
        useNativeDriver: true,
      }),
      Animated.timing(centerAnimValue, {
        toValue: targetValue,
        duration: 300,
        easing: Easing.out(Easing.cubic),
        useNativeDriver: true,
      }),
    ]).start(({ finished }) => {
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
          width: width * 0.8,
          transform: [
            {
              translateX: slideAnim.interpolate({
                inputRange: [0, width * 0.8],
                outputRange: [-width * 0.8, 0],
              }),
            },
          ],
        },
      ]}
    >
      <View style={styles.borderRight} />
      {content}
    </Animated.View>
  );
};

export default PhoneExtensionPanel;

const styles = StyleSheet.create({
  panel: {
    position: 'absolute',
    left: 0,
    height: '100%',
    flexShrink: 1,
    // paddingTop: 64,
  },
  borderRight: {
    position: 'absolute',
    right: 0,
    top: 0,
    bottom: 0,
    width: 1,
    backgroundColor: LAYOUT.BORDER_COLOR,
  },
});

import React, { useEffect, useRef, useState } from 'react';
import { View, Animated, StyleSheet, Easing } from 'react-native';
import { LAYOUT } from '../../constants';
import type { ExtensionPanelProps } from './types';

/**
 * 骨架屏组件 - 内容加载前的占位
 */
const SkeletonPlaceholder: React.FC = () => (
  <View style={styles.skeletonContainer}>
    {/* 顶部标题骨架 */}
    <View style={styles.skeletonHeader}>
      <View style={[styles.skeletonBlock, { width: 120, height: 24 }]} />
    </View>
    {/* 内容骨架 */}
    <View style={styles.skeletonContent}>
      <View style={[styles.skeletonBlock, { width: '100%', height: 60, marginBottom: 12 }]} />
      <View style={[styles.skeletonBlock, { width: '80%', height: 40, marginBottom: 12 }]} />
      <View style={[styles.skeletonBlock, { width: '90%', height: 40 }]} />
    </View>
  </View>
);

/**
 * Phone 扩展面板
 * - 从左侧滑入
 * - 推动中间窗口 80%
 * - 右侧保留 20% 显示对话页面
 * - 有右侧分割线
 * - 内容未加载时显示骨架屏
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
  const [shouldRender, setShouldRender] = useState(visible);

  // Debug log
  console.log('[PhoneExtensionPanel] visible:', visible, 'hasContent:', !!content, 'shouldRender:', shouldRender);

  useEffect(() => {
    const targetValue = visible ? width * 0.8 : 0;
    console.log('[PhoneExtensionPanel] useEffect - visible:', visible, 'targetValue:', targetValue);

    // 如果要显示，先渲染组件再播放动画
    if (visible) {
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
      console.log('[PhoneExtensionPanel] animation finished:', finished, 'visible:', visible);
      // 动画结束后，如果是隐藏状态，才移除组件
      if (finished && !visible) {
        setShouldRender(false);
      }
    });
  }, [visible, width, slideAnim, centerAnimValue]);

  // 不渲染
  if (!shouldRender) {
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
      {content || <SkeletonPlaceholder />}
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
    backgroundColor: '#fff',
  },
  borderRight: {
    position: 'absolute',
    right: 0,
    top: 0,
    bottom: 0,
    width: 1,
    backgroundColor: LAYOUT.BORDER_COLOR,
  },
  // 骨架屏样式
  skeletonContainer: {
    flex: 1,
    backgroundColor: '#fff',
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
    backgroundColor: '#e7e7e7',
    borderRadius: 8,
  },
});

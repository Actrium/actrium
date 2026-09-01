import React, { useEffect, useRef, useState } from 'react';
import { Animated, StyleSheet, Easing } from 'react-native';
import type { TabletExtensionPanelProps } from './types';
import { useColors } from '../../../../theme/useColors';
import type { ColorTokens } from '../../../../theme/colors';

/**
 * Tablet Extension Panel
 * - Slide from right, overwhelm the top of the dialogue page
 * - Do not promote intermediate windows (centerAnimValue Always 0)
 * - No gesture interaction, button control only
 */
const TabletExtensionPanel: React.FC<TabletExtensionPanelProps> = ({
  visible,
  content,
  width,
  centerAnimValue,
}) => {
  const colors = useColors();
  const styles = React.useMemo(() => makeStyles(colors), [colors]);

  // Panel Animation Value (Scruise)RN Animatedcontrol only the shift of your own position)
  const slideAnim = useRef(
    new Animated.Value(visible && content ? 0 : width),
  ).current;

  // Control panel actual rendering (remove after animation)
  const [shouldRender, setShouldRender] = useState(visible && !!content);

  useEffect(() => {
    const shouldShow = visible && !!content;
    const targetValue = shouldShow ? 0 : width;

    if (shouldShow) {
      setShouldRender(true);
    }

    // Tablet Do not move the middle window in mode
    centerAnimValue.value = 0;

    // Panel slides right
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
    zIndex: 10, // Overwrite in Upper
    backgroundColor: colors.bgChatTablet,
  },
});

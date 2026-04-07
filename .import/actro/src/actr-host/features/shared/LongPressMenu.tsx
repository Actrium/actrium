import React, { useState, useRef } from 'react';
import {
  View,
  StyleSheet,
  Pressable,
  Modal,
  TouchableWithoutFeedback,
  Dimensions,
  LayoutRectangle,
} from 'react-native';
import { AppText } from '../../../components';
import {
  getTypographyStyle,
  longPressMenu,
  spacing,
} from '../../../theme/tokens';
import { HOST_SHADOWS } from '../../styles/shadows';

interface MenuOption {
  label: string;
  onPress: (position?: { top: number; left: number; width: number }) => void;
  icon?: React.ReactNode;
  textColor?: string;
}

interface LongPressMenuProps {
  children: React.ReactNode;
  options: MenuOption[];
  disabled?: boolean;
  onPress?: () => void;
}

export const LongPressMenu: React.FC<LongPressMenuProps> = ({
  children,
  options,
  disabled,
  onPress,
}) => {
  const [visible, setVisible] = useState(false);
  const [menuLayout, setMenuLayout] = useState<LayoutRectangle | null>(null);
  const [pressLocation, setPressLocation] = useState<{ x: number; y: number } | null>(null);
  const triggerRef = useRef<View>(null);
  const pressLocationRef = useRef<{ x: number; y: number } | null>(null);

  const handlePressIn = (event: any) => {
    if (disabled) return;
    
    // 捕获触摸位置
    const nativeEvent = event?.nativeEvent;
    if (nativeEvent) {
      if (nativeEvent.pageX !== undefined && nativeEvent.pageY !== undefined) {
        pressLocationRef.current = {
          x: nativeEvent.pageX,
          y: nativeEvent.pageY,
        };
      } else if (nativeEvent.locationX !== undefined && nativeEvent.locationY !== undefined) {
        // 如果是相对位置，需要转换为绝对位置
        triggerRef.current?.measureInWindow((x, y) => {
          pressLocationRef.current = {
            x: x + nativeEvent.locationX,
            y: y + nativeEvent.locationY,
          };
        });
      }
    }
  };

  const handleLongPress = () => {
    if (disabled) return;

    // 获取触发器在屏幕间的位置
    triggerRef.current?.measureInWindow((x, y, width, height) => {
      setMenuLayout({ x, y, width, height });
      if (pressLocationRef.current) {
        setPressLocation(pressLocationRef.current);
      }
      setVisible(true);
    });
  };

  const close = () => setVisible(false);

  const renderMenu = () => {
    if (!menuLayout) return null;

    const screenHeight = Dimensions.get('window').height;
    const screenWidth = Dimensions.get('window').width;

    // 确定菜单显示在气泡上方还是下方
    const menuWidth = longPressMenu.width;
    const isAbove = menuLayout.y > screenHeight / 2;

    // 计算位置
    const top = isAbove
      ? menuLayout.y - (options.length * 48 + 20) // 简略估算高度 (12+12+24=48 per item)
      : menuLayout.y + menuLayout.height + 8;

    // 左右对齐逻辑：如果有长按位置，使用长按位置；否则使用气泡框中心
    let left: number;
    if (pressLocation) {
      // 使用长按位置，菜单中心对齐到长按位置
      left = pressLocation.x - menuWidth / 2;
    } else {
      // 回退到气泡框中心
      left = menuLayout.x + menuLayout.width / 2 - menuWidth / 2;
    }
    
    // 边界检查
    if (left + menuWidth > screenWidth - 20)
      left = screenWidth - menuWidth - 20;
    if (left < 20) left = 20;

    return (
      <Modal
        visible={visible}
        transparent
        animationType='fade'
        onRequestClose={close}
      >
        <TouchableWithoutFeedback onPress={close}>
          <View style={styles.modalBackdrop}>
            <View
              style={[
                styles.menuContainer,
                {
                  top: Math.max(20, top),
                  left,
                  width: menuWidth,
                },
              ]}
            >
              {options.map((option, index) => (
                <Pressable
                  key={index}
                  style={({ pressed }) => [
                    styles.menuItem,
                    pressed && styles.menuItemPressed,
                  ]}
                  onPress={() => {
                    option.onPress({
                      top: Math.max(20, top),
                      left,
                      width: menuWidth,
                    });
                    close();
                  }}
                >
                  {option.icon}
                  <AppText
                    variant='menuLabel'
                    style={[
                      styles.menuText,
                      option.textColor && { color: option.textColor },
                    ]}
                  >
                    {option.label}
                  </AppText>
                </Pressable>
              ))}
            </View>
          </View>
        </TouchableWithoutFeedback>
      </Modal>
    );
  };

  return (
    <>
      <View ref={triggerRef} collapsable={false}>
        <Pressable
          onPress={() => {
            if (disabled) return;
            onPress?.();
          }}
          onPressIn={handlePressIn}
          onLongPress={handleLongPress}
          delayLongPress={400}
          unstable_pressDelay={100}
          style={({ pressed }) => ({
            opacity: pressed ? 0.8 : 1,
          })}
        >
          {children}
        </Pressable>
      </View>
      {renderMenu()}
    </>
  );
};

const styles = StyleSheet.create({
  modalBackdrop: {
    flex: 1,
    backgroundColor: 'rgba(0,0,0,0.1)', // 极淡的遮罩，突出菜单
  },
  menuContainer: {
    position: 'absolute',
    backgroundColor: '#FFFFFF',
    borderRadius: 34,
    padding: longPressMenu.padding,
    gap: spacing.xxs,
    ...HOST_SHADOWS.menuSurface,
  },
  menuItem: {
    flexDirection: 'row',
    alignItems: 'center',
    paddingVertical: spacing.md,
    paddingHorizontal: spacing.lg,
    borderRadius: 34,
    gap: spacing.md,
  },
  menuItemPressed: {
    backgroundColor: '#F2F2F7',
  },
  menuText: {
    ...getTypographyStyle('longPressMenuLabel'),
    color: '#333333',
    fontWeight: '500',
  },
});

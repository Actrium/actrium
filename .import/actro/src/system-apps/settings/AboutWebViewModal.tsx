import React, { useCallback, useEffect, useRef, useState } from 'react';
import {
  Modal,
  Animated,
  Easing,
  StyleSheet,
  useWindowDimensions,
} from 'react-native';
import { AboutPageKey } from './aboutPages';
import { AboutWebViewPage } from './AboutWebViewPage';

interface AboutWebViewModalProps {
  visible: boolean;
  page: AboutPageKey | null;
  onClose: () => void;
}

export const AboutWebViewModal = ({
  visible,
  page,
  onClose,
}: AboutWebViewModalProps) => {
  const { width } = useWindowDimensions();
  const translateX = useRef(new Animated.Value(width)).current;
  const [modalVisible, setModalVisible] = useState(visible);

  useEffect(() => {
    translateX.setValue(width);
  }, [translateX, width]);

  useEffect(() => {
    if (!visible || !page) {
      return;
    }

    setModalVisible(true);
    // 设置页和登录页统一复用同一个全屏 Modal 壳，保证协议/关于页的进入动画和返回方式一致。
    Animated.timing(translateX, {
      toValue: 0,
      duration: 260,
      easing: Easing.out(Easing.cubic),
      useNativeDriver: true,
    }).start();
  }, [page, translateX, visible]);

  const handleClose = useCallback(() => {
    // 先做退场动画再关闭 Modal，避免底层页面直接闪现。
    Animated.timing(translateX, {
      toValue: width,
      duration: 220,
      easing: Easing.in(Easing.cubic),
      useNativeDriver: true,
    }).start(({ finished }) => {
      if (!finished) {
        return;
      }
      setModalVisible(false);
      onClose();
    });
  }, [onClose, translateX, width]);

  useEffect(() => {
    if (visible) {
      return;
    }
    setModalVisible(false);
  }, [visible]);

  if (!modalVisible || !page) {
    return null;
  }

  return (
    <Modal
      visible={modalVisible}
      transparent
      animationType='none'
      presentationStyle='overFullScreen'
      onRequestClose={handleClose}
    >
      <Animated.View
        style={[
          styles.container,
          {
            transform: [{ translateX }],
          },
        ]}
      >
        <AboutWebViewPage page={page} onBack={handleClose} />
      </Animated.View>
    </Modal>
  );
};

const styles = StyleSheet.create({
  container: {
    flex: 1,
    backgroundColor: '#FFFFFF',
  },
});

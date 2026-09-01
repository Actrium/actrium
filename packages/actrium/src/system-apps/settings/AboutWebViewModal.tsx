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
    // Set page and login page to be used in the same full screen Modal Shell, warranty./The way the page enters animated and returns is the same.
    Animated.timing(translateX, {
      toValue: 0,
      duration: 260,
      easing: Easing.out(Easing.cubic),
      useNativeDriver: true,
    }).start();
  }, [page, translateX, visible]);

  const handleClose = useCallback(() => {
    // We'll do animation before closing. Modal, avoids a direct flash of a bottom page.
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

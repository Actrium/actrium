import React, { useCallback, useEffect, useRef, useState } from 'react';
import {
  ActivityIndicator,
  Animated,
  Easing,
  Platform,
  StyleSheet,
  Text,
  TouchableOpacity,
  View,
} from 'react-native';
import { useTranslation } from 'react-i18next';
import {
  Camera,
  useCameraDevice,
  useCodeScanner,
} from 'react-native-vision-camera';
import { TabEvents } from '../../../native-bridge/tab/TabEvents';
import { GlobalAlert } from '../../../../src/components';

type LoadAppCameraPanelProps = {
  tabId: string;
  cameraActive: boolean;
  onCameraActiveChange: (active: boolean) => void;
  onBeforeOpenCamera?: () => void;
  onAfterCameraPermissionGranted?: () => void | Promise<void>;
  onScanUrl: (url: string) => void;
};

export const LoadAppCameraPanel: React.FC<LoadAppCameraPanelProps> = ({
  tabId,
  cameraActive,
  onCameraActiveChange,
  onBeforeOpenCamera,
  onAfterCameraPermissionGranted,
  onScanUrl,
}) => {
  const { t } = useTranslation();
  const [requesting, setRequesting] = useState(false);
  const [scanContainerHeight, setScanContainerHeight] = useState(0);
  const scanAnimationHeight = 72;
  const scanTravelDistance = Math.max(
    0,
    scanContainerHeight - scanAnimationHeight,
  );
  const device = useCameraDevice('back');
  // 一次打开相机只允许一次有效扫码，
  // 避免同一二维码帧触发多次 onCodeScanned 回调。
  const scanLockedRef = useRef(false);
  const scanLineAnim = useRef(new Animated.Value(0)).current;
  const scanLineLoopRef = useRef<Animated.CompositeAnimation | null>(null);
  const isSupported = Platform.OS === 'ios' || Platform.OS === 'android';

  useEffect(() => {
    // 相机关闭后解锁，确保下一次用户主动扫码仍可正常触发。
    if (!cameraActive) {
      scanLockedRef.current = false;
    }
  }, [cameraActive]);

  useEffect(() => {
    const unsubscribe = TabEvents.onTabExitedFullscreen((event) => {
      if (event.tabId !== tabId) return;
      onCameraActiveChange(false);
      setRequesting(false);
    });

    return () => {
      unsubscribe();
    };
  }, [onCameraActiveChange, tabId]);

  useEffect(() => {
    const shouldPlayScanAnimation =
      cameraActive && device != null && scanTravelDistance > 0;
    if (!shouldPlayScanAnimation) {
      scanLineLoopRef.current?.stop();
      scanLineLoopRef.current = null;
      scanLineAnim.setValue(0);
      return;
    }

    scanLineAnim.setValue(0);
    const loop = Animated.loop(
      Animated.sequence([
        Animated.timing(scanLineAnim, {
          toValue: 1,
          duration: 1400,
          easing: Easing.linear,
          useNativeDriver: false,
        }),
        Animated.timing(scanLineAnim, {
          toValue: 0,
          duration: 0,
          useNativeDriver: false,
        }),
      ]),
    );
    scanLineLoopRef.current = loop;
    loop.start();

    return () => {
      loop.stop();
      scanLineLoopRef.current = null;
    };
  }, [cameraActive, device, scanLineAnim, scanTravelDistance]);

  const handleOpenCamera = useCallback(async () => {
    onBeforeOpenCamera?.();

    if (!isSupported) {
      GlobalAlert.alert('当前平台不支持扫码', '请手动输入 URL 加载应用');
      return;
    }

    if (!device) {
      onCameraActiveChange(true);
      return;
    }

    setRequesting(true);
    try {
      let status = await Camera.getCameraPermissionStatus();
      if (status !== 'granted') {
        status = await Camera.requestCameraPermission();
      }
      if (status === 'granted') {
        void onAfterCameraPermissionGranted?.();
        onCameraActiveChange(true);
      } else {
        console.error('[LoadAppCameraPanel] Camera permission denied:', status);
        GlobalAlert.alert('相机初始化失败', '请稍后重试');
      }
    } catch (e) {
      console.error('[LoadAppCameraPanel] Camera init failed:', e);
      GlobalAlert.alert('相机初始化失败', '请稍后重试');
    } finally {
      setRequesting(false);
    }
  }, [
    device,
    isSupported,
    onAfterCameraPermissionGranted,
    onBeforeOpenCamera,
    onCameraActiveChange,
  ]);

  const codeScanner = useCodeScanner({
    codeTypes: ['qr'],
    onCodeScanned: (codes) => {
      const scannedValue = codes[0]?.value?.trim();
      if (!scannedValue || scanLockedRef.current) return;

      // 先上锁，再立即关闭相机，尽量切断后续重复帧回调。
      scanLockedRef.current = true;
      onCameraActiveChange(false);
      onScanUrl(scannedValue);
    },
  });

  if (!cameraActive) {
    return (
      <View
        style={styles.rootContainer}
        onLayout={(e) => setScanContainerHeight(e.nativeEvent.layout.height)}
      >
        <View style={styles.centerContent}>
          {requesting ? (
            <>
              <ActivityIndicator size='large' color='#34A8FB' />
              <Text style={styles.infoText}>{t('actr.app.requestCameraPermission')}</Text>
            </>
          ) : (
            <TouchableOpacity
              style={styles.openCameraBtn}
              onPress={handleOpenCamera}
              activeOpacity={0.7}
            >
              <Text style={styles.openCameraBtnText}>{t('actr.app.openCameraScan')}</Text>
            </TouchableOpacity>
          )}
        </View>
      </View>
    );
  }

  if (device == null) {
    return (
      <View
        style={styles.rootContainer}
        onLayout={(e) => setScanContainerHeight(e.nativeEvent.layout.height)}
      >
        <View style={styles.centerContent}>
          <ActivityIndicator size='large' color='#34A8FB' />
          <Text style={styles.infoText}>{t('actr.app.initCamera')}</Text>
        </View>
      </View>
    );
  }

  const scanLineTranslateY = scanLineAnim.interpolate({
    inputRange: [0, 1],
    outputRange: [0, scanTravelDistance],
  });

  return (
    <View
      style={styles.rootContainer}
      onLayout={(e) => setScanContainerHeight(e.nativeEvent.layout.height)}
    >
      <View style={styles.cameraContainer}>
        <Camera
          style={StyleSheet.absoluteFill}
          device={device}
          isActive={true}
          codeScanner={codeScanner}
          {...(Platform.OS === 'android'
            ? { androidPreviewViewType: 'texture-view' as const }
            : {})}
        />
        <View style={styles.scanAnimationOverlay} pointerEvents='none'>
          <Animated.Image
            source={require('../../../assets/load/qrcode_animation.png')}
            style={[
              styles.scanAnimation,
              {
                height: scanAnimationHeight,
                transform: [{ translateY: scanLineTranslateY }],
              },
            ]}
            resizeMode='stretch'
          />
        </View>
      </View>
    </View>
  );
};

const styles = StyleSheet.create({
  rootContainer: {
    flex: 1,
  },
  centerContent: {
    flex: 1,
    justifyContent: 'center',
    alignItems: 'center',
    gap: 12,
  },
  infoText: {
    fontSize: 16,
    color: '#666',
    textAlign: 'center',
  },
  openCameraBtn: {
    paddingHorizontal: 32,
    paddingVertical: 14,
    backgroundColor: '#34A8FB',
    borderRadius: 24,
  },
  openCameraBtnText: {
    fontSize: 17,
    color: '#fff',
    fontWeight: '600',
  },
  cameraContainer: {
    flex: 1,
  },
  scanAnimation: {
    position: 'absolute',
    top: 0,
    left: 0,
    width: '100%',
  },
  scanAnimationOverlay: {
    ...StyleSheet.absoluteFillObject,
    zIndex: 10,
    elevation: 10,
  },
});

export default LoadAppCameraPanel;

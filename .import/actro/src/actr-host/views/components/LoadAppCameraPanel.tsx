import React, { useCallback, useEffect, useRef, useState } from 'react';
import {
  ActivityIndicator,
  Animated,
  Easing,
  Linking,
  Platform,
  StyleSheet,
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
import { AppText } from '../../../../src/components';
import { GlobalAlert } from '../../../../src/components';
import { getTypographyStyle, spacing } from '../../../theme/tokens';

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

  const showCameraPermissionAlert = useCallback(() => {
    GlobalAlert.alert(
      t('actr.app.cameraPermissionDeniedTitle'),
      t('actr.app.cameraPermissionDeniedMessage'),
      [
        { text: t('common.cancel'), style: 'cancel' },
        {
          text: t('actr.voice.goToSettings'),
          variant: 'primary',
          onPress: () => Linking.openSettings(),
        },
      ],
    );
  }, [t]);

  const stopScanLineAnimation = useCallback(() => {
    scanLineLoopRef.current?.stop();
    scanLineLoopRef.current = null;
    scanLineAnim.stopAnimation();
    scanLineAnim.setValue(0);
  }, [scanLineAnim]);

  useEffect(() => {
    // 相机关闭后解锁，确保下一次用户主动扫码仍可正常触发。
    if (!cameraActive) {
      scanLockedRef.current = false;
    }
  }, [cameraActive]);

  useEffect(() => {
    // 退出全屏时要关闭相机，重置状态
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
      stopScanLineAnimation();
      return;
    }

    scanLineAnim.setValue(0);
    const loop = Animated.loop(
      Animated.sequence([
        Animated.timing(scanLineAnim, {
          toValue: 1,
          duration: 1400,
          easing: Easing.linear,
          // 位移动画只依赖 transform，交给原生线程执行后，
          // 扫码成功时即使 JS 正在初始化会话，也不会把扫描线拖卡。
          useNativeDriver: true,
        }),
        Animated.timing(scanLineAnim, {
          toValue: 0,
          duration: 0,
          useNativeDriver: true,
        }),
      ]),
    );
    scanLineLoopRef.current = loop;
    loop.start();

    return () => {
      stopScanLineAnimation();
    };
  }, [cameraActive, device, scanLineAnim, scanTravelDistance, stopScanLineAnimation]);

  const handleOpenCamera = useCallback(async () => {
    onBeforeOpenCamera?.();

    if (!isSupported) {
      GlobalAlert.alert('当前平台不支持扫码', '请手动输入 URL 加载应用');
      return;
    }

    setRequesting(true);
    try {
      let status = await Camera.getCameraPermissionStatus();

      // Android 首次调用可能返回 'denied'，需要尝试申请权限
      // iOS 的 'denied' 表示用户已明确拒绝，不应再次申请
      if (status === 'not-determined' || (Platform.OS === 'android' && status === 'denied')) {
        status = await Camera.requestCameraPermission();
      }

      // 系统限制或用户明确拒绝后，引导到系统设置
      if (status === 'denied' || status === 'restricted') {
        showCameraPermissionAlert();
        return;
      }

      if (status === 'granted') {
        // 权限通过后再判断设备是否可用，避免“没权限导致 device 为空”被误判成初始化异常。
        if (!device) {
          console.error('[LoadAppCameraPanel] Camera device unavailable after permission granted');
          GlobalAlert.alert(t('actr.app.cameraInitFailedTitle'), t('actr.app.retryLater'));
          return;
        }
        void onAfterCameraPermissionGranted?.();
        onCameraActiveChange(true);
      } else {
        console.error('[LoadAppCameraPanel] Camera permission denied:', status);
        showCameraPermissionAlert();
      }
    } catch (e) {
      console.error('[LoadAppCameraPanel] Camera init failed:', e);
      GlobalAlert.alert(t('actr.app.cameraInitFailedTitle'), t('actr.app.retryLater'));
    } finally {
      setRequesting(false);
    }
  }, [
    device,
    isSupported,
    onAfterCameraPermissionGranted,
    onBeforeOpenCamera,
    onCameraActiveChange,
    showCameraPermissionAlert,
    t,
  ]);

  const codeScanner = useCodeScanner({
    codeTypes: ['qr'],
    onCodeScanned: (codes) => {
      const scannedValue = codes[0]?.value?.trim();
      if (!scannedValue || scanLockedRef.current) return;

      // 先上锁并立即停掉扫描线，保证“扫到码”这一刻视觉上立刻静止，
      // 避免后续 URL 解析和会话初始化挤占 JS 后出现最后几帧卡顿。
      scanLockedRef.current = true;
      stopScanLineAnimation();
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
              <AppText variant='helperText' style={styles.infoText}>
                {t('actr.app.requestCameraPermission')}
              </AppText>
            </>
          ) : (
            <TouchableOpacity
              style={styles.openCameraBtn}
              onPress={handleOpenCamera}
              activeOpacity={0.7}
            >
              <AppText variant='scanActionLabel' style={styles.openCameraBtnText}>
                {t('actr.app.openCameraScan')}
              </AppText>
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
          <AppText variant='helperText' style={styles.infoText}>
            {t('actr.app.initCamera')}
          </AppText>
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
          isActive={cameraActive}
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
    gap: spacing.md,
  },
  infoText: {
    ...getTypographyStyle('helperText'),
    color: '#666',
    textAlign: 'center',
  },
  openCameraBtn: {
    height: Platform.OS === 'android' ? 44 : 48,
    paddingHorizontal: Platform.OS === 'android' ? 28 : 32,
    backgroundColor: '#34A8FB',
    borderRadius: Platform.OS === 'android' ? 22 : 24,
    alignItems: 'center',
    justifyContent: 'center',
  },
  openCameraBtnText: {
    ...getTypographyStyle('scanActionLabel'),
    color: '#fff',
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

import React, { useCallback, useEffect, useRef, useState } from 'react';
import {
  ActivityIndicator,
  Animated,
  Easing,
  Linking,
  Platform,
  StyleSheet,
  View,
} from 'react-native';
import { useTranslation } from 'react-i18next';
import {
  Camera,
  useCameraDevice,
  useCodeScanner,
} from 'react-native-vision-camera';
import { TabEvents } from '../../../native-bridge/tab/TabEvents';
import { AppText, GlobalAlert } from '../../../components';
import { getTypographyStyle, spacing } from '../../../theme/tokens';
import { useColors } from '../../../theme/useColors';
import type { ColorTokens } from '../../../theme/colors';

type LoadAppCameraPanelProps = {
  tabId: string;
  cameraDesiredActive: boolean;
  cameraActive: boolean;
  onCameraActiveChange: (active: boolean) => void;
  onAfterOpenCameraPermissionDenied?: () => void;
  onAfterCameraPermissionGranted?: () => void | Promise<void>;
  onScanUrl: (url: string) => boolean | void;
};

export const LoadAppCameraPanel: React.FC<LoadAppCameraPanelProps> = ({
  tabId,
  cameraDesiredActive,
  cameraActive,
  onCameraActiveChange,
  onAfterOpenCameraPermissionDenied,
  onAfterCameraPermissionGranted,
  onScanUrl,
}) => {
  const { t } = useTranslation();
  const colors = useColors();
  const styles = React.useMemo(() => makeStyles(colors), [colors]);
  const [requesting, setRequesting] = useState(false);
  const [scanContainerHeight, setScanContainerHeight] = useState(0);
  const scanAnimationHeight = 72;
  const scanTravelDistance = Math.max(
    0,
    scanContainerHeight - scanAnimationHeight,
  );
  const device = useCameraDevice('back');
  // The camera is open at once and only allowed to be scanned at once.
  // Avoids multiple triggers of the same 2D frame onCodeScanned Revert.
  const scanLockedRef = useRef(false);
  const autoOpenAttemptedRef = useRef(false);
  const cameraDesiredActiveRef = useRef(cameraDesiredActive);
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
    cameraDesiredActiveRef.current = cameraDesiredActive;
  }, [cameraDesiredActive]);

  useEffect(() => {
    // When the camera is off and still needs to keep the sweep, it cannot be unlocked immediately.
    // Otherwise, the loading gap after the scrutinizing has been successfully activated again by the same frame, 2D code.
    if (cameraActive || !cameraDesiredActive) {
      scanLockedRef.current = false;
    }
  }, [cameraActive, cameraDesiredActive]);

  useEffect(() => {
    // Close the camera when exiting fullscreen, reset the state
    const unsubscribe = TabEvents.onTabExitedFullscreen((event) => {
      if (event.tabId !== tabId) return;
      autoOpenAttemptedRef.current = false;
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
          // Bitmap only depends on transformAfter the original course,
          // Even when the sweep works. JS Initializing sessions does not allow for the use of scanners.
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
    if (!isSupported) {
      GlobalAlert.alert('Sweeping is not supported on the current platform', 'Please enter manually URL Loading applications');
      return;
    }

    setRequesting(true);
    try {
      let status = await Camera.getCameraPermissionStatus();

      // Android First call is likely to return 'denied', need to try to apply for permission
      // iOS It's... 'denied' Indicates that the user has expressly refused and that the application should not be repeated
      if (status === 'not-determined' || (Platform.OS === 'android' && status === 'denied')) {
        status = await Camera.requestCameraPermission();
      }

      // System restriction or channel to system settings after the user has expressly rejected
      if (status === 'denied' || status === 'restricted') {
        onAfterOpenCameraPermissionDenied?.();
        showCameraPermissionAlert();
        return;
      }

      if (status === 'granted') {
        if (!cameraDesiredActiveRef.current) {
          return;
        }
        // Once the permission is passed, the equipment is judged to be available and avoided“No permission to cause device empty”The blogger says that the government has been wrongly convicted of initialization abnormalities.
        if (!device) {
          console.error('[LoadAppCameraPanel] Camera device unavailable after permission granted');
          GlobalAlert.alert(t('actr.app.cameraInitFailedTitle'), t('actr.app.retryLater'));
          return;
        }
        void onAfterCameraPermissionGranted?.();
        onCameraActiveChange(true);
      } else {
        console.error('[LoadAppCameraPanel] Camera permission denied:', status);
        onAfterOpenCameraPermissionDenied?.();
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
    onAfterOpenCameraPermissionDenied,
    onCameraActiveChange,
    showCameraPermissionAlert,
    t,
  ]);

  useEffect(() => {
    if (!cameraDesiredActive) {
      autoOpenAttemptedRef.current = false;
      scanLockedRef.current = false;
      setRequesting(false);
      return;
    }

    if (cameraActive || requesting || autoOpenAttemptedRef.current) return;

    autoOpenAttemptedRef.current = true;
    void handleOpenCamera();
  }, [cameraActive, cameraDesiredActive, handleOpenCamera, requesting]);

  const codeScanner = useCodeScanner({
    codeTypes: ['qr'],
    onCodeScanned: (codes) => {
      const scannedValue = codes[0]?.value?.trim();
      if (!scannedValue || scanLockedRef.current) return;

      // Lock and stop the scans immediately.“Sweeped to the code.”The moment is now a moment of visual silence.
      // Avoid Follow-up URL Parsing and Initializing Sessions JS And then the last few frames of Carden.
      scanLockedRef.current = true;
      stopScanLineAnimation();
      const shouldReopenCamera = onScanUrl(scannedValue) === true;
      if (shouldReopenCamera) {
        autoOpenAttemptedRef.current = false;
      }
      onCameraActiveChange(false);
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
              <ActivityIndicator size='large' color={colors.accentBlue} />
              <AppText variant='helperText' style={styles.infoText}>
                {t('actr.app.requestCameraPermission')}
              </AppText>
            </>
          ) : null}
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

const makeStyles = (colors: ColorTokens) => {
  return StyleSheet.create({
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
    color: colors.textSecondary,
    textAlign: 'center',
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
};

export default LoadAppCameraPanel;

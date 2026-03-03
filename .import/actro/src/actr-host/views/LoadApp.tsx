import React, {
  useCallback,
  useState,
  useMemo,
  useRef,
  useEffect,
} from 'react';
import {
  Image,
  NativeSyntheticEvent,
  StyleSheet,
  StyleProp,
  TextInput,
  TextInputSubmitEditingEventData,
  TextStyle,
  TouchableWithoutFeedback,
  TouchableOpacity,
  View,
  ViewStyle,
  Text,
  ActivityIndicator,
  ScrollView,
  useWindowDimensions,
  Alert,
  Linking,
  Platform,
  SafeAreaView,
  AppState,
} from 'react-native';
import { useTranslation } from 'react-i18next';
import {
  Camera,
  useCameraDevice,
  useCodeScanner,
} from 'react-native-vision-camera';
import { useTabIdMapStore } from '../store/useTabIdMapStore';
import { useTab } from '../contexts/TabContext';
import { extractAppIdFromUrl } from '../utils';
import { AppSessionService } from '../../services/AppSessionService';
import { BookmarkRepository } from '../../repository/BookmarkRepository';
import { useKeyboardInset } from '../../hooks/useKeyboardInset';
import { HOST_SHADOWS } from '../styles/shadows';
import { setTabBookmarkStatus } from '../../../src/native-bridge/tab/TabModule';

const LoadApp = () => {
  const { t } = useTranslation();
  const [hasPermission, setHasPermission] = useState(false);
  const [cameraActive, setCameraActive] = useState(false);
  const [requesting, setRequesting] = useState(false);
  const [cameraLoadTimedOut, setCameraLoadTimedOut] = useState(false);
  const [isResolving, setIsResolving] = useState(false);
  const { keyboardInset, dismissKeyboard } = useKeyboardInset();
  const isResolvingRef = useRef(false);
  const inputRef = useRef<TextInput>(null);
  const tabCtx = useTab();
  const setTabIdMap = useTabIdMapStore((state) => state.setTabIdMap);
  const { width: screenWidth } = useWindowDimensions();

  // 响应式尺寸：手机端使用屏幕宽度的 85%，iPad 端保持 500
  const isPhone = screenWidth < 600;
  const scanSize = isPhone ? screenWidth * 0.85 : 500;
  const inputWidth = isPhone ? screenWidth * 0.85 : 500;
  const containerGap = isPhone ? 24 : 36;
  const scanBorderRadius = isPhone ? 20 : 34;
  const inputBorderRadius = 24;

  const device = useCameraDevice('back');

  // 若 tab 预设了 appUrl，视为从收藏加载，mount 时自动触发加载，无需用户手动输入
  useEffect(() => {
    if (tabCtx.tab.appUrl) {
      void handleResolveUrl(tabCtx.tab.appUrl);

      // FIXME 要手动再通知 native 更新收藏标识
      void setTabBookmarkStatus(tabCtx.tab.id, true);
    }
  }, []);


  const isSupported = Platform.OS === 'ios' || Platform.OS === 'android';

  const delay = useCallback((ms: number) => {
    return new Promise<void>((resolve) => {
      setTimeout(resolve, ms);
    });
  }, []);

  const requestCameraPermissionWithRetry = useCallback(async () => {
    if (Platform.OS !== 'android') {
      let status = await Camera.getCameraPermissionStatus();
      if (status !== 'granted') {
        status = await Camera.requestCameraPermission();
      }
      return status;
    }

    let status = await Camera.getCameraPermissionStatus();
    if (status === 'granted') return status;

    // Android 端首次进入页面时，PermissionAwareActivity 可能尚未就绪，短暂重试避免 NO_ACTIVITY 误报。
    const maxRetry = 4;
    let lastError: unknown;
    for (let i = 0; i < maxRetry; i += 1) {
      try {
        if (Platform.OS === 'android' && AppState.currentState !== 'active') {
          await delay(120);
          continue;
        }
        return await Camera.requestCameraPermission();
      } catch (error) {
        lastError = error;
        const message =
          error instanceof Error ? error.message : String(error ?? '');
        const isNoActivity = message.includes('NO_ACTIVITY');
        if (!isNoActivity || i === maxRetry - 1) {
          throw error;
        }
        await delay(180);
      }
    }

    if (lastError) throw lastError;
    return status;
  }, [delay]);

  useEffect(() => {
    if (!cameraActive || device) {
      setCameraLoadTimedOut(false);
      return;
    }

    const timer = setTimeout(() => {
      setCameraLoadTimedOut(true);
    }, 2500);

    return () => {
      clearTimeout(timer);
    };
  }, [cameraActive, device]);

  console.log('[LoadApp] Rendering...', {
    hasPermission,
    cameraActive,
    device: !!device,
    isSupported,
    cameraLoadTimedOut,
  });

  const showPermissionDeniedAlert = useCallback(() => {
    Alert.alert('需要相机权限', '请开启相机权限后重试', [
      { text: '取消', style: 'cancel' },
      {
        text: '去设置',
        onPress: () => {
          void Linking.openSettings();
        },
      },
    ]);
  }, []);

  // 点击按钮后才申请相机权限并打开相机
  const handleOpenCamera = useCallback(async () => {
    inputRef.current?.blur();
    dismissKeyboard();

    if (!isSupported) {
      console.log(
        '[LoadApp] Camera not supported on this platform, skipping hardware init.',
      );
      Alert.alert('当前平台不支持扫码', '请手动输入 URL 加载应用');
      return;
    }

    // 模拟器/无摄像头设备：不触发权限请求，直接走 UI 兜底提示
    if (!device) {
      setCameraActive(true);
      return;
    }

    setRequesting(true);
    console.log('[LoadApp] Requesting camera permission...');
    try {
      const status = await requestCameraPermissionWithRetry();

      console.log('[LoadApp] Camera permission status:', status);
      const granted = status === 'granted';
      setHasPermission(granted);
      if (granted) {
        setCameraActive(true);
      } else if (status === 'denied') {
        Alert.alert('未授予相机权限', '请允许相机权限后重试');
      } else {
        showPermissionDeniedAlert();
      }
    } catch (e) {
      console.error('[LoadApp] Camera permission request failed:', e);
      const errorMsg =
        e instanceof Error ? e.message : typeof e === 'string' ? e : '';
      const isNoActivityError = errorMsg.includes('NO_ACTIVITY');
      const maybeMissingManifestPermission =
        errorMsg.includes('android.permission.CAMERA') ||
        errorMsg.includes('AndroidManifest') ||
        errorMsg.toLowerCase().includes('permission');

      if (Platform.OS === 'android' && isNoActivityError) {
        Alert.alert(
          '相机初始化中',
          '页面初始化尚未完成，请稍等 1 秒后再试',
        );
      } else if (Platform.OS === 'android' && maybeMissingManifestPermission) {
        Alert.alert(
          '相机权限配置缺失',
          'Android 宿主工程可能未声明 CAMERA 权限，请在 AndroidManifest.xml 中添加 android.permission.CAMERA',
        );
      } else {
        Alert.alert('相机初始化失败', '请稍后重试');
      }
    } finally {
      setRequesting(false);
    }
  }, [
    device,
    dismissKeyboard,
    isSupported,
    requestCameraPermissionWithRetry,
    showPermissionDeniedAlert,
  ]);

  const codeScanner = useCodeScanner({
    codeTypes: ['qr'],
    onCodeScanned: (codes) => {
      if (!codes.length || !codes[0].value) return;
      void handleResolveUrl(codes[0].value);
    },
  });

  const handleResolveUrl = useCallback(
    async (rawUrl: string) => {
      const url = rawUrl.trim();
      if (!url) return;
      if (isResolvingRef.current) return;

      const appId = extractAppIdFromUrl(url);
      if (!appId) {
        console.error('扫描到的二维码无效:', url);
        return;
      }

      isResolvingRef.current = true;
      setIsResolving(true);
      try {
        const initResult = await AppSessionService.initSession(
          tabCtx.tab.id,
          appId,
          url,
        );
        if (!initResult.ok) {
          if (initResult.reason === 'APP_NOT_FOUND') {
            Alert.alert(t('actr.app.notExist'));
          } else {
            Alert.alert(t('actr.app.loadFailed'), t('actr.app.networkRetry'));
          }
          return;
        }

        setTabIdMap(tabCtx.tab.id, appId, url);
        // 将收藏表中对应的收藏项 tab_id 更新为当前 tab
        void BookmarkRepository.updateTabId(appId, tabCtx.tab.id);
      } catch (error) {
        console.error('[LoadApp] Failed to resolve app from url:', error);
        Alert.alert(t('actr.app.loadFailed'), t('actr.app.retryLater'));
      } finally {
        isResolvingRef.current = false;
        setIsResolving(false);
      }
    },
    [setTabIdMap, tabCtx.tab.id],
  );

  const handleLoadWithUrl = (
    e: NativeSyntheticEvent<TextInputSubmitEditingEventData>,
  ) => {
    // const url = e.nativeEvent.text;
    const url =
      e.nativeEvent.text ||
      'https://gate-askaway.betaenv.com/with/a495b23c-71f1-4f5c-910a-d3d8fa692d1a';
    if (!url) return;

    void handleResolveUrl(url);
  };

  const renderScanContent = () => {
    if (!cameraActive) {
      return (
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
      );
    }

    if (device == null) {
      if (cameraLoadTimedOut) {
        return (
          <View style={styles.centerContent}>
            <Text style={styles.infoText}>未检测到可用相机</Text>
            <Text style={styles.secondaryInfoText}>
              模拟器或无摄像头设备请改用下方 URL 输入
            </Text>
          </View>
        );
      }

      return (
        <View style={styles.centerContent}>
          <ActivityIndicator size='large' color='#34A8FB' />
          <Text style={styles.infoText}>{t('actr.app.initCamera')}</Text>
        </View>
      );
    }

    return (
      <Camera
        style={StyleSheet.absoluteFill}
        device={device}
        isActive={true}
        codeScanner={codeScanner}
      />
    );
  };

  const renderInput = (
    inputStyle: StyleProp<TextStyle>,
    containerStyle: StyleProp<ViewStyle>,
  ) => (
    <View style={containerStyle}>
      <Image
        source={require('../../assets/icons/load_link.png')}
        style={styles.inputPrefix}
      />
      <TouchableWithoutFeedback>
        <TextInput
          ref={inputRef}
          style={inputStyle}
          placeholder={t('actr.app.inputUrl')}
          placeholderTextColor='#999'
          onSubmitEditing={handleLoadWithUrl}
          blurOnSubmit={true}
          editable={!isResolving}
          onFocus={() => {
            if (cameraActive) {
              setCameraActive(false);
            }
          }}
        />
      </TouchableWithoutFeedback>
    </View>
  );

  if (isPhone) {
    return (
      <TouchableWithoutFeedback onPress={dismissKeyboard} accessible={false}>
        <View style={styles.phoneContainer}>
          <View style={styles.phoneScanSection}>
            <View style={styles.phoneScanArea}>{renderScanContent()}</View>
          </View>
          <View style={styles.phoneInputDock}>
            <SafeAreaView style={styles.phoneInputSafeArea}>
              <View style={styles.phoneInputSection}>
                {renderInput(styles.phoneInput, styles.phoneInputContainer)}
              </View>
            </SafeAreaView>
          </View>
        </View>
      </TouchableWithoutFeedback>
    );
  }

  return (
    <TouchableWithoutFeedback onPress={dismissKeyboard} accessible={false}>
      <ScrollView
        contentContainerStyle={[
          styles.scrollContent,
          { paddingBottom: keyboardInset },
        ]}
        keyboardShouldPersistTaps='handled'
      >
        <View style={[styles.LoadAppContainer, { gap: containerGap }]}>
          {/* camera scan qrcode */}
          <View
            style={[
              styles.scanArea,
              {
                width: scanSize,
                height: scanSize,
                borderRadius: scanBorderRadius,
              },
            ]}
          >
            {renderScanContent()}
          </View>

          {/* input app url */}
          {renderInput(
            [
              styles.input,
              { width: inputWidth, borderRadius: inputBorderRadius },
            ],
            [styles.inputContainer, { borderRadius: inputBorderRadius }],
          )}
        </View>
      </ScrollView>
    </TouchableWithoutFeedback>
  );
};

export default LoadApp;

const styles = StyleSheet.create({
  scrollContent: {
    flexGrow: 1,
  },
  phoneContainer: {
    flex: 1,
    width: '100%',
    backgroundColor: '#fff',
  },
  phoneScanSection: {
    flex: 1,
    width: '100%',
  },
  phoneScanArea: {
    flex: 1,
    width: '100%',
    overflow: 'hidden',
    backgroundColor: '#fff',
  },
  phoneInputDock: {
    width: '100%',
    marginTop: 24,
    paddingBottom: 12,
  },
  phoneInputSafeArea: {
    width: '100%',
    backgroundColor: '#fff',
  },
  phoneInputSection: {
    width: '100%',
    alignItems: 'stretch',
    justifyContent: 'flex-end',
    paddingTop: 6,
    paddingHorizontal: 26,
    paddingBottom: 10,
  },
  LoadAppContainer: {
    flex: 1,
    alignItems: 'center',
    justifyContent: 'center',
    flexDirection: 'column',
    // gap 由动态样式控制
  },
  scanArea: {
    // width / height / borderRadius 由动态样式控制
    overflow: 'hidden',
    backgroundColor: '#fff',

    shadowColor: '#000000',
    shadowOpacity: 0.1,
    shadowRadius: 8,
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
  secondaryInfoText: {
    fontSize: 14,
    color: '#888',
    textAlign: 'center',
    paddingHorizontal: 20,
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
  input: {
    // width / borderRadius 由动态样式控制
    paddingHorizontal: 34,
    paddingVertical: 14,
    fontSize: 17,
  },
  inputContainer: {
    position: 'relative',
    backgroundColor: '#fff',
    // borderRadius 由动态样式控制
    ...HOST_SHADOWS.inputPill,
  },
  phoneInputContainer: {
    position: 'relative',
    width: '100%',
    height: 48,
    backgroundColor: '#fff',
    borderRadius: 24,
    ...HOST_SHADOWS.inputPill,
  },
  phoneInput: {
    width: '100%',
    height: 48,
    borderRadius: 24,
    paddingHorizontal: 34,
    paddingVertical: 0,
    fontSize: 17,
  },
  inputPrefix: {
    position: 'absolute',
    left: 11,
    top: '50%',
    marginTop: -9,
    width: 18,
    height: 18,
  },
});

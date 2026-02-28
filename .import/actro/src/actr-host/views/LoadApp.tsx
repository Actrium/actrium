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
  Platform,
  SafeAreaView,
} from 'react-native';
import {
  Camera,
  useCameraDevice,
  useCodeScanner,
  CameraPermissionStatus,
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
  const [hasPermission, setHasPermission] = useState(false);
  const [cameraActive, setCameraActive] = useState(false);
  const [requesting, setRequesting] = useState(false);
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

  // 检查是否为模拟器 (通过简单的硬件特征判断，无需额外依赖)
  const isSimulator = useMemo(() => {
    return Platform.OS === 'ios' && (Platform.isTV || !device);
  }, [device]);

  const isSupported = Platform.OS === 'ios' && !isSimulator;

  console.log('[LoadApp] Rendering...', {
    hasPermission,
    cameraActive,
    device: !!device,
    isSupported,
    isSimulator,
  });

  // 点击按钮后才申请相机权限并打开相机
  const handleOpenCamera = useCallback(async () => {
    inputRef.current?.blur();
    dismissKeyboard();

    if (!isSupported) {
      console.log(
        '[LoadApp] Camera not supported or running on Simulator, skipping hardware init.',
      );
      return;
    }

    setRequesting(true);
    console.log('[LoadApp] Requesting camera permission...');
    try {
      const status = await Camera.requestCameraPermission();
      console.log('[LoadApp] Camera permission status:', status);
      const granted = status === 'granted';
      setHasPermission(granted);
      if (granted) {
        setCameraActive(true);
      }
    } catch (e) {
      console.error('[LoadApp] Camera permission request failed:', e);
    } finally {
      setRequesting(false);
    }
  }, [dismissKeyboard, isSupported]);

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
            Alert.alert('应用不存在');
          } else {
            Alert.alert('加载失败', '请检查网络后重试');
          }
          return;
        }

        setTabIdMap(tabCtx.tab.id, appId, url);
        // 将收藏表中对应的收藏项 tab_id 更新为当前 tab
        void BookmarkRepository.updateTabId(appId, tabCtx.tab.id);
      } catch (error) {
        console.error('[LoadApp] Failed to resolve app from url:', error);
        Alert.alert('加载失败', '请稍后重试');
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
      'https://gate.actrium-beta.example/with/a495b23c-71f1-4f5c-910a-d3d8fa692d1a';
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
              <Text style={styles.infoText}>正在申请相机权限...</Text>
            </>
          ) : (
            <TouchableOpacity
              style={styles.openCameraBtn}
              onPress={handleOpenCamera}
              activeOpacity={0.7}
            >
              <Text style={styles.openCameraBtnText}>打开相机扫码</Text>
            </TouchableOpacity>
          )}
        </View>
      );
    }

    if (device == null) {
      return (
        <View style={styles.centerContent}>
          <ActivityIndicator size='large' color='#34A8FB' />
          <Text style={styles.infoText}>正在初始化相机...</Text>
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
          placeholder='请输入 URL'
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

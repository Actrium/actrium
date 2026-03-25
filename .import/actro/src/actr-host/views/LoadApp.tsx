import React, { useCallback, useState, useRef, useEffect } from 'react';
import {
  InteractionManager,
  NativeSyntheticEvent,
  Platform,
  StyleSheet,
  StyleProp,
  TextInput,
  TextInputSubmitEditingEventData,
  TextStyle,
  TouchableWithoutFeedback,
  View,
  ViewStyle,
  ScrollView,
  useWindowDimensions,
} from 'react-native';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import { useTranslation } from 'react-i18next';
import { useTabIdMapStore } from '../store/useTabIdMapStore';
import { useTab } from '../contexts/TabContext';
import { extractAppIdFromUrl } from '../utils';
import { AppSessionService } from '../../services/AppSessionService';
import { useKeyboardInset } from '../../hooks/useKeyboardInset';
import { HOST_SHADOWS } from '../styles/shadows';
import { setTabBookmarkStatus } from '../../../src/native-bridge/tab/TabModule';
import { TabEvents } from '../../../src/native-bridge/tab/TabEvents';
import { HostIconButton } from '../components/HostIconButton';
import SafeBottomView from '../components/SafeBottomView';
import { Icon } from '../components/Icon';
import { LoadAppCameraPanel } from './components/LoadAppCameraPanel';
import { GlobalAlert } from '../../../src/components';
import { CHAT_HEADER_HEIGHT } from '../constants';
import { isTablet } from '../../utils/device';
import { KeyboardStickyView } from 'react-native-keyboard-controller';
import { useNetworkPermission } from '../hooks/useNetworkPermission';

// 在短时间窗口内抑制同一 URL 的重复加载，
// 典型场景是权限弹窗前后扫码回调抖动导致重复触发。
const RESOLVE_DEDUP_MS = 3000;
// 上游重复触发时，避免“应用不存在”弹窗连续轰炸用户。
const ALERT_DEDUP_MS = 2000;

const LoadApp = () => {
  const { t } = useTranslation();
  const [cameraActive, setCameraActive] = useState(false);
  const [inputExpanded, setInputExpanded] = useState(false);
  const [isResolving, setIsResolving] = useState(false);
  const { keyboardInset, dismissKeyboard } = useKeyboardInset();
  const insets = useSafeAreaInsets();
  const isResolvingRef = useRef(false);
  const pendingResolveUrlRef = useRef<string | null>(null);
  const lastAutoResolvedAppUrlRef = useRef<string | null>(null);
  // 当前正在加载的 URL，用于去重与定位竞态问题。
  const inFlightUrlRef = useRef<string | null>(null);
  // 最近一次完成加载的 URL 信息，用于过滤短时间内的重复回调。
  const lastResolvedUrlRef = useRef<string | null>(null);
  const lastResolvedAtRef = useRef(0);
  const lastNotFoundAlertAtRef = useRef(0);
  const inputRef = useRef<TextInput>(null);
  const tabCtx = useTab();
  const setTabIdMap = useTabIdMapStore((state) => state.setTabIdMap);
  const { width: screenWidth } = useWindowDimensions();
  const { ensureNetworkPermission } = useNetworkPermission();

  // 响应式尺寸：手机端使用屏幕宽度的 85%，iPad 端保持 500
  const isPhone = screenWidth < 600;
  const scanSize = isPhone ? screenWidth * 0.85 : 500;
  const padInputWidth = 600;
  const containerGap = isPhone ? 24 : 36;
  const scanBorderRadius = isPhone ? 20 : 34;
  const inputBorderRadius = 24;
  // Pad 模式不再展示 ChatHeader，因此不需要预留 Header 高度。
  const scanTopMargin = isTablet ? 10 : CHAT_HEADER_HEIGHT + 10;

  useEffect(() => {
    const unsubscribeWillExit = TabEvents.onTabWillExitFullscreen((event) => {
      if (event.tabId !== tabCtx.tab.id) return;
      // 退出动作发起时先同步关闭相机，避免 Native 切页后摄像头仍短暂存活。
      setCameraActive(false);
    });

    return () => {
      unsubscribeWillExit();
    };
  }, [tabCtx.tab.id]);

  // Pad 端输入框常驻展示，Phone 端保持原有“按钮切换输入框”交互。
  const showInputField = isTablet || (inputExpanded && !cameraActive);

  useEffect(() => {
    if (!showInputField) return;
    const timer = setTimeout(() => {
      inputRef.current?.focus();
    }, 0);
    return () => {
      clearTimeout(timer);
    };
  }, [showInputField]);

  const handleResolveUrl = useCallback(
    async (rawUrl: string) => {
      const url = rawUrl.trim();
      if (!url) return;
      const now = Date.now();

      // 保持 single-flight 语义：已有加载进行中时，不再发起新的加载。
      if (isResolvingRef.current) return;
      if (
        lastResolvedUrlRef.current === url &&
        now - lastResolvedAtRef.current < RESOLVE_DEDUP_MS
      ) {
        return;
      }

      const appId = extractAppIdFromUrl(url);
      if (!appId) {
        console.error('扫描到的二维码无效:', url);
        return;
      }

      isResolvingRef.current = true;
      inFlightUrlRef.current = url;
      setIsResolving(true);
      try {
        const initResult = await AppSessionService.initSession(
          tabCtx.tab.id,
          appId,
          url,
        );
        if (!initResult.ok) {
          if (initResult.reason === 'APP_NOT_FOUND') {
            const nowForAlert = Date.now();
            if (
              nowForAlert - lastNotFoundAlertAtRef.current >=
              ALERT_DEDUP_MS
            ) {
              GlobalAlert.alert(t('actr.app.notExist'));
              lastNotFoundAlertAtRef.current = nowForAlert;
            }
          } else {
            GlobalAlert.alert(
              t('actr.app.loadFailed'),
              t('actr.app.networkRetry'),
            );
          }
          return;
        }

        setTabIdMap(tabCtx.tab.id, appId, url);
      } catch (error) {
        console.error('[LoadApp] Failed to resolve app from url:', error);
        GlobalAlert.alert(t('actr.app.loadFailed'), t('actr.app.retryLater'));
      } finally {
        lastResolvedUrlRef.current = url;
        lastResolvedAtRef.current = Date.now();
        isResolvingRef.current = false;
        inFlightUrlRef.current = null;
        setIsResolving(false);
      }
    },
    [setTabIdMap, t, tabCtx.tab.id],
  );

  const enqueueResolveUrl = useCallback(
    async (rawUrl: string) => {
      const url = rawUrl.trim();
      if (!url) return;

      // 首次自动打开收藏 / 扫码时，系统权限弹窗可能还没处理完。
      // 这里先缓存 URL，等权限结果明确后再继续，避免 getAppInfo 抢跑后直接报网络错误。
      pendingResolveUrlRef.current = url;

      const granted = await ensureNetworkPermission();
      if (!granted) {
        pendingResolveUrlRef.current = null;
        GlobalAlert.alert(t('actr.app.loadFailed'), t('actr.app.retryLater'));
        return;
      }

      const nextUrl = pendingResolveUrlRef.current;
      pendingResolveUrlRef.current = null;
      if (!nextUrl) return;

      await handleResolveUrl(nextUrl);
    },
    [ensureNetworkPermission, handleResolveUrl, t],
  );

  useEffect(() => {
    if (!tabCtx.tab.appUrl) return;
    if (lastAutoResolvedAppUrlRef.current === tabCtx.tab.appUrl) return;

    // 预设 appUrl 可能来自收藏恢复，effect 因为 re-render 重跑时不能重复发起初始化。
    lastAutoResolvedAppUrlRef.current = tabCtx.tab.appUrl;
    void enqueueResolveUrl(tabCtx.tab.appUrl);

    // FIXME 要手动再通知 native 更新收藏标识
    void setTabBookmarkStatus(tabCtx.tab.id, true);
  }, [enqueueResolveUrl, tabCtx.tab.appUrl, tabCtx.tab.id]);

  const handleLoadWithUrl = (
    e: NativeSyntheticEvent<TextInputSubmitEditingEventData>,
  ) => {
    const url = e.nativeEvent.text;
    if (!url) return;

    void enqueueResolveUrl(url);
  };

  const renderInput = (
    inputStyle: StyleProp<TextStyle>,
    containerStyle: StyleProp<ViewStyle>,
  ) => {
    if (!showInputField) {
      return (
        <HostIconButton
          iconType='keyboard'
          size={48}
          iconSize={24}
          disableTint
          style={styles.inputToggleBtn}
          activeOpacity={0.8}
          onPress={() => {
            setCameraActive(false);
            setInputExpanded(true);
          }}
        />
      );
    }

    return (
      <View style={containerStyle}>
        <Icon
          type='keyboard'
          size={24}
          disableTint
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
  };

  if (isPhone) {
    return (
      <TouchableWithoutFeedback onPress={dismissKeyboard} accessible={false}>
        <View style={styles.phoneContainer}>
          <View
            style={[
              styles.phoneScanSection,
              // 中文注释：iOS 上输入 Dock 通过 StickyView 浮动到键盘上方，
              // 内容区需要同步让出键盘高度，避免视觉上仍然“顶到底部”。
              { marginBottom: Platform.OS === 'ios' ? keyboardInset : 0 },
            ]}
          >
            <View style={[styles.phoneScanArea, { marginTop: scanTopMargin }]}>
              <LoadAppCameraPanel
                tabId={tabCtx.tab.id}
                cameraActive={cameraActive}
                onCameraActiveChange={setCameraActive}
                onBeforeOpenCamera={() => {
                  setInputExpanded(false);
                  inputRef.current?.blur();
                  dismissKeyboard();
                }}
                onAfterCameraPermissionGranted={() => {
                  void ensureNetworkPermission();
                }}
                onScanUrl={(url) => {
                  // 先让相机关闭和扫码层收起完成一帧提交，再启动会话初始化，
                  // 避免重活直接堵住 JS，导致“已扫中但动画最后卡一下”的观感。
                  InteractionManager.runAfterInteractions(() => {
                    void enqueueResolveUrl(url);
                  });
                }}
              />
            </View>
          </View>
          <KeyboardStickyView
            enabled={Platform.OS === 'ios'}
            style={styles.phoneInputDock}
          >
            <SafeBottomView style={styles.phoneInputSafeArea} minBottom={12}>
              <View style={styles.phoneInputSection}>
                {renderInput(styles.phoneInput, styles.phoneInputContainer)}
              </View>
            </SafeBottomView>
          </KeyboardStickyView>
        </View>
      </TouchableWithoutFeedback>
    );
  }

  return (
    <TouchableWithoutFeedback onPress={dismissKeyboard} accessible={false}>
      <View style={styles.padContainer}>
        {/* Pad 端扫码区保持主体居中，输入框改为底部独立定位，避免受整体居中布局影响。 */}
        <ScrollView
          contentContainerStyle={[
            styles.scrollContent,
            styles.padScrollContent,
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
                  marginTop: scanTopMargin,
                },
              ]}
            >
              <LoadAppCameraPanel
                tabId={tabCtx.tab.id}
                cameraActive={cameraActive}
                onCameraActiveChange={setCameraActive}
                onBeforeOpenCamera={() => {
                  setInputExpanded(false);
                  inputRef.current?.blur();
                  dismissKeyboard();
                }}
                onAfterCameraPermissionGranted={() => {
                  void ensureNetworkPermission();
                }}
                onScanUrl={(url) => {
                  // 先让相机关闭和扫码层收起完成一帧提交，再启动会话初始化，
                  // 避免重活直接堵住 JS，导致“已扫中但动画最后卡一下”的观感。
                  InteractionManager.runAfterInteractions(() => {
                    void enqueueResolveUrl(url);
                  });
                }}
              />
            </View>
          </View>
        </ScrollView>

        {renderInput(
          [
            styles.padInput,
            { width: padInputWidth, borderRadius: inputBorderRadius },
          ],
          [
            styles.padInputContainer,
            {
              width: padInputWidth,
              borderRadius: inputBorderRadius,
              bottom: 42 + keyboardInset,
            },
          ],
        )}
      </View>
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
  padContainer: {
    flex: 1,
    width: '100%',
    backgroundColor: '#fff',
  },
  padScrollContent: {
    paddingBottom: 120,
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
    // paddingBottom: 12,
  },
  phoneInputSafeArea: {
    width: '100%',
    backgroundColor: '#fff',
  },
  phoneInputSection: {
    width: '100%',
    alignItems: 'center',
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
  // Pad 端输入框为常显模式，单独维护尺寸，避免影响 Phone 端 Dock 输入样式。
  padInput: {
    paddingLeft: 39,
    paddingRight: 34,
    height: 48,
    paddingVertical: 0,
    fontSize: 17,
  },
  padInputContainer: {
    position: 'absolute',
    left: '50%',
    height: 48,
    backgroundColor: '#fff',
    marginLeft: -300,
    // width / borderRadius 由动态样式控制
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
    paddingLeft: 39,
    paddingRight: 34,
    paddingVertical: 0,
    fontSize: 17,
  },
  inputPrefix: {
    position: 'absolute',
    left: 11,
    top: '50%',
    marginTop: -12,
  },
  inputToggleBtn: {
    width: 48,
    height: 48,
    borderRadius: 24,
    backgroundColor: '#fff',
    alignItems: 'center',
    justifyContent: 'center',
    ...HOST_SHADOWS.inputPill,
  },
});

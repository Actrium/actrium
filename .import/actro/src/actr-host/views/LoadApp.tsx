import React, { useCallback, useState, useRef, useEffect } from 'react';
import {
  ActivityIndicator,
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
import { useTranslation } from 'react-i18next';
import { useTabIdMapStore } from '../store/useTabIdMapStore';
import { useTab } from '../contexts/TabContext';
import { extractAppIdFromUrl } from '../utils';
import { AppSessionService } from '../../services/AppSessionService';
import { useKeyboardInset } from '../../hooks/useKeyboardInset';
import { HOST_SHADOWS } from '../styles/shadows';
import { setTabBookmarkStatus } from '../../../src/native-bridge/tab/TabModule';
import { HostIconButton } from '../components/HostIconButton';
import SafeBottomView from '../components/SafeBottomView';
import { Icon } from '../components/Icon';
import { LoadAppCameraPanel } from './components/LoadAppCameraPanel';
import { GlobalAlert } from '../../../src/components';
import { CHAT_HEADER_HEIGHT } from '../constants';
import { isPhone, isTablet } from '../../utils/device';
import { KeyboardStickyView } from 'react-native-keyboard-controller';
import { useNetworkPermission } from '../hooks/useNetworkPermission';
import { BookmarkRepository } from '../../repository/BookmarkRepository';
import { AppText } from '../../components';
import { getTypographyStyle, loadApp, spacing } from '../../theme/tokens';

// 在短时间窗口内抑制同一 URL 的重复加载，
// 典型场景是权限弹窗前后扫码回调抖动导致重复触发。
const RESOLVE_DEDUP_MS = 3000;
// 扫码命中后给相机关闭一个很短的提交窗口，
// 再弹错误提示，避免相机收起和弹框挂载抢同一帧导致观感卡顿。
const INVALID_SCAN_ALERT_DELAY_MS = 150;

const extractAlertMessage = (message?: string) => {
  if (!message) return '';

  try {
    const parsed = JSON.parse(message);
    // 接口或底层转发把整包响应序列化进 message 时，只提取真正给用户看的 message 文案。
    if (parsed && typeof parsed === 'object') {
      if (typeof parsed.message === 'string' && parsed.message.trim()) {
        return parsed.message;
      }

      if (
        parsed.data &&
        typeof parsed.data === 'object' &&
        typeof parsed.data.message === 'string' &&
        parsed.data.message.trim()
      ) {
        return parsed.data.message;
      }
    }
  } catch {
    // 非 JSON 字符串说明本身就是可展示文案，直接透传。
  }

  return message;
};

const LoadApp = () => {
  const { t } = useTranslation();
  const tabCtx = useTab();
  const [cameraActive, setCameraActive] = useState(false);
  const [inputExpanded, setInputExpanded] = useState(false);
  const [isResolving, setIsResolving] = useState(false);
  const [isPresetAppUrlLoading, setIsPresetAppUrlLoading] = useState(
    Boolean(tabCtx.tab.appUrl),
  );
  const { keyboardInset, dismissKeyboard } = useKeyboardInset();
  const isResolvingRef = useRef(false);
  const pendingResolveUrlRef = useRef<string | null>(null);
  // 去重状态集合：防止各种场景下的重复加载和重复弹窗
  const dedupeStateRef = useRef({
    // 记录已自动加载过的 appUrl（来自收藏恢复），防止 useEffect 重复执行时再次加载
    lastAutoResolvedUrl: null as string | null,
    // 记录最近一次完成加载的 URL，配合 lastResolvedAt 实现 3 秒内去重
    // 场景：权限弹窗前后可能触发重复回调，或用户快速重复扫同一个码
    lastResolvedUrl: null as string | null,
    // 记录最近一次完成加载的时间戳，用于 3 秒去重窗口判断
    lastResolvedAt: 0,
  });
  const inputRef = useRef<TextInput>(null);
  const setTabIdMap = useTabIdMapStore((state) => state.setTabIdMap);
  const { width: screenWidth } = useWindowDimensions();
  const { ensureNetworkPermission } = useNetworkPermission();

  // 中文注释：Android 宿主已经通过 windowSoftInputMode 处理键盘上移，
  // Pad 端不要再额外叠加 RN 的 keyboardInset，否则扫码区和输入框会被重复抬高。
  const shouldApplyKeyboardInset = !(Platform.OS === 'android' && !isPhone);
  const scanSize = isPhone ? screenWidth * 0.85 : 500;
  const padInputWidth = 600;
  const containerGap = isPhone ? 24 : 36;
  const scanBorderRadius = isPhone ? 20 : 34;
  const inputBorderRadius = 24;
  // Pad 模式不再展示 ChatHeader，因此不需要预留 Header 高度。
  const scanTopMargin = isTablet ? 10 : CHAT_HEADER_HEIGHT + 10;

  // Pad 端输入框常驻展示，Phone 端保持原有“按钮切换输入框”交互。
  const showInputField = isTablet || (inputExpanded && !cameraActive);

  useEffect(() => {
    // 中文注释：Pad 端输入框常驻展示，但进入页面时不再主动抢焦点，仅保留 Phone 端展开输入框后的自动聚焦。
    if (!showInputField || isTablet) return;
    const timer = setTimeout(() => {
      inputRef.current?.focus();
    }, 0);
    return () => {
      clearTimeout(timer);
    };
  }, [showInputField]);

  const renderLoadingView = () => (
    <View style={styles.loadingContainer}>
      <ActivityIndicator size='large' color='#007AFF' />
      <AppText variant='statusBody' style={styles.loadingText}>
        {t('actr.app.loading')}
      </AppText>
    </View>
  );

  const handleResolveUrl = useCallback(
    async (rawUrl: string) => {
      const url = rawUrl.trim();
      if (!url) return;
      const now = Date.now();

      // 保持 single-flight 语义：已有加载进行中时，不再发起新的加载。
      if (isResolvingRef.current) return;
      // 短时间内抑制同一 URL 的重复加载，防止权限弹窗前后回调抖动。
      if (
        dedupeStateRef.current.lastResolvedUrl === url &&
        now - dedupeStateRef.current.lastResolvedAt < RESOLVE_DEDUP_MS
      ) {
        return;
      }

      const appId = extractAppIdFromUrl(url);
      if (!appId) {
        console.error('扫描到的二维码无效:', url);
        // 二维码内容不满足宿主约定时，用户侧也需要立即得到可见反馈，
        // 否则只打日志会表现为”扫了没反应”。
        GlobalAlert.alert(t('actr.app.invalidUrl'), url);
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
          const alertMessage = extractAlertMessage(initResult.message);
          if (alertMessage) {
            GlobalAlert.alert(undefined, alertMessage);
            return;
          }

          if (initResult.reason === 'APP_NOT_FOUND') {
            GlobalAlert.alert(t('actr.app.notExist'));
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
        dedupeStateRef.current.lastResolvedUrl = url;
        dedupeStateRef.current.lastResolvedAt = Date.now();
        isResolvingRef.current = false;
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

  // 使用 ref 保存最新的 enqueueResolveUrl，避免 useEffect 因函数重新创建而重复执行
  const enqueueResolveUrlRef = useRef(enqueueResolveUrl);
  useEffect(() => {
    enqueueResolveUrlRef.current = enqueueResolveUrl;
  }, [enqueueResolveUrl]);

  useEffect(() => {
    if (!tabCtx.tab.appUrl) {
      setIsPresetAppUrlLoading(false);
      return;
    }
    // 防止同一个 appUrl 重复初始化
    if (dedupeStateRef.current.lastAutoResolvedUrl === tabCtx.tab.appUrl)
      return;

    const presetAppUrl = tabCtx.tab.appUrl;

    // 预设 appUrl 说明宿主已经明确要恢复目标应用，
    // 这里先切到加载态，避免用户短暂看到扫码页再跳走。
    setIsPresetAppUrlLoading(true);
    // 标记已处理，防止 effect 重复执行
    dedupeStateRef.current.lastAutoResolvedUrl = presetAppUrl;
    void enqueueResolveUrlRef.current(presetAppUrl).finally(async () => {
      // 这里只关闭当前这轮预设 URL 对应的加载态，
      // 避免旧请求回收时把新一轮 appUrl 恢复流程的 loading 提前关掉。
      if (dedupeStateRef.current.lastAutoResolvedUrl === presetAppUrl) {
        setIsPresetAppUrlLoading(false);
      }

      // 校验应用是否在收藏表中，再设置收藏状态
      const appId = extractAppIdFromUrl(presetAppUrl);
      if (appId) {
        try {
          const bookmark = await BookmarkRepository.getBookmarkByAppId(appId);
          if (bookmark) {
            await setTabBookmarkStatus(tabCtx.tab.id, true);
          }
        } catch (error) {
          console.error('[LoadApp] Failed to check bookmark status:', error);
        }
      }
    });
  }, [tabCtx.tab.appUrl, tabCtx.tab.id]);

  const handleLoadWithUrl = (
    e: NativeSyntheticEvent<TextInputSubmitEditingEventData>,
  ) => {
    const url = e.nativeEvent.text;
    if (!url) return;

    void enqueueResolveUrl(url);
  };

  const handleScannedUrl = useCallback(
    (rawUrl: string) => {
      const url = rawUrl.trim();
      if (!url) return;

      const appId = extractAppIdFromUrl(url);
      if (!appId) {
        console.error('扫描到的二维码无效:', url);
        // 无效二维码不需要走权限校验和初始化链路，
        // 这里只等相机关闭提交后再弹提示，避免和相机收起抢同一帧。
        setTimeout(() => {
          GlobalAlert.alert(t('actr.app.invalidUrl'));
        }, INVALID_SCAN_ALERT_DELAY_MS);
        return;
      }

      // 合法二维码仍然等相机关闭和扫码层收起完成一帧后再继续，
      // 避免真正的初始化重活堵住相机收起动画。
      InteractionManager.runAfterInteractions(() => {
        void enqueueResolveUrl(url);
      });
    },
    [enqueueResolveUrl, t],
  );

  const renderInput = (
    inputStyle: StyleProp<TextStyle>,
    containerStyle: StyleProp<ViewStyle>,
  ) => {
    if (!showInputField) {
      return (
        <HostIconButton
          iconType='keyboard'
          size={loadApp.inputToggleButtonSize}
          iconSize={loadApp.inputToggleIconSize}
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
          size={loadApp.inputPrefixIconSize}
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
    if (isPresetAppUrlLoading) {
      return <View style={styles.phoneContainer}>{renderLoadingView()}</View>;
    }

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
                onScanUrl={handleScannedUrl}
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

  if (isPresetAppUrlLoading) {
    return <View style={styles.padContainer}>{renderLoadingView()}</View>;
  }

  return (
    <TouchableWithoutFeedback onPress={dismissKeyboard} accessible={false}>
      <View style={styles.padContainer}>
        {/* Pad 端扫码区保持主体居中，输入框改为底部独立定位，避免受整体居中布局影响。 */}
        <ScrollView
          contentContainerStyle={[
            styles.scrollContent,
            styles.padScrollContent,
            { paddingBottom: shouldApplyKeyboardInset ? keyboardInset : 0 },
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
                onScanUrl={handleScannedUrl}
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
              bottom: 42 + (shouldApplyKeyboardInset ? keyboardInset : 0),
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
  loadingContainer: {
    flex: 1,
    alignItems: 'center',
    justifyContent: 'center',
    backgroundColor: '#fff',
  },
  loadingText: {
    marginTop: spacing.md,
    color: '#666',
    ...getTypographyStyle('statusBody'),
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
    ...HOST_SHADOWS.softSurface,
  },
  // Pad 端输入框为常显模式，单独维护尺寸，避免影响 Phone 端 Dock 输入样式。
  padInput: {
    paddingLeft: 39,
    paddingRight: 34,
    height: loadApp.inputHeight,
    paddingVertical: 0,
    ...getTypographyStyle(loadApp.inputTypographyVariant),
  },
  padInputContainer: {
    position: 'absolute',
    left: '50%',
    height: loadApp.inputHeight,
    backgroundColor: '#fff',
    marginLeft: -300,
    // width / borderRadius 由动态样式控制
    ...HOST_SHADOWS.inputPill,
  },
  phoneInputContainer: {
    position: 'relative',
    width: '100%',
    height: loadApp.inputHeight,
    backgroundColor: '#fff',
    borderRadius: 24,
    ...HOST_SHADOWS.inputPill,
  },
  phoneInput: {
    width: '100%',
    height: loadApp.inputHeight,
    borderRadius: 24,
    paddingLeft: 39,
    paddingRight: 34,
    paddingVertical: 0,
    ...getTypographyStyle(loadApp.inputTypographyVariant),
  },
  inputPrefix: {
    position: 'absolute',
    left: 11,
    top: '50%',
    marginTop: -(loadApp.inputPrefixIconSize / 2),
  },
  inputToggleBtn: {
    width: loadApp.inputToggleButtonSize,
    height: loadApp.inputToggleButtonSize,
    borderRadius: loadApp.inputToggleButtonSize / 2,
    backgroundColor: '#fff',
    alignItems: 'center',
    justifyContent: 'center',
    ...HOST_SHADOWS.inputPill,
  },
});

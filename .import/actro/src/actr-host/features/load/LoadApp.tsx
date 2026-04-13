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
} from 'react-native';
import { useTranslation } from 'react-i18next';
import { useTabIdMapStore } from '../../store/useTabIdMapStore';
import { useTab } from '../../contexts/TabContext';
import { extractAppIdFromUrl } from '../../utils';
import { AppSessionService } from '../../../services/AppSessionService';
import { useKeyboardInset } from '../../../hooks/useKeyboardInset';
import { HOST_SHADOWS } from '../../styles/shadows';
import { setTabBookmarkStatus } from '../../../native-bridge/tab/TabModule';
import { HostIconButton, SafeBottomView, Icon } from '../../../components';
import { LoadAppCameraPanel } from './LoadAppCameraPanel';
import { GlobalAlert } from '../../../components';
import { CHAT_HEADER_HEIGHT } from '../../constants';
import { isPhone, isTablet } from '../../../utils/device';
import { KeyboardStickyView } from 'react-native-keyboard-controller';
import { useNetworkPermission } from '../../hooks/useNetworkPermission';
import { BookmarkRepository } from '../../../repository/BookmarkRepository';
import { AppText } from '../../../components';
import { getTypographyStyle, loadApp, spacing } from '../../../theme/tokens';
import { useColors } from '../../../theme/useColors';
import type { ColorTokens } from '../../../theme/colors';

type LoadRequestSource = 'preset' | 'scan' | 'input';

type LoadRequest = {
  url: string;
  source: LoadRequestSource;
};

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
  const colors = useColors();
  const styles = React.useMemo(() => makeStyles(colors), [colors]);
  const tabCtx = useTab();
  const [cameraActive, setCameraActive] = useState(false);
  const [inputExpanded, setInputExpanded] = useState(false);
  const [isResolving, setIsResolving] = useState(false);
  const [loadRequest, setLoadRequest] = useState<LoadRequest | null>(null);
  const [padContainerWidth, setPadContainerWidth] = useState(0);
  const { keyboardInset, dismissKeyboard } = useKeyboardInset();
  const isResolvingRef = useRef(false);
  // 去重状态集合：防止各种场景下的重复加载和重复弹窗
  const dedupeStateRef = useRef({
    // 记录已投递过的预设 appUrl，防止 useEffect 重复执行时再次触发同一轮自动恢复
    lastPresetQueuedUrl: null as string | null,
    // 记录最近一次完成加载的 URL，配合 lastResolvedAt 实现 3 秒内去重
    // 场景：权限弹窗前后可能触发重复回调，或用户快速重复扫同一个码
    lastResolvedUrl: null as string | null,
    // 记录最近一次完成加载的时间戳，用于 3 秒去重窗口判断
    lastResolvedAt: 0,
  });
  const inputRef = useRef<TextInput>(null);
  const setTabIdMap = useTabIdMapStore((state) => state.setTabIdMap);
  const { ensureNetworkPermission } = useNetworkPermission();

  // 中文注释：Android 宿主已经通过 windowSoftInputMode 处理键盘上移，
  // Pad 端不要再额外叠加 RN 的 keyboardInset，否则扫码区和输入框会被重复抬高。
  const shouldApplyKeyboardInset = !(Platform.OS === 'android' && !isPhone);
  const scanMaxSize = 500;
  const padInputMaxWidth = 600;
  const padInputBottomOffset = 42;
  const containerGap = isPhone ? 24 : 36;
  const scanBorderRadius = isPhone ? 20 : 34;
  const inputBorderRadius = 24;
  // Pad 模式不再展示 ChatHeader，因此不需要预留 Header 高度。
  const scanTopMargin = isTablet ? 10 : CHAT_HEADER_HEIGHT + 10;
  const effectiveKeyboardInset = shouldApplyKeyboardInset ? keyboardInset : 0;
  const padInputOccupiedHeight =
    loadApp.inputHeight + padInputBottomOffset + effectiveKeyboardInset;
  const padScanSize =
    padContainerWidth > 0 ? Math.min(padContainerWidth * 0.9, scanMaxSize) : scanMaxSize;

  // Pad 端输入框常驻展示，Phone 端保持原有“按钮切换输入框”交互。
  const showInputField = isTablet || (inputExpanded && !cameraActive);
  const showLoading = loadRequest != null || isResolving;

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
      <ActivityIndicator size='large' color={colors.brandPrimary} />
      <AppText variant='statusBody' style={styles.loadingText}>
        {t('actr.app.loading')}
      </AppText>
    </View>
  );

  const handleResolveUrl = useCallback(
    async (url: string) => {
      const appId = extractAppIdFromUrl(url);
      if (!appId) {
        console.error('扫描到的二维码无效:', url);
        // 二维码内容不满足宿主约定时，用户侧也需要立即得到可见反馈，
        // 否则只打日志会表现为”扫了没反应”。
        GlobalAlert.alert(t('actr.app.invalidUrl'), url);
        return;
      }

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
      }
    },
    [setTabIdMap, t, tabCtx.tab.id],
  );

  const enqueueLoadRequest = useCallback(
    async (rawUrl: string, source: LoadRequestSource) => {
      const url = rawUrl.trim();
      if (!url) return;
      const now = Date.now();

      // 中文注释：继续保留原来的 single-flight 语义，
      // 已有加载进行中时，不允许新的扫码/输入/预设恢复插队。
      if (isResolvingRef.current) return;

      // 中文注释：统一入口后仍保留原有短时间同 URL 去重，
      // 防止权限弹窗前后、扫码回调抖动或用户快速重复提交触发重复初始化。
      if (
        dedupeStateRef.current.lastResolvedUrl === url &&
        now - dedupeStateRef.current.lastResolvedAt < RESOLVE_DEDUP_MS
      ) {
        return;
      }

      isResolvingRef.current = true;
      setIsResolving(true);
      setLoadRequest({ url, source });

      try {
        const granted = await ensureNetworkPermission();
        if (!granted) {
          GlobalAlert.alert(t('actr.app.loadFailed'), t('actr.app.retryLater'));
          return;
        }

        await handleResolveUrl(url);

        if (source === 'preset') {
          const appId = extractAppIdFromUrl(url);
          if (!appId) return;

          try {
            const bookmark = await BookmarkRepository.getBookmarkByAppId(appId);
            if (bookmark) {
              await setTabBookmarkStatus(tabCtx.tab.id, true);
            }
          } catch (error) {
            console.error('[LoadApp] Failed to check bookmark status:', error);
          }
        }
      } finally {
        isResolvingRef.current = false;
        setIsResolving(false);
        setLoadRequest(null);
      }
    },
    [ensureNetworkPermission, handleResolveUrl, t, tabCtx.tab.id],
  );

  useEffect(() => {
    if (!tabCtx.tab.appUrl) {
      return;
    }
    const presetAppUrl = tabCtx.tab.appUrl.trim();
    if (!presetAppUrl) return;

    // 中文注释：预设 appUrl 属于宿主恢复语义，只需要对同一值投递一次；
    // 后续真正的加载去重仍由统一的请求入口和完成态时间窗负责。
    if (dedupeStateRef.current.lastPresetQueuedUrl === presetAppUrl) return;

    dedupeStateRef.current.lastPresetQueuedUrl = presetAppUrl;
    void enqueueLoadRequest(presetAppUrl, 'preset');
  }, [tabCtx.tab.appUrl, tabCtx.tab.id]);

  const handleLoadWithUrl = (
    e: NativeSyntheticEvent<TextInputSubmitEditingEventData>,
  ) => {
    const url = e.nativeEvent.text;
    if (!url) return;

    void enqueueLoadRequest(url, 'input');
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
        void enqueueLoadRequest(url, 'scan');
      });
    },
    [enqueueLoadRequest, t],
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
          style={styles.inputToggleBtn}
          iconColor={colors.textSecondary}
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
          color={colors.textSecondary}
          style={styles.inputPrefix}
        />
        <TouchableWithoutFeedback>
          <TextInput
            ref={inputRef}
            style={inputStyle}
            placeholder={t('actr.app.inputUrl')}
            placeholderTextColor={colors.textTertiary}
            onSubmitEditing={handleLoadWithUrl}
            blurOnSubmit={true}
            editable={!showLoading}
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
    if (showLoading) {
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

  if (showLoading) {
    return <View style={styles.padContainer}>{renderLoadingView()}</View>;
  }

  return (
    <TouchableWithoutFeedback onPress={dismissKeyboard} accessible={false}>
      <View
        style={styles.padContainer}
        onLayout={(event) => {
          const { width } = event.nativeEvent.layout;
          if (width !== padContainerWidth) {
            // 中文注释：扫码区需要显式使用同一个数值作为宽高，
            // 这里读取 Pad 可用宽度后再计算 90% 宽度，避免仅靠 aspectRatio 时被其它布局链路拉成长方形。
            setPadContainerWidth(width);
          }
        }}
      >
        {/* Pad 端扫码区保持主体居中，输入框改为底部独立定位，避免受整体居中布局影响。 */}
        <ScrollView
          contentContainerStyle={[
            styles.scrollContent,
            styles.padScrollContent,
            {
              paddingBottom: padInputOccupiedHeight,
            },
          ]}
          keyboardShouldPersistTaps='handled'
        >
          <View
            style={[
              styles.LoadAppContainer,
              {
                gap: containerGap,
              },
            ]}
          >
            {/* camera scan qrcode */}
            <View
              style={[
                styles.scanArea,
                {
                  width: padScanSize,
                  height: padScanSize,
                  borderRadius: scanBorderRadius,
                  marginTop: isTablet ? 0 : scanTopMargin,
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

        <View
          style={[
            styles.padInputDock,
            { bottom: padInputBottomOffset + effectiveKeyboardInset },
          ]}
        >
          {renderInput(
            [
              styles.padInput,
              {
                width: '100%',
                borderRadius: inputBorderRadius,
              },
            ],
            [
              styles.padInputContainer,
              {
                width: '90%',
                maxWidth: padInputMaxWidth,
                borderRadius: inputBorderRadius,
              },
            ],
          )}
        </View>
      </View>
    </TouchableWithoutFeedback>
  );
};

export default LoadApp;

const makeStyles = (colors: ColorTokens) => StyleSheet.create({
  scrollContent: {
    flexGrow: 1,
  },
  phoneContainer: {
    flex: 1,
    width: '100%',
    backgroundColor: colors.bgScanArea,
  },
  padContainer: {
    flex: 1,
    width: '100%',
    backgroundColor: colors.bgScanArea,
  },
  padScrollContent: {
    paddingBottom: 120,
  },
  loadingContainer: {
    flex: 1,
    alignItems: 'center',
    justifyContent: 'center',
    backgroundColor: colors.bgScanArea,
  },
  loadingText: {
    marginTop: spacing.md,
    color: colors.textSecondary,
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
    backgroundColor: colors.bgScanArea,
  },
  phoneInputDock: {
    width: '100%',
    marginTop: 24,
  },
  phoneInputSafeArea: {
    width: '100%',
    backgroundColor: colors.bgScanArea,
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
  },
  scanArea: {
    overflow: 'hidden',
    backgroundColor: colors.bgScanArea,
    ...HOST_SHADOWS.softSurface,
  },
  padInput: {
    paddingLeft: 39,
    paddingRight: 34,
    height: loadApp.inputHeight,
    paddingVertical: 0,
    ...getTypographyStyle(loadApp.inputTypographyVariant),
  },
  padInputDock: {
    position: 'absolute',
    left: 0,
    right: 0,
    alignItems: 'center',
  },
  padInputContainer: {
    height: loadApp.inputHeight,
    backgroundColor: colors.bgScanArea,
    ...HOST_SHADOWS.inputPill,
  },
  phoneInputContainer: {
    position: 'relative',
    width: '100%',
    height: loadApp.inputHeight,
    backgroundColor: colors.bgLoadDark,
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
    color: colors.textSecondary,
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
    backgroundColor: colors.bgLoadDark,
    alignItems: 'center',
    justifyContent: 'center',
    ...HOST_SHADOWS.inputPill,
  },
});

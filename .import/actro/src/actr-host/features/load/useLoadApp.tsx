import React, { useCallback, useState, useRef, useEffect } from 'react';
import {
  ActivityIndicator,
  InteractionManager,
  NativeSyntheticEvent,
  StyleSheet,
  StyleProp,
  TextInput,
  TextInputSubmitEditingEventData,
  TextStyle,
  TouchableWithoutFeedback,
  View,
  ViewStyle,
  TouchableOpacity,
} from 'react-native';
import { useTranslation } from 'react-i18next';
import { useTabIdMapStore } from '../../store/useTabIdMapStore';
import { useTab } from '../../contexts/TabContext';
import { extractAppIdFromUrl } from '../../utils';
import { AppSessionService } from '../../../services/AppSessionService';
import { useKeyboardInset } from '../../../hooks/useKeyboardInset';
import { Icon, GlobalAlert, AppText } from '../../../components';
import { isTablet } from '../../../utils/device';
import { useNetworkPermission } from '../../hooks/useNetworkPermission';
import { MessageRepository } from '../../../repository/MessageRepository';
import { useMessageStore } from '../../store/useMessageStore';
import { useBookmarkStore } from '../../store/useBookmarkStore';
import { getTypographyStyle, loadApp, spacing } from '../../../theme/tokens';
import { useColors } from '../../../theme/useColors';
import type { ColorTokens } from '../../../theme/colors';
import { AppRepository } from '../../../repository/AppRepository';
import { updateTabInfo } from '../../../native-bridge/tab/TabModule';
import { TabEvents } from '../../../native-bridge/tab/TabEvents';
import { useAppStore } from '../../store/useAppStore';

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
// Phone 端先完成扫码区淡出，再触发输入框 focus 和键盘吸附，
// 避免两段动画叠在一起把居中的扫码内容重新推位。
const PHONE_INPUT_FOCUS_DELAY_MS = 0;

// Phone 和 Pad 共用的样式
const makeSharedStyles = (colors: ColorTokens) =>
  StyleSheet.create({
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
    inputPrefix: {
      position: 'absolute',
      left: 11,
      top: '50%',
      marginTop: -(loadApp.inputPrefixIconSize / 2),
    },
    inputClearButton: {
      position: 'absolute',
      right: 8,
      top: '50%',
      marginTop: -(loadApp.inputPrefixIconSize / 2),
      alignItems: 'center',
      justifyContent: 'center',
    },
  });

export const useLoadApp = () => {
  const { t } = useTranslation();
  const colors = useColors();
  const sharedStyles = React.useMemo(() => makeSharedStyles(colors), [colors]);
  const tabCtx = useTab();
  const [cameraActive, setCameraActive] = useState(false);
  const [cameraDesiredActive, setCameraDesiredActive] = useState(false);
  const [inputExpanded, setInputExpanded] = useState(false);
  const [inputFocused, setInputFocused] = useState(false);
  const [inputValue, setInputValue] = useState('');
  const [isResolving, setIsResolving] = useState(false);
  const [tabFullscreenActive, setTabFullscreenActive] = useState(true);
  const [loadRequest, setLoadRequest] = useState<LoadRequest | null>(null);
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
  const cleanMessage = useMessageStore((state) => state.cleanMessage);
  const { ensureNetworkPermission } = useNetworkPermission();

  const showLoading = loadRequest != null || isResolving;
  const shouldRequestCamera =
    tabFullscreenActive &&
    !showLoading &&
    (isTablet ? !inputFocused : !inputExpanded);

  useEffect(() => {
    // 仅当 Phone 端切到输入框展开态时再主动聚焦，
    // 并且等扫码区淡出完成后再触发键盘，避免淡出阶段发生布局重算。
    if (!inputExpanded || isTablet) return;
    const timer = setTimeout(() => {
      inputRef.current?.focus();
    }, PHONE_INPUT_FOCUS_DELAY_MS);
    return () => {
      clearTimeout(timer);
    };
  }, [inputExpanded, isTablet]);

  useEffect(() => {
    // 相机期望状态只由页面可交互条件驱动，
    // 真正打开相机仍交给 CameraPanel 做权限和设备检查。
    setCameraDesiredActive(shouldRequestCamera);
    if (!shouldRequestCamera) {
      setCameraActive(false);
    }
  }, [shouldRequestCamera]);

  useEffect(() => {
    const unsubscribeEntered = TabEvents.onTabEnteredFullscreen((event) => {
      if (event.tabId !== tabCtx.tab.id) return;
      // 重新进入全屏时恢复条件判断，由上面的 shouldRequestCamera 统一决定是否开相机。
      setTabFullscreenActive(true);
    });

    const unsubscribeExited = TabEvents.onTabExitedFullscreen((event) => {
      if (event.tabId !== tabCtx.tab.id) return;
      // 退出全屏要立刻关闭相机并清掉期望状态，避免后台 tab 继续占用摄像头。
      setTabFullscreenActive(false);
      setCameraDesiredActive(false);
      setCameraActive(false);
    });

    return () => {
      unsubscribeEntered();
      unsubscribeExited();
    };
  }, [tabCtx.tab.id]);

  const renderLoadingView = () => (
    <View style={sharedStyles.loadingContainer}>
      <ActivityIndicator size="large" color={colors.brandPrimary} />
      <AppText variant="statusBody" style={sharedStyles.loadingText}>
        {t('actr.app.loading')}
      </AppText>
    </View>
  );

  const extractAlertMessage = (message?: string) => {
    if (!message) return '';

    try {
      const parsed = JSON.parse(message);
      // 接口或底层转发把整包响应序列化进 message 时，只提取真正给用户看的 message 文案。
      if (parsed && typeof parsed === 'object') {
        if (typeof parsed.message === 'string' && parsed.message.trim()) {
          return parsed.message;
        }

        if (typeof parsed.data === 'object' && parsed.data?.code === 10010) {
          return t('actr.app.notExist');
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

  const handleResolveUrl = useCallback(
    async (url: string, source: LoadRequestSource) => {
      const appId = extractAppIdFromUrl(url);
      if (!appId) {
        console.error('扫描到的二维码无效:', url);
        // 二维码内容不满足宿主约定时，用户侧也需要立即得到可见反馈，
        // 否则只打日志会表现为"扫了没反应"。
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
          if (initResult.reason === 'APP_NOT_FOUND') {
            await useAppStore.getState().updateAppStatus(appId, 'dead');
          }

          // 只有宿主透传的 preset appUrl 才允许走"历史会话兜回 Chat"；
          // 手输和扫码仍保持现有语义，避免把真实无效地址误恢复成旧会话。
          if (source === 'preset' && initResult.reason === 'APP_NOT_FOUND') {
            const hasHistory =
              await MessageRepository.hasMessagesByTabIdAndAppId(
                tabCtx.tab.id,
                appId,
              );

            if (hasHistory) {
              const localAppInfo = await AppRepository.getApp(appId);
              GlobalAlert.alert(t('actr.app.notExist'));
              // 预设恢复命中历史消息但线上应用已不存在时，
              // tab 仍保留本地历史展示能力，同时显式把 native tab 状态标成 dead。
              void updateTabInfo({
                tabId: tabCtx.tab.id,
                title: localAppInfo?.instance_name || tabCtx.tab.title,
                icon: localAppInfo?.info?.logo || '',
                appUrl: url,
                status: 'dead',
              });
              setTabIdMap(tabCtx.tab.id, appId, url);
              return;
            }

            // 预设恢复失败且当前 tab 没有对应历史消息时，
            // 说明这个 tab 上残留的数据已经不再可信，需要同时清掉内存和 SQLite。
            cleanMessage(tabCtx.tab.id);
            await MessageRepository.deleteMessagesByTabId(tabCtx.tab.id);
          }

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
    [cleanMessage, setTabIdMap, t, tabCtx.tab.id],
  );

  const enqueueLoadRequest = useCallback(
    async (rawUrl: string, source: LoadRequestSource) => {
      const url = rawUrl.trim();
      if (!url) return;
      const now = Date.now();

      // 继续保留原来的 single-flight 语义，
      // 已有加载进行中时，不允许新的扫码/输入/预设恢复插队。
      if (isResolvingRef.current) return;

      // 统一入口后仍保留原有短时间同 URL 去重，
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

        await handleResolveUrl(url, source);

        if (source === 'preset') {
          const appId = extractAppIdFromUrl(url);
          if (!appId) return;

          try {
            await useBookmarkStore.getState().loadBookmarks();
            await useBookmarkStore.getState().syncNativeBookmarkStatusByAppId({
              appId,
            });
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

    // 预设 appUrl 属于宿主恢复语义，只需要对同一值投递一次；
    // 后续真正的加载去重仍由统一的请求入口和完成态时间窗负责。
    if (dedupeStateRef.current.lastPresetQueuedUrl === presetAppUrl) return;

    dedupeStateRef.current.lastPresetQueuedUrl = presetAppUrl;
    void enqueueLoadRequest(presetAppUrl, 'preset');
  }, [tabCtx.tab.appUrl, tabCtx.tab.id]);

  const submitInputUrl = useCallback(
    (rawUrl: string) => {
      const url = rawUrl.trim();
      if (!url) return;

      void enqueueLoadRequest(url, 'input');
    },
    [enqueueLoadRequest],
  );

  const handleLoadWithUrl = (
    e: NativeSyntheticEvent<TextInputSubmitEditingEventData>,
  ) => {
    const url = e.nativeEvent.text;
    if (!url) return;

    submitInputUrl(url);
  };

  const handleScannedUrl = useCallback(
    (rawUrl: string) => {
      const url = rawUrl.trim();
      if (!url) return true;

      const appId = extractAppIdFromUrl(url);
      if (!appId) {
        console.error('扫描到的二维码无效:', url);
        // 无效二维码不需要走权限校验和初始化链路，
        // 这里只等相机关闭提交后再弹提示，避免和相机收起抢同一帧。
        setTimeout(() => {
          GlobalAlert.alert(t('actr.app.invalidUrl'));
        }, INVALID_SCAN_ALERT_DELAY_MS);
        return true;
      }

      // 合法二维码仍然等相机关闭和扫码层收起完成一帧后再继续，
      // 避免真正的初始化重活堵住相机收起动画。
      InteractionManager.runAfterInteractions(() => {
        void enqueueLoadRequest(url, 'scan');
      });
      return false;
    },
    [enqueueLoadRequest, t],
  );

  const renderInput = (
    inputStyle: StyleProp<TextStyle>,
    containerStyle: StyleProp<ViewStyle>,
    options?: { showPrefixIcon?: boolean },
  ) => {
    const showPrefixIcon = options?.showPrefixIcon ?? true;
    return (
      <View style={containerStyle}>
        {showPrefixIcon ? (
          <Icon
            type="keyboard"
            size={loadApp.inputPrefixIconSize}
            color={colors.textIconStrong}
            style={sharedStyles.inputPrefix}
          />
        ) : null}
        <TouchableWithoutFeedback>
          <TextInput
            ref={inputRef}
            style={inputStyle}
            value={inputValue}
            placeholder={t('actr.app.inputUrl')}
            placeholderTextColor={colors.textTertiary}
            onChangeText={setInputValue}
            onSubmitEditing={handleLoadWithUrl}
            enterKeyHint="done"
            returnKeyType="done"
            returnKeyLabel="确认"
            blurOnSubmit={true}
            editable={!showLoading}
            onFocus={() => {
              setInputFocused(true);
              if (cameraActive) {
                setCameraActive(false);
              }
            }}
            onBlur={() => {
              setInputFocused(false);
            }}
          />
        </TouchableWithoutFeedback>
        {inputValue.trim() ? (
          <TouchableOpacity
            style={sharedStyles.inputClearButton}
            activeOpacity={0.7}
            onPress={() => {
              setInputValue('');
              inputRef.current?.focus();
            }}
          >
            <Icon
              type="input_clear"
              size={loadApp.inputPrefixIconSize}
              color={colors.textIconSecondary}
            />
          </TouchableOpacity>
        ) : null}
      </View>
    );
  };

  return {
    cameraActive,
    cameraDesiredActive,
    setCameraActive,
    inputExpanded,
    setInputExpanded,
    inputRef,
    inputFocused,
    inputValue,
    showLoading,
    keyboardInset,
    tabCtx,
    dismissKeyboard,
    ensureNetworkPermission,
    handleScannedUrl,
    submitInputUrl,
    renderInput,
    renderLoadingView,
  };
};

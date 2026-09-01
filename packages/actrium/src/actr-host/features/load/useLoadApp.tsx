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
import { authIsClientAppNotFoundResponse } from '../../coreContract';

type LoadRequestSource = 'preset' | 'scan' | 'input';

type LoadRequest = {
  url: string;
  source: LoadRequestSource;
};

// Cursing the same within a short window URL The new article is a new one.
// The typical scene is a back-coated swab of the right-barre window, which causes a repetition to trigger.
const RESOLVE_DEDUP_MS = 3000;
// The camera was shut down by a short submission window after the scrutinizing hit.
// Plays a false hint to avoid the same frame as the frame of the camera to close and the frame to the frame to make the carton look like it.
const INVALID_SCAN_ALERT_DELAY_MS = 150;
// Phone First you finish the scan and then trigger the input box focus And the keyboard suffocates,
// The project will be a new one, and it will be a new one.
const PHONE_INPUT_FOCUS_DELAY_MS = 0;

// Phone and Pad Common Styles
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
  // Deweighting assembly: prevent reloading and re-loading of bullet windows in various scenarios
  const dedupeStateRef = useRef({
    // Record presets for delivered appUrl♪ Prevention useEffect Repeating the automatic resumption of the same round
    lastPresetQueuedUrl: null as string | null,
    // Record last loaded URL- Yes, sir. lastResolvedAt Achieved 3 Heavy in seconds.
    // scene: permission to bomb windows may trigger a repeat or user quickly repeats the same code
    lastResolvedUrl: null as string | null,
    // Record the latest time stamp that was loaded for use in 3 Second to weigh the window
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
    // Just when Phone The end is then actively focused when you reach the input box to expand the state.
    // And triggers the keyboard when the sweep area is out of the loop and avoids a layout rerun at the fadeout stage.
    if (!inputExpanded || isTablet) return;
    const timer = setTimeout(() => {
      inputRef.current?.focus();
    }, PHONE_INPUT_FOCUS_DELAY_MS);
    return () => {
      clearTimeout(timer);
    };
  }, [inputExpanded, isTablet]);

  useEffect(() => {
    // The camera's expectation is driven only by the interactive conditions of the page.
    // The real open camera is still in hand. CameraPanel (c) Performing a check of the rights and equipment.
    setCameraDesiredActive(shouldRequestCamera);
    if (!shouldRequestCamera) {
      setCameraActive(false);
    }
  }, [shouldRequestCamera]);

  useEffect(() => {
    const unsubscribeEntered = TabEvents.onTabEnteredFullscreen((event) => {
      if (event.tabId !== tabCtx.tab.id) return;
      // Resume condition determination when re-entering full screen, by shouldRequestCamera Make a unified decision on whether to turn on the camera.
      setTabFullscreenActive(true);
    });

    const unsubscribeExited = TabEvents.onTabExitedFullscreen((event) => {
      if (event.tabId !== tabCtx.tab.id) return;
      // Exit full screen to shut down the camera and clear the desired state and avoid the backstage. tab Keep taking the camera.
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
      // Interface or bottom-relay to sequence the entire response package message , only the actual user is shown. message The case.
      if (parsed && typeof parsed === 'object') {
        if (typeof parsed.message === 'string' && parsed.message.trim()) {
          return parsed.message;
        }

        if (
          typeof parsed.data === 'object' &&
          authIsClientAppNotFoundResponse(parsed.data)
        ) {
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
      // Not JSON String descriptions are themselves a display of the script and are directly accessible.
    }

    return message;
  };

  const handleResolveUrl = useCallback(
    async (url: string, source: LoadRequestSource) => {
      const appId = extractAppIdFromUrl(url);
      if (!appId) {
        console.error('The 2D code scanned is invalid.:', url);
        // The user side also needs to receive immediate and visible feedback when the content of the 2D code does not meet the host's agreement.
        // Otherwise, it'll be a journal."It's clean and no response.".
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

          // Only the host. preset appUrl I'm not allowed to go."History Talk Back Chat";
          // Hand-dropping and scrutinizing remain the current semantics and avoid the misdirection of a valid invalid address into an old session.
          if (source === 'preset' && initResult.reason === 'APP_NOT_FOUND') {
            const hasHistory =
              await MessageRepository.hasMessagesByTabIdAndAppId(
                tabCtx.tab.id,
                appId,
              );

            if (hasHistory) {
              const localAppInfo = await AppRepository.getApp(appId);
              GlobalAlert.alert(t('actr.app.notExist'));
              // The first time I was in the country, I was in the middle of a new era of life, and I was in the middle of a new era of life.
              // tab We're still keeping our local history displays, while we're showing them. native tab Status Mark dead.
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

            // Preset recovery failed and current tab The blogger says that the government is not a party to the law.
            // That's what it means. tab The data remaining on the site is no longer credible and requires both memory and memory removal. SQLite.
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

      // Keep the original single-flight Semantics,
      // New sweeps are not allowed when loading is already in progress/Input/Pre-set recovery plugs.
      if (isResolvingRef.current) return;

      // Keep the same for the first time after the unified entrance URL Heavy,
      // Prevents the initialization of a power-barrel window, a sweep back and forth, or a user ' s rapid duplicate submission.
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

    // Preset appUrl (a) is the semantic of the host, which needs only one input of the same value;
    // The real load-taking of the follow-up remains the responsibility of the unified request entry and the completion time window.
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
        console.error('The 2D code scanned is invalid.:', url);
        // The two-dimensional code is not valid without the permission to check and initialize the link.
        // Only when the camera is closed and submitted will the hint be played so that the same frame is avoided as the camera.
        setTimeout(() => {
          GlobalAlert.alert(t('actr.app.invalidUrl'));
        }, INVALID_SCAN_ALERT_DELAY_MS);
        return true;
      }

      // The two-dimensional code is still waiting for the camera to close and the scaffolding to be completed.
      // The video shows the video of the video, which is a video of the video of the video, and the video of the video of the video, which shows the video of the video.
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
            returnKeyLabel="Confirm."
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

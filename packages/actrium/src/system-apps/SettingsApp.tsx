/**
 * Settings System App - Built-in settings page
 */
import React, { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { Animated, BackHandler, Easing, Platform, View, StyleSheet, useWindowDimensions } from 'react-native';
import { useTranslation } from 'react-i18next';
import DeviceInfo from 'react-native-device-info';
import { SettingsMain } from './settings/SettingsMain';
import { AccountDetail } from './settings/AccountDetail';
import { AboutActrium } from './settings/AboutActrium';
import { NotificationSettingsPage } from './settings/NotificationSettings';
import { LoginModal } from './auth/LoginModal';
import { useUserStore } from '../runtime-host/store/useUserStore';
import { useAuthTaskStore } from '../runtime-host/store/useAuthTaskStore';
import { createFeedbackAction, createLogoutAction, createDeleteAccountAction } from './settings/actions';
import type { AboutPageKey } from './settings/AboutActrium';
import { AuthRepository } from '../repository/AuthRepository';
import { authApi, CURRENT_ENV } from '../api/auth';

const SETTINGS_CLIENT_ID = '02094310-3454-4929-80b5-8ee85d09f34b';
export type SettingsPage = 'main' | 'accountDetail' | 'aboutActrium' | 'notificationSettings';

// Page Toggle Animation Length
const SLIDE_DURATION = 300;

interface SettingsAppProps {
  systemAppPayload?: Record<string, unknown>;
}

type SettingsAction = 'feedback' | 'logout';

interface SettingsDeepLinkPayload {
  page?: SettingsPage;
  aboutPage?: AboutPageKey;
  action?: SettingsAction;
  token?: string | number;
}

const isRecord = (v: unknown): v is Record<string, unknown> =>
  typeof v === 'object' && v !== null && !Array.isArray(v);

const parseSettingsDeepLinkPayload = (
  payload?: Record<string, unknown>,
): SettingsDeepLinkPayload => {
  if (!payload || !isRecord(payload)) return {};
  const page = payload.page;
  const aboutPage = payload.aboutPage;
  const action = payload.action;
  const token = payload.token;

  return {
    page:
      page === 'main' || page === 'accountDetail' || page === 'aboutActrium' || page === 'notificationSettings'
        ? page
        : undefined,
    aboutPage:
      aboutPage === 'aboutApp' ||
      aboutPage === 'userAgreement' ||
      aboutPage === 'privacyPolicy' ||
      aboutPage === 'icp'
        ? aboutPage
        : undefined,
    action: action === 'feedback' || action === 'logout' ? action : undefined,
    token:
      typeof token === 'string' || typeof token === 'number'
        ? token
        : undefined,
  };
};

export default function SettingsApp({
  systemAppPayload,
}: SettingsAppProps) {
  const { t } = useTranslation();
  const { width } = useWindowDimensions();
  const isTablet = width >= 768;
  const [currentPage, setCurrentPage] = useState<SettingsPage>('main');
  const [aboutInitialPage, setAboutInitialPage] = useState<AboutPageKey | undefined>(
    undefined,
  );
  const [loginModalVisible, setLoginModalVisible] = useState(false);

  // Page Toggle Animations
  const [overlayPage, setOverlayPage] = useState<SettingsPage | null>(null);
  const slideAnim = useRef(new Animated.Value(0)).current;
  const isAnimating = useRef(false);

  const clearUser = useUserStore((state) => state.clearUser);
  const userInfo = useUserStore((state) => state.userInfo);
  const sessionToken = useUserStore((state) => state.sessionToken);
  const setUserInfo = useUserStore((state) => state.setUserInfo);
  const setSessionToken = useUserStore((state) => state.setSessionToken);
  const clearAuthorizationCache = useAuthTaskStore(
    (state) => state.clearAuthorizationCache,
  );
  const consumedPageTokensRef = useRef<Set<string>>(new Set());
  const consumedActionTokensRef = useRef<Set<string>>(new Set());

  const handleFeedbackPress = useMemo(() => createFeedbackAction({ t }), [t]);

  const handleLogout = useMemo(
    () =>
      createLogoutAction({
        t,
        clearUser,
        clearAuthorizationCache,
        onAfterLogout: () => setCurrentPage('main'),
      }),
    [t, clearAuthorizationCache, clearUser],
  );

  const deepLink = useMemo(
    () => parseSettingsDeepLinkPayload(systemAppPayload),
    [systemAppPayload],
  );
  const appVersionText = useMemo(() => {
    const version = DeviceInfo.getVersion();
    const buildNumber = DeviceInfo.getBuildNumber();

    // Show only version numbers when construction numbers are empty, avoiding setting meaningless blank brackets on pages
    return buildNumber ? `${version}(${buildNumber})` : version;
  }, []);

  useEffect(() => {
    let cancelled = false;

    const ensureUserInfo = async () => {
      // Sets the page to try to restore the display only once when it is first mounted.
      // Follow-up userInfo/sessionToken The change is no longer a reverse trigger recovery and the old user is not replaced by the cleanup login.
      const currentUserInfo = useUserStore.getState().userInfo;
      if (currentUserInfo) {
        return;
      }

      const token = await AuthRepository.getToken(CURRENT_ENV);
      if (!token) {
        return;
      }

      // Set page display login information dependent userInfoJust token , and then re-raised it with the active re-raised after the mount
      try {
        const userRes = await authApi.getUserInfo(token);
        if (!userRes.success || !userRes.data || cancelled) {
          return;
        }

        setUserInfo(userRes.data);
        setSessionToken(token);
      } catch (error) {
        console.error('[SettingsApp] Failed to fetch user info on mount:', error);
      }
    };

    ensureUserInfo();

    return () => {
      cancelled = true;
    };
  }, [setSessionToken, setUserInfo]);

  useEffect(() => {
    // Animation is performed by jumping through the deep chain to avoid a state conflict.
    if (!deepLink.page || isAnimating.current) {
      return;
    }

    const deepLinkToken =
      deepLink.token === undefined || deepLink.token === null
        ? ''
        : String(deepLink.token);

    // Page Deep Chains token Only once, to avoid duplication. props Update to drag user back to second page
    if (deepLinkToken && !consumedPageTokensRef.current.has(deepLinkToken)) {
      consumedPageTokensRef.current.add(deepLinkToken);
      setCurrentPage(deepLink.page);
      setAboutInitialPage(deepLink.aboutPage);
    }

    // action Deep chains are heavy, avoiding the same. token Duplicate during reloading
    if (deepLink.action && deepLinkToken) {
      if (consumedActionTokensRef.current.has(deepLinkToken)) {
        return;
      }
      consumedActionTokensRef.current.add(deepLinkToken);
      if (deepLink.action === 'feedback') {
        handleFeedbackPress();
      } else if (deepLink.action === 'logout') {
        handleLogout();
      }
    }
  }, [
    deepLink,
    handleFeedbackPress,
    handleLogout,
  ]);

  const handleLoginPress = () => {
    setLoginModalVisible(true);
  };

  const handleDeleteAccount = useMemo(
    () =>
      createDeleteAccountAction({
        t,
        sessionToken,
        clearUser,
        clearAuthorizationCache,
        onAfterDelete: () => setCurrentPage('main'),
      }),
    [t, sessionToken, clearUser, clearAuthorizationCache],
  );

  // Slide new page from right
  const navigateTo = useCallback((page: SettingsPage) => {
    if (isAnimating.current) return;
    isAnimating.current = true;

    slideAnim.setValue(width);
    setOverlayPage(page);

    Animated.timing(slideAnim, {
      toValue: 0,
      duration: SLIDE_DURATION,
      useNativeDriver: true,
      easing: Easing.bezier(0.25, 0.1, 0.25, 1),
    }).start(({ finished }) => {
      if (finished) {
        setCurrentPage(page);
        setOverlayPage(null);
      }
      isAnimating.current = false;
    });
  }, [width, slideAnim]);

  // Slides Right on Current Page
  const goBack = useCallback(() => {
    if (isAnimating.current || currentPage === 'main') return;
    isAnimating.current = true;

    const page = currentPage;
    slideAnim.setValue(0);
    setOverlayPage(page);
    setCurrentPage('main');

    Animated.timing(slideAnim, {
      toValue: width,
      duration: SLIDE_DURATION,
      useNativeDriver: true,
      easing: Easing.bezier(0.25, 0.1, 0.25, 1),
    }).start(({ finished }) => {
      if (finished) {
        setOverlayPage(null);
      }
      isAnimating.current = false;
    });
  }, [currentPage, width, slideAnim]);

  // Android Up-stop system back key, trigger on secondary page goBack And stops the exit application.
  useEffect(() => {
    if (Platform.OS !== 'android') return;
    const subscription = BackHandler.addEventListener(
      'hardwareBackPress',
      () => {
        if (currentPage !== 'main') {
          goBack();
          return true;
        }
        return false;
      },
    );
    return () => subscription.remove();
  }, [currentPage, goBack]);

  // Rendering Page Contents
  const renderPage = (page: SettingsPage) => {
    switch (page) {
      case 'main':
        return (
          <SettingsMain
            onAccountPress={() => navigateTo('accountDetail')}
            onAboutPress={() => {
              setAboutInitialPage(undefined);
              navigateTo('aboutActrium');
            }}
            onNotificationPress={() => navigateTo('notificationSettings')}
            onFeedbackPress={handleFeedbackPress}
            onLoginPress={handleLoginPress}
            onLogout={handleLogout}
            appVersionText={appVersionText}
            isTablet={isTablet}
          />
        );
      case 'accountDetail':
        return (
          <AccountDetail
            onBack={goBack}
            onDeleteAccount={handleDeleteAccount}
            onLogout={handleLogout}
            isTablet={isTablet}
          />
        );
      case 'aboutActrium':
        return (
          <AboutActrium
            onBack={goBack}
            isTablet={isTablet}
            initialPage={aboutInitialPage}
          />
        );
      case 'notificationSettings':
        return (
          <NotificationSettingsPage
            onBack={goBack}
            isTablet={isTablet}
          />
        );
      default:
        return null;
    }
  };

  return (
    <View style={styles.container}>
      {/* Bottom Page */}
      {renderPage(currentPage)}

      {/* Animated overlay: New page slides from right / Slides Right on Current Page */}
      {overlayPage && (
        <Animated.View
          style={[
            StyleSheet.absoluteFillObject,
            {
              transform: [{ translateX: slideAnim }],
              backgroundColor: '#000',
              zIndex: 20,
            },
          ]}
        >
          {renderPage(overlayPage)}
        </Animated.View>
      )}

      {loginModalVisible && (
        <LoginModal
          visible
          onClose={() => setLoginModalVisible(false)}
          clientId={SETTINGS_CLIENT_ID}
          mode="login_only"
        />
      )}
    </View>
  );
}

const styles = StyleSheet.create({
  container: {
    flex: 1,
    backgroundColor: '#000000',
  },
});

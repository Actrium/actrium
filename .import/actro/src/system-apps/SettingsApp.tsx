/**
 * Settings System App - Built-in settings page
 */
import React, { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { Animated, BackHandler, Easing, Platform, View, StyleSheet, useWindowDimensions } from 'react-native';
import { useTranslation } from 'react-i18next';
import DeviceInfo from 'react-native-device-info';
import { SettingsMain } from './settings/SettingsMain';
import { AccountDetail } from './settings/AccountDetail';
import { AboutAskAway } from './settings/AboutAskAway';
import { NotificationSettingsPage } from './settings/NotificationSettings';
import { LoginModal } from './auth/LoginModal';
import { useUserStore } from '../actr-host/store/useUserStore';
import { useAuthTaskStore } from '../actr-host/store/useAuthTaskStore';
import { createFeedbackAction, createLogoutAction, createDeleteAccountAction } from './settings/actions';
import type { AboutPageKey } from './settings/AboutAskAway';
import { AuthRepository } from '../repository/AuthRepository';
import { authApi, CURRENT_ENV } from '../api/auth';

const SETTINGS_CLIENT_ID = '02094310-3454-4929-80b5-8ee85d09f34b';
export type SettingsPage = 'main' | 'accountDetail' | 'aboutAskAway' | 'notificationSettings';

// 页面切换动画时长
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
      page === 'main' || page === 'accountDetail' || page === 'aboutAskAway' || page === 'notificationSettings'
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

  // 页面切换动画
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

    // 构建号为空时只显示版本号，避免设置页出现无意义的空括号
    return buildNumber ? `${version}(${buildNumber})` : version;
  }, []);

  useEffect(() => {
    let cancelled = false;

    const ensureUserInfo = async () => {
      // 中文注释：设置页只在首次挂载时尝试一次恢复展示态，
      // 后续 userInfo/sessionToken 的变化不再反向触发恢复，避免清理登录态时把旧用户补回来。
      const currentUserInfo = useUserStore.getState().userInfo;
      if (currentUserInfo) {
        return;
      }

      const token = await AuthRepository.getToken(CURRENT_ENV);
      if (!token) {
        return;
      }

      // 设置页展示登录信息依赖 userInfo，只有 token 时要在挂载后主动补拉一次
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
    // 中文注释：动画进行中跳过深链跳转，避免状态冲突。
    if (!deepLink.page || isAnimating.current) {
      return;
    }

    const deepLinkToken =
      deepLink.token === undefined || deepLink.token === null
        ? ''
        : String(deepLink.token);

    // 页面深链同 token 只应用一次，避免重复 props 更新把用户拉回二级页
    if (deepLinkToken && !consumedPageTokensRef.current.has(deepLinkToken)) {
      consumedPageTokensRef.current.add(deepLinkToken);
      setCurrentPage(deepLink.page);
      setAboutInitialPage(deepLink.aboutPage);
    }

    // action 深链做去重，避免同一 token 在重载时重复执行
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

  // 从右侧滑入新页面
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

  // 当前页面向右侧滑出
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

  // 中文注释：Android 上拦截系统返回键，二级页面时触发 goBack 并阻止退出应用。
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

  // 渲染页面内容
  const renderPage = (page: SettingsPage) => {
    switch (page) {
      case 'main':
        return (
          <SettingsMain
            onAccountPress={() => navigateTo('accountDetail')}
            onAboutPress={() => {
              setAboutInitialPage(undefined);
              navigateTo('aboutAskAway');
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
      case 'aboutAskAway':
        return (
          <AboutAskAway
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
      {/* 底层页面 */}
      {renderPage(currentPage)}

      {/* 动画覆盖层：新页面从右侧滑入 / 当前页面向右侧滑出 */}
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

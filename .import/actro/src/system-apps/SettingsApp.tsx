/**
 * Settings System App - Built-in settings page
 */
import React, { useEffect, useMemo, useRef, useState } from 'react';
import { View, StyleSheet, useWindowDimensions } from 'react-native';
import { useTranslation } from 'react-i18next';
import DeviceInfo from 'react-native-device-info';
import { SettingsMain } from './settings/SettingsMain';
import { AccountDetail } from './settings/AccountDetail';
import { AboutAskAway } from './settings/AboutAskAway';
import { LoginModal } from './auth/LoginModal';
import { useUserStore } from '../actr-host/store/useUserStore';
import { useAuthTaskStore } from '../actr-host/store/useAuthTaskStore';
import { createFeedbackAction, createLogoutAction, createDeleteAccountAction } from './settings/actions';
import type { AboutPageKey } from './settings/AboutAskAway';

const SETTINGS_CLIENT_ID = '02094310-3454-4929-80b5-8ee85d09f34b';
export type SettingsPage = 'main' | 'accountDetail' | 'aboutAskAway';

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
      page === 'main' || page === 'accountDetail' || page === 'aboutAskAway'
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
  const clearUser = useUserStore((state) => state.clearUser);
  const sessionToken = useUserStore((state) => state.sessionToken);
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
    if (!deepLink.page) {
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

  return (
    <View style={styles.container}>
      {currentPage === 'main' && (
        <SettingsMain
          onAccountPress={() => setCurrentPage('accountDetail')}
          onAboutPress={() => {
            setAboutInitialPage(undefined);
            setCurrentPage('aboutAskAway');
          }}
          onFeedbackPress={handleFeedbackPress}
          onLoginPress={handleLoginPress}
          onLogout={handleLogout}
          appVersionText={appVersionText}
          isTablet={isTablet}
        />
      )}
      {currentPage === 'accountDetail' && (
        <AccountDetail
          onBack={() => setCurrentPage('main')}
          onDeleteAccount={handleDeleteAccount}
          onLogout={handleLogout}
          isTablet={isTablet}
        />
      )}
      {currentPage === 'aboutAskAway' && (
        <AboutAskAway
          onBack={() => setCurrentPage('main')}
          isTablet={isTablet}
          initialPage={aboutInitialPage}
        />
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

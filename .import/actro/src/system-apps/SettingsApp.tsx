/**
 * Settings System App - Built-in settings page
 */
import React, { useState, useCallback } from 'react';
import { View, StyleSheet, useWindowDimensions, Alert } from 'react-native';
import { useTranslation } from 'react-i18next';
import { SettingsMain } from './settings/SettingsMain';
import { AccountDetail } from './settings/AccountDetail';
import { AboutAskAway } from './settings/AboutAskAway';
import { LoginModal } from './auth/LoginModal';
import { useUserStore } from '../actr-host/store/useUserStore';
import { AuthRepository } from '../repository/AuthRepository';
import { CURRENT_ENV } from '../api/auth';

const SETTINGS_CLIENT_ID = '02094310-3454-4929-80b5-8ee85d09f34b';
type SettingsPage = 'main' | 'accountDetail' | 'aboutAskAway';

export default function SettingsApp() {
  const { t } = useTranslation();
  const { width } = useWindowDimensions();
  const isTablet = width >= 768;
  const [currentPage, setCurrentPage] = useState<SettingsPage>('main');
  const [loginModalVisible, setLoginModalVisible] = useState(false);
  const clearUser = useUserStore((state) => state.clearUser);

  const handleLoginPress = () => {
    setLoginModalVisible(true);
  };

  const doLogout = useCallback(() => {
    clearUser();
    AuthRepository.clearToken(CURRENT_ENV);
    setCurrentPage('main');
  }, [clearUser]);

  const handleLogout = useCallback(() => {
    Alert.alert(
      t('actr.settings.logout'),
      '',
      [
        { text: t('common.cancel'), style: 'cancel' },
        { text: t('common.confirm'), style: 'destructive', onPress: doLogout },
      ],
    );
  }, [t, doLogout]);

  const handleDeleteAccount = () => {
    // TODO: Implement delete account logic
    console.log('Delete account pressed');
  };

  return (
    <View style={styles.container}>
      {currentPage === 'main' && (
        <SettingsMain
          onAccountPress={() => setCurrentPage('accountDetail')}
          onAboutPress={() => setCurrentPage('aboutAskAway')}
          onLoginPress={handleLoginPress}
          onLogout={handleLogout}
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
        />
      )}

      <LoginModal
        visible={loginModalVisible}
        onClose={() => setLoginModalVisible(false)}
        clientId={SETTINGS_CLIENT_ID}
        mode="login_only"
      />
    </View>
  );
}

const styles = StyleSheet.create({
  container: {
    flex: 1,
    backgroundColor: '#000000',
  },
});

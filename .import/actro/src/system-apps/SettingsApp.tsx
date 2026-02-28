/**
 * Settings System App - Built-in settings page
 */
import React, { useState } from 'react';
import { View, StyleSheet, useWindowDimensions } from 'react-native';
import { SettingsMain } from './settings/SettingsMain';
import { AccountDetail } from './settings/AccountDetail';
import { LoginModal } from './auth/LoginModal';

const SETTINGS_CLIENT_ID = '02094310-3454-4929-80b5-8ee85d09f34b';

export default function SettingsApp() {
  const { width } = useWindowDimensions();
  const isTablet = width >= 768;
  const [showAccountDetail, setShowAccountDetail] = useState(false);
  const [loginModalVisible, setLoginModalVisible] = useState(false);

  const handleLoginPress = () => {
    setLoginModalVisible(true);
  };

  const handleLogout = () => {
    // TODO: Implement logout logic
    console.log('Logout pressed');
  };

  const handleDeleteAccount = () => {
    // TODO: Implement delete account logic
    console.log('Delete account pressed');
  };

  return (
    <View style={styles.container}>
      {!showAccountDetail ? (
        <SettingsMain
          onAccountPress={() => setShowAccountDetail(true)}
          onLoginPress={handleLoginPress}
          onLogout={handleLogout}
          isTablet={isTablet}
        />
      ) : (
        <AccountDetail
          onBack={() => setShowAccountDetail(false)}
          onDeleteAccount={handleDeleteAccount}
          onLogout={handleLogout}
          isTablet={isTablet}
        />
      )}

      <LoginModal
        visible={loginModalVisible}
        onClose={() => setLoginModalVisible(false)}
        clientId={SETTINGS_CLIENT_ID}
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

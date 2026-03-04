import React, { useCallback, useState } from 'react';
import {
  View,
  Text,
  Image,
  StyleSheet,
  TouchableOpacity,
  Alert,
} from 'react-native';
import { useTranslation } from 'react-i18next';
import { LoginBridge, LoginType } from '../../native-bridge';

type ThemeVariant = 'light' | 'dark';

interface LoginContentProps {
  variant?: ThemeVariant;
  logoSize?: number;
  buttonMaxWidth?: number;
  fontSize?: {
    tagline?: number;
    button?: number;
  };
  spacing?: {
    logoTop?: number;
    spacer?: number;
    buttonBottom?: number;
    policyBottom?: number;
  };
}

const themeColors = {
  light: {
    tagline: '#000000',
    buttonBg: '#FFFFFF',
    buttonBorder: '#E5E5E5',
    buttonText: '#333',
    iconTint: '#000',
    policyText: '#999',
    linkText: '#1C69FF',
  },
  dark: {
    tagline: '#fff',
    buttonBg: 'rgba(255, 255, 255, 0.15)',
    buttonBorder: 'rgba(255, 255, 255, 0.1)',
    buttonText: '#fff',
    iconTint: '#fff',
    policyText: 'rgba(255, 255, 255, 0.8)',
    linkText: '#1C69FF',
  },
};

export const LoginContent = ({
  variant = 'light',
  logoSize = 120,
  buttonMaxWidth = 400,
  fontSize = {},
  spacing = {},
}: LoginContentProps) => {
  const { t } = useTranslation();
  const [agreePolicy, setAgreePolicy] = useState(false);
  const colors = themeColors[variant];

  const {
    tagline: taglineFontSize = 15,
    button: buttonFontSize = 16,
  } = fontSize;

  const {
    logoTop = 20,
    spacer = 40,
    buttonBottom = 28,
    policyBottom = 32,
  } = spacing;

  const onLoginPress = useCallback(
    (type: LoginType) => {
      if (!agreePolicy) {
        Alert.alert(
          t('actr.auth.agreePolicyTitle'),
          t('actr.auth.agreePolicyDetail'),
          [
            { text: t('common.cancel'), style: 'cancel' },
            {
              text: t('actr.auth.agreeAndLogin'),
              style: 'destructive',
              onPress: () => {
                setAgreePolicy(true);
                LoginBridge.loginWithType(type);
              },
            },
          ],
        );
        return;
      }
      LoginBridge.loginWithType(type);
    },
    [agreePolicy, t],
  );

  return (
    <View style={styles.container}>
      {logoSize > 0 && (
        <View style={[styles.logoContainer, { marginTop: logoTop }]}>
          <Image
            source={require('../../assets/login/login_logo.png')}
            style={[styles.logo, { width: logoSize, height: logoSize }]}
            resizeMode="contain"
          />
        </View>
      )}

      <View style={{ height: spacer }} />

      <Text style={[styles.tagline, { fontSize: taglineFontSize, color: colors.tagline }]}>
        {t('actr.auth.loginTip')}
      </Text>

      <View style={[styles.buttonContainer, { maxWidth: buttonMaxWidth, marginBottom: buttonBottom }]}>
        <TouchableOpacity
          style={[
            styles.loginButton,
            { backgroundColor: colors.buttonBg, borderColor: colors.buttonBorder },
          ]}
          onPress={() => onLoginPress('apple')}
          activeOpacity={0.7}
        >
          <Image
            source={require('../../assets/login/apple_logo.png')}
            style={[styles.btnIcon, { tintColor: colors.iconTint }]}
          />
          <Text style={[styles.btnText, { fontSize: buttonFontSize, color: colors.buttonText }]}>
            {t('actr.auth.appleLogin')}
          </Text>
        </TouchableOpacity>

        <TouchableOpacity
          style={[
            styles.loginButton,
            { backgroundColor: colors.buttonBg, borderColor: colors.buttonBorder },
          ]}
          onPress={() => onLoginPress('wechat')}
          activeOpacity={0.7}
        >
          <Image
            source={require('../../assets/login/wechat_logo.png')}
            style={styles.btnIcon}
          />
          <Text style={[styles.btnText, { fontSize: buttonFontSize, color: colors.buttonText }]}>
            {t('actr.auth.wechatLogin')}
          </Text>
        </TouchableOpacity>
      </View>

      <View style={[styles.policyContainer, { marginBottom: policyBottom }]}>
        <TouchableOpacity
          onPress={() => setAgreePolicy(!agreePolicy)}
          style={styles.checkboxContainer}
          hitSlop={{ top: 10, bottom: 10, left: 10, right: 10 }}
        >
          <View style={[styles.checkbox, agreePolicy && styles.checkboxSelected]}>
            {agreePolicy && <Text style={styles.checkMark}>✓</Text>}
          </View>
        </TouchableOpacity>
        <Text style={[styles.policyText, { color: colors.policyText }]}>
          {t('actr.auth.readAndAgree')}{' '}
          <Text style={[styles.linkText, { color: colors.linkText }]}>{t('actr.auth.userAgreement')}</Text>{' '}
          {t('common.and')}{' '}
          <Text style={[styles.linkText, { color: colors.linkText }]}>{t('actr.auth.privacyPolicy')}</Text>
        </Text>
      </View>
    </View>
  );
};

const styles = StyleSheet.create({
  container: {
    width: '100%',
    alignItems: 'center',
  },
  logoContainer: {
    alignItems: 'center',
  },
  logo: {
    maxWidth: 155,
    maxHeight: 155,
  },
  tagline: {
    marginBottom: 25,
    textAlign: 'center',
    fontWeight: '400',
  },
  buttonContainer: {
    width: '100%',
    paddingHorizontal: 8,
  },
  loginButton: {
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'center',
    height: 48,
    borderRadius: 24,
    marginBottom: 10,
    borderWidth: 0,
  },
  btnIcon: {
    width: 16,
    height: 16,
    marginRight: 4,
  },
  btnText: {
    fontWeight: '500',
  },
  policyContainer: {
    flexDirection: 'row',
    alignItems: 'center',
  },
  checkboxContainer: {
    padding: 4,
  },
  checkbox: {
    width: 14,
    height: 14,
    borderRadius: 7,
    borderWidth: 1,
    borderColor: '#969696',
    alignItems: 'center',
    justifyContent: 'center',
  },
  checkboxSelected: {
    backgroundColor: '#597EF7',
    borderColor: '#597EF7',
  },
  checkMark: {
    fontSize: 10,
    color: '#fff',
    fontWeight: 'bold',
  },
  policyText: {
    fontSize: 12,
    marginLeft: 6,
  },
  linkText: {},
});

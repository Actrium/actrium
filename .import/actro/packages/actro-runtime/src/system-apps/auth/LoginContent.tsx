import React, { useCallback, useMemo, useState } from 'react';
import {
  View,
  Text,
  Image,
  StyleSheet,
  TouchableOpacity,
  Platform,
} from 'react-native';
import { useTranslation } from 'react-i18next';
import { LoginBridge, LoginType } from '../../native-bridge';
import { GlobalAlertModal } from '../../components';
import { useColors } from '../../theme/useColors';

type ThemeVariant = 'light' | 'dark';

interface LoginContentProps {
  variant?: ThemeVariant;
  logoSize?: number;
  buttonMaxWidth?: number;
  agreePolicy?: boolean;
  onAgreePolicyChange?: (value: boolean) => void;
  onOpenUserAgreement?: () => void;
  onOpenPrivacyPolicy?: () => void;
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


export const LoginContent = ({
  variant = 'light',
  logoSize = 120,
  buttonMaxWidth = 400,
  agreePolicy,
  onAgreePolicyChange,
  onOpenUserAgreement,
  onOpenPrivacyPolicy,
  fontSize = {},
  spacing = {},
}: LoginContentProps) => {
  const { t } = useTranslation();
  const sysColors = useColors();
  const colors = {
    tagline: sysColors.authTagline,
    buttonBg: sysColors.authButtonBg,
    buttonBorder: sysColors.authButtonBorder,
    buttonText: sysColors.authButtonText,
    iconTint: sysColors.authIconTint,
    policyText: sysColors.authPolicyText,
    linkText: sysColors.authLinkText,
  };

  const [innerAgreePolicy, setInnerAgreePolicy] = useState(false);
  const [pendingLoginType, setPendingLoginType] = useState<LoginType | null>(null);
  const resolvedAgreePolicy = useMemo(
    () => agreePolicy ?? innerAgreePolicy,
    [agreePolicy, innerAgreePolicy],
  );

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

  const updateAgreePolicy = useCallback(
    (value: boolean) => {
      if (agreePolicy === undefined) {
        setInnerAgreePolicy(value);
      }
      onAgreePolicyChange?.(value);
    },
    [agreePolicy, onAgreePolicyChange],
  );

  const onLoginPress = useCallback(
    (type: LoginType) => {
      if (!resolvedAgreePolicy) {
        setPendingLoginType(type);
        return;
      }
      LoginBridge.loginWithType(type);
    },
    [resolvedAgreePolicy],
  );

  const handlePolicyAlertClose = useCallback(() => {
    setPendingLoginType(null);
  }, []);

  const handleAgreeAndLogin = useCallback(() => {
    if (!pendingLoginType) return;
    updateAgreePolicy(true);
    setPendingLoginType(null);
    LoginBridge.loginWithType(pendingLoginType);
  }, [pendingLoginType, updateAgreePolicy]);

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
        {Platform.OS !== 'android' && (
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
        )}

        {/* {Platform.OS === 'android' && (
          <TouchableOpacity
            style={[
              styles.loginButton,
              { backgroundColor: colors.buttonBg, borderColor: colors.buttonBorder },
            ]}
            onPress={() => onLoginPress('google')}
            activeOpacity={0.7}
          >
            <Image
              source={require('../../assets/login/google_logo.png')}
              style={styles.btnIcon}
            />
            <Text style={[styles.btnText, { fontSize: buttonFontSize, color: colors.buttonText }]}>
              {t('actr.auth.googleLogin')}
            </Text>
          </TouchableOpacity>
        )} */}

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
          onPress={() => updateAgreePolicy(!resolvedAgreePolicy)}
          style={styles.checkboxContainer}
          hitSlop={{ top: 10, bottom: 10, left: 10, right: 10 }}
        >
          <View style={[styles.checkbox, resolvedAgreePolicy && styles.checkboxSelected]}>
            {resolvedAgreePolicy && <Text style={styles.checkMark}>✓</Text>}
          </View>
        </TouchableOpacity>
        <Text style={[styles.policyText, { color: colors.policyText }]}>
          {t('actr.auth.readAndAgree')}{' '}
          <Text
            style={[styles.linkText, { color: colors.linkText }]}
            onPress={onOpenUserAgreement}
          >
            {t('actr.auth.userAgreement')}
          </Text>{' '}
          {t('common.and')}{' '}
          <Text
            style={[styles.linkText, { color: colors.linkText }]}
            onPress={onOpenPrivacyPolicy}
          >
            {t('actr.auth.privacyPolicy')}
          </Text>
        </Text>
      </View>
      <GlobalAlertModal
        visible={pendingLoginType !== null}
        title={t('actr.auth.agreePolicyTitle')}
        message={t('actr.auth.agreePolicyDetail')}
        buttons={[
          { text: t('common.cancel'), style: 'cancel' },
          {
            text: t('actr.auth.agreeAndLogin'),
            variant: 'primary',
            onPress: handleAgreeAndLogin,
          },
        ]}
        options={{ cancelable: true }}
        onClose={handlePolicyAlertClose}
      />
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
    flexShrink: 1,
  },
  linkText: {
    textDecorationLine: 'underline',
  },
});

import React, { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import {
  View,
  StyleSheet,
  Modal,
  TouchableWithoutFeedback,
  ActivityIndicator,
  Image,
  ImageBackground,
  useWindowDimensions,
} from 'react-native';
import { useTranslation } from 'react-i18next';
import { LoginContent } from './LoginContent';
import { useLoginFlow, LoginFlowMode } from '../../hooks/auth';
import { HostIconButton } from '../../components';
import { getHostLayoutTokens } from '../../utils/layout';
import type { AboutPageKey } from '../settings/aboutPages';
import { AboutWebViewModal } from '../settings/AboutWebViewModal';
import { AppText } from '../../components';
import { getTypographyStyle } from '../../theme/tokens';
import { useColors } from '../../theme/useColors';
import type { ColorTokens } from '../../theme/colors';

interface LoginModalProps {
  visible: boolean;
  onClose: () => void;
  clientId: string;
  appId?: string;
  onSuccess?: () => void;
  /** 登录模式：login_only 仅登录，login_and_authorize 登录后自动授权 */
  mode?: LoginFlowMode;
}

export const LoginModal = ({
  visible,
  onClose,
  clientId,
  appId,
  onSuccess,
  mode = 'login_and_authorize',
}: LoginModalProps) => {
  const { t } = useTranslation();
  const { width } = useWindowDimensions();
  const colors = useColors();
  const styles = useMemo(() => makeStyles(colors), [colors]);
  const layoutTokens = useMemo(() => getHostLayoutTokens(width), [width]);
  const { step } = useLoginFlow({
    mode,
    clientId,
    appId,
    autoInit: visible,
  });
  const [agreePolicy, setAgreePolicy] = useState(false);
  const [activePolicyPage, setActivePolicyPage] = useState<AboutPageKey | null>(null);

  // 记录上一次的 step，用于检测状态变化
  const prevStepRef = useRef<typeof step | null>(null);

  // 登录/授权成功后自动关闭弹窗（只在弹窗可见且状态变为 success 时触发）
  useEffect(() => {
    if (visible && step === 'success' && prevStepRef.current !== 'success') {
      const timer = setTimeout(() => {
        onSuccess?.();
        onClose();
      }, 500);
      prevStepRef.current = step;
      return () => clearTimeout(timer);
    }
    prevStepRef.current = step;
  }, [visible, step, onSuccess, onClose]);

  // 弹窗关闭时重置状态，确保下次打开时能正确检测变化
  useEffect(() => {
    if (!visible) {
      prevStepRef.current = null;
      setActivePolicyPage(null);
    }
  }, [visible]);

  const handlePolicyPageOpen = useCallback((page: AboutPageKey) => {
    setActivePolicyPage(page);
  }, []);

  const handlePolicyPageClose = useCallback(() => {
    setActivePolicyPage(null);
  }, []);

  const renderContent = () => {
    if (step === 'checking' || step === 'logging_in') {
      return (
        <View style={styles.loadingContainer}>
          <ActivityIndicator size="large" color={colors.authLoadingIndicator} />
          <AppText style={styles.loadingText}>{t('actr.auth.loggingIn')}</AppText>
        </View>
      );
    }

    if (step === 'authorizing') {
      return (
        <View style={styles.loadingContainer}>
          <ActivityIndicator size="large" color={colors.authLoadingIndicator} />
          <AppText style={styles.loadingText}>{t('actr.auth.authorizing')}</AppText>
        </View>
      );
    }

    if (step === 'success') {
      return (
        <View style={styles.loadingContainer}>
          <AppText style={styles.successText}>✓</AppText>
        </View>
      );
    }

    return (
      <LoginContent
        variant="light"
        logoSize={0}
        agreePolicy={agreePolicy}
        onAgreePolicyChange={setAgreePolicy}
        onOpenUserAgreement={() => handlePolicyPageOpen('userAgreement')}
        onOpenPrivacyPolicy={() => handlePolicyPageOpen('privacyPolicy')}
        fontSize={{ tagline: 17, button: 17 }}
        spacing={{ logoTop: 0, spacer: 16, buttonBottom: 0, policyBottom: 0 }}
      />
    );
  };

  const showLogo = step === 'login_options' || step === 'idle';

  return (
    <Modal
      visible={visible}
      transparent
      animationType="fade"
      onRequestClose={onClose}
    >
      <TouchableWithoutFeedback onPress={onClose}>
        <View style={styles.overlay}>
          <TouchableWithoutFeedback>
            <View
              style={[
                styles.modalContainer,
                {
                  // phone 端保持 95% 宽度，只有 pad 端才限制最大宽度为 400
                  maxWidth: layoutTokens.isTablet ? 400 : undefined,
                },
              ]}
            >
              <ImageBackground
                source={require('../../assets/login/login_bg.png')}
                style={styles.modalBg}
                imageStyle={styles.modalBgImage}
                resizeMode="cover"
              />
              <View style={[styles.modalBg, styles.darkBg]} />
              <View style={styles.modalContentWrapper}>
                <View style={styles.header}>
                  <AppText style={styles.title}>{t('actr.auth.login') + ' AskAway'}</AppText>
                  <HostIconButton
                    iconType='close'
                    size={44}
                    iconSize={20}
                    iconColor={colors.authModalLoginCloseIcon}
                    backgroundColor={colors.authModalLoginCloseBg}
                    wrapperStyle={styles.closeButton}
                    onPress={onClose}
                    hitSlop={{ top: 10, bottom: 10, left: 10, right: 10 }}
                  />
                </View>

                {showLogo && (
                  <View style={styles.logoRow}>
                    <Image
                      source={require('../../assets/login/login_logo.png')}
                      style={{ width: 100, height: 100 }}
                      resizeMode="contain"
                    />
                  </View>
                )}

                <View style={styles.content}>
                  {renderContent()}
                </View>
              </View>
            </View>
          </TouchableWithoutFeedback>
        </View>
      </TouchableWithoutFeedback>
      <AboutWebViewModal
        visible={!!activePolicyPage}
        page={activePolicyPage}
        onClose={handlePolicyPageClose}
      />
    </Modal>
  );
};

const makeStyles = (colors: ColorTokens) => StyleSheet.create({
  overlay: {
    flex: 1,
    backgroundColor: 'rgba(0, 0, 0, 0.5)',
    justifyContent: 'center',
    alignItems: 'center',
  },
  modalContainer: {
    width: '95%',
    backgroundColor: colors.authModalLoginBg,
    borderRadius: 33,
    overflow: 'hidden',
    position: 'relative',
  },
  modalBg: {
    ...StyleSheet.absoluteFillObject,
  },
  modalBgImage: {
    borderRadius: 33,
  },
  darkBg: {
    backgroundColor: colors.authDarkBg,
  },
  modalContentWrapper: {
    paddingTop: 16,
    paddingBottom: 59,
    paddingHorizontal: 16,
  },
  header: {
    width: '100%',
    height: 44,
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'center',
    marginBottom: 28,
    position: 'relative',
  },
  title: {
    ...getTypographyStyle('title3'),
    color: colors.authModalLoginTitle,
  },
  logoRow: {
    width: '100%',
    alignItems: 'center',
    justifyContent: 'center',
  },
  closeButton: {
    position: 'absolute',
    right: 0,
    width: 44,
    height: 44,
    borderRadius: 22,
    backgroundColor: colors.authModalLoginCloseBg,
    alignItems: 'center',
    justifyContent: 'center',
  },
  content: {
    width: '100%',
  },
  loadingContainer: {
    padding: 40,
    alignItems: 'center',
    justifyContent: 'center',
  },
  loadingText: {
    marginTop: 12,
    ...getTypographyStyle('body'),
    color: colors.authLoadingText,
  },
  successText: {
    fontSize: 40,
    lineHeight: 40,
    color: colors.authModalLoginSuccessText,
  },
});

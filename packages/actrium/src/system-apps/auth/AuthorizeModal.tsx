import React, { useEffect, useMemo, useRef } from 'react';
import {
  View,
  Image,
  StyleSheet,
  Modal,
  TouchableWithoutFeedback,
  useWindowDimensions,
} from 'react-native';
import { useTranslation } from 'react-i18next';
import { AuthorizeContent } from './AuthorizeContent';
import { useAppStore } from '../../actr-host/store/useAppStore';
import { useAuthorize } from '../../hooks/auth';
import { HostIconButton } from '../../components';
import { getHostLayoutTokens } from '../../utils/layout';
import { AppText } from '../../components';
import { getTypographyStyle } from '../../theme/tokens';
import { useColors } from '../../theme/useColors';
import type { ColorTokens } from '../../theme/colors';

interface AuthorizeModalProps {
  visible: boolean;
  onClose: () => void;
  appId: string;
  onSuccess?: () => void;
}

export const AuthorizeModal = ({
  visible,
  onClose,
  appId,
  onSuccess,
}: AuthorizeModalProps) => {
  const { t } = useTranslation();
  const { width } = useWindowDimensions();
  const colors = useColors();
  const styles = useMemo(() => makeStyles(colors), [colors]);
  const layoutTokens = useMemo(() => getHostLayoutTokens(width), [width]);
  const appInfo = useAppStore((state) => state.apps[appId]);

  const { step, loading, authorize } = useAuthorize({
    clientId: appInfo?.client_id || '',
    appId,
    isModal: true,
  });

  // Recording last time. step, for detecting state changes
  const prevStepRef = useRef<typeof step | null>(null);

  // The bullet window is closed after the authorization is successful (only visible and state is changed to authorized (a) The time trigger)
  useEffect(() => {
    if (visible && step === 'authorized' && prevStepRef.current !== 'authorized') {
      onSuccess?.();
      onClose();
    }
    prevStepRef.current = step;
  }, [visible, step, onSuccess, onClose]);

  // Reset when the bullet window closes to ensure that changes are correctly detected when next open
  useEffect(() => {
    if (!visible) {
      prevStepRef.current = null;
    }
  }, [visible]);

  const handleAuthorize = () => {
    authorize();
  };

  return (
    <Modal
      visible={visible}
      transparent
      animationType='fade'
      onRequestClose={onClose}
    >
      <TouchableWithoutFeedback onPress={onClose}>
        <View style={styles.overlay}>
          <TouchableWithoutFeedback>
            <View
              style={[
                styles.modalContainer,
                {
                  // phone End hold. 95% width, only pad End limit maximum width to 400
                  maxWidth: layoutTokens.isTablet ? 400 : undefined,
                },
              ]}
            >
              <View style={styles.modalContentWrapper}>
                {/* HeaderLeft app logo + Title, Right Close button */}
                <View style={styles.header}>
                  <View style={styles.headerLeft}>
                    {appInfo?.info?.logo ? (
                      <Image
                        source={{ uri: appInfo.info.logo }}
                        style={styles.headerLogo}
                        resizeMode='cover'
                      />
                    ) : (
                      <View
                        style={[
                          styles.headerLogo,
                          styles.headerLogoPlaceholder,
                        ]}
                      />
                    )}
                    <AppText style={styles.headerTitle} numberOfLines={1}>
                      {appInfo?.instance_name || t('actr.app.officialService')}
                    </AppText>
                  </View>
                  <HostIconButton
                    iconType='close'
                    size={44}
                    iconSize={20}
                    iconColor={colors.authModalAuthorizeCloseIcon}
                    backgroundColor={colors.authModalAuthorizeCloseBg}
                    wrapperStyle={styles.closeButton}
                    onPress={onClose}
                    hitSlop={{ top: 10, bottom: 10, left: 10, right: 10 }}
                  />
                </View>

                {/* Mandate content (do not show) appInfoRow, because header ♪ I've shown ♪ */}
                <View style={styles.content}>
                  <AuthorizeContent
                    variant='light'
                    appLogo={appInfo?.info?.logo}
                    appName={appInfo?.instance_name}
                    loading={loading}
                    onAuthorize={handleAuthorize}
                    onReject={onClose}
                    padding={0}
                    showAppInfo={false}
                  />
                </View>
              </View>
            </View>
          </TouchableWithoutFeedback>
        </View>
      </TouchableWithoutFeedback>
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
    backgroundColor: colors.authModalAuthorizeBg,
    borderRadius: 33,
    overflow: 'hidden',
  },
  modalContentWrapper: {
    paddingTop: 16,
    paddingBottom: 16,
    paddingHorizontal: 16,
  },
  header: {
    width: '100%',
    height: 44,
    flexDirection: 'row',
    alignItems: 'center',
    marginBottom: 24,
    position: 'relative',
  },
  headerLeft: {
    flex: 1,
    flexDirection: 'row',
    alignItems: 'center',
    marginRight: 54,
  },
  headerLogo: {
    width: 44,
    height: 44,
    borderRadius: 22,
    marginRight: 10,
    overflow: 'hidden',
    borderWidth: 1,
    borderColor: '#fff',
  },
  headerLogoPlaceholder: {
    backgroundColor: colors.authModalAuthorizePlaceholder,
  },
  headerTitle: {
    ...getTypographyStyle('title3'),
    color: colors.authModalAuthorizeTitle,
    flexShrink: 1,
  },
  closeButton: {
    position: 'absolute',
    right: 0,
    width: 44,
    height: 44,
    borderRadius: 22,
    backgroundColor: colors.authModalAuthorizeCloseBg,
    alignItems: 'center',
    justifyContent: 'center',
  },
  content: {
    width: '100%',
    // The mandate is pad The end will be maxWidth Close, left is defaulted when invisibility is in the center
    alignItems: 'center',
  },
});

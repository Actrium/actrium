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

  // 记录上一次的 step，用于检测状态变化
  const prevStepRef = useRef<typeof step | null>(null);

  // 授权成功后关闭弹窗（只在弹窗可见且状态变为 authorized 时触发）
  useEffect(() => {
    if (visible && step === 'authorized' && prevStepRef.current !== 'authorized') {
      onSuccess?.();
      onClose();
    }
    prevStepRef.current = step;
  }, [visible, step, onSuccess, onClose]);

  // 弹窗关闭时重置状态，确保下次打开时能正确检测变化
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
                  // phone 端保持 95% 宽度，只有 pad 端才限制最大宽度为 400
                  maxWidth: layoutTokens.isTablet ? 400 : undefined,
                },
              ]}
            >
              <View style={styles.modalContentWrapper}>
                {/* Header：左侧 app logo + 标题，右侧关闭按钮 */}
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

                {/* 授权内容（不显示 appInfoRow，因为 header 已经展示） */}
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
    // 授权内容在 pad 端会被 maxWidth 收窄，不显式居中时会默认贴左
    alignItems: 'center',
  },
});

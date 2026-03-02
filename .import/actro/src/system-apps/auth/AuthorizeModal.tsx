import React, { useEffect, useRef } from 'react';
import {
  View,
  Text,
  Image,
  StyleSheet,
  Modal,
  TouchableOpacity,
  TouchableWithoutFeedback,
} from 'react-native';
import { useTranslation } from 'react-i18next';
import { AuthorizeContent } from './AuthorizeContent';
import { useAppStore } from '../../actr-host/store/useAppStore';
import { useAuthorize } from '../../hooks/auth';
import { Icon } from '../../actr-host/components/Icon';

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
            <View style={styles.modalContainer}>
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
                    <Text style={styles.headerTitle} numberOfLines={1}>
                      {appInfo?.app_name || t('actr.app.officialService')}
                    </Text>
                  </View>
                  <TouchableOpacity
                    style={styles.closeButton}
                    onPress={onClose}
                    hitSlop={{ top: 10, bottom: 10, left: 10, right: 10 }}
                  >
                    <Icon type='close' size={20} color='#666' />
                  </TouchableOpacity>
                </View>

                {/* 授权内容（不显示 appInfoRow，因为 header 已经展示） */}
                <View style={styles.content}>
                  <AuthorizeContent
                    variant='light'
                    appLogo={appInfo?.info?.logo}
                    appName={appInfo?.app_name}
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

const styles = StyleSheet.create({
  overlay: {
    flex: 1,
    backgroundColor: 'rgba(0, 0, 0, 0.5)',
    justifyContent: 'center',
    alignItems: 'center',
  },
  modalContainer: {
    width: '95%',
    // maxWidth: 360,
    backgroundColor: '#E8E8E9',
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
    backgroundColor: '#E5E5E5',
  },
  headerTitle: {
    fontSize: 17,
    fontWeight: '600',
    color: '#333333',
    flexShrink: 1,
  },
  closeButton: {
    position: 'absolute',
    right: 0,
    width: 44,
    height: 44,
    borderRadius: 22,
    backgroundColor: '#78788029',
    alignItems: 'center',
    justifyContent: 'center',
  },
  content: {
    width: '100%',
  },
});

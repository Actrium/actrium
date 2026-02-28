import React from 'react';
import {
  View,
  Text,
  StyleSheet,
  Modal,
  TouchableOpacity,
  TouchableWithoutFeedback,
  ActivityIndicator,
  Image,
} from 'react-native';
import { useTranslation } from 'react-i18next';
import { LoginContent } from './LoginContent';
import { AuthorizeContent } from './AuthorizeContent';
import { useLoginFlow, LoginStep } from '../../hooks/useLoginFlow';
import { useAppStore } from '../../actr-host/store/useAppStore';
import { Icon } from '../../actr-host/components/Icon';

interface LoginModalProps {
  visible: boolean;
  onClose: () => void;
  clientId: string;
  appId?: string;
  onSuccess?: () => void;
}

export const LoginModal = ({
  visible,
  onClose,
  clientId,
  appId,
  onSuccess,
}: LoginModalProps) => {
  const { t } = useTranslation();
  const { step, loading, authorize } = useLoginFlow({
    clientId,
    appId,
    isModal: true,
  });
  const appInfo = useAppStore((state) => appId ? state.apps[appId] : null);

  const handleAuthorize = async () => {
    await authorize();
    onSuccess?.();
    onClose();
  };

  const handleReject = () => {
    onClose();
  };

  const renderContent = () => {
    if (step === 'check_status' || (loading && step !== 'authorizing')) {
      return (
        <View style={styles.loadingContainer}>
          <ActivityIndicator size="large" color="#1C69FF" />
          <Text style={styles.loadingText}>{t('actr.auth.authorizing')}</Text>
        </View>
      );
    }

    if (step === 'authorizing') {
      return (
        <AuthorizeContent
          variant="light"
          appLogo={appInfo?.info?.logo}
          appName={appInfo?.app_name}
          loading={loading}
          onAuthorize={handleAuthorize}
          onReject={handleReject}
          padding={0}
        />
      );
    }

    if (step === 'success') {
      return (
        <View style={styles.loadingContainer}>
          <Text style={styles.successText}>✓</Text>
        </View>
      );
    }

    return (
      <LoginContent
        variant="light"
        logoSize={0}
        buttonMaxWidth={280}
        fontSize={{ tagline: 17, button: 17 }}
        spacing={{ logoTop: 0, spacer: 16, buttonBottom: 25, policyBottom: 0 }}
      />
    );
  };

  const getTitle = (currentStep: LoginStep) => {
    if (currentStep === 'authorizing') {
      return appInfo?.app_name || t('actr.app.officialService');
    }
    return t('actr.auth.login') + ' AskAway';
  };

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
            <View style={styles.modalContainer}>
              <View style={styles.modalContentWrapper}>
                {/* Header (Title & Close Button) */}
                <View style={styles.header}>
                  <Text style={styles.title}>{getTitle(step)}</Text>
                  <TouchableOpacity
                    style={styles.closeButton}
                    onPress={onClose}
                    hitSlop={{ top: 10, bottom: 10, left: 10, right: 10 }}
                  >
                    <Icon type="close" size={20} color="#666" />
                  </TouchableOpacity>
                </View>

                {/* Logo Container */}
                <View style={styles.logoRow}>
                  <Image
                    source={require('../../assets/login/login_logo.png')}
                    style={{ width: 100, height: 100 }}
                    resizeMode="contain"
                  />
                </View>

                {/* Content */}
                <View style={styles.content}>
                  {renderContent()}
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
    width: '85%',
    maxWidth: 360,
    backgroundColor: 'rgba(203, 214, 253, 1)', // 更加浅的蓝色
    borderRadius: 33,
    overflow: 'hidden',
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
    fontSize: 17,
    fontWeight: '600',
    color: '#333333',
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
    backgroundColor: '#F0F0F0',
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
    fontSize: 14,
    color: '#666',
  },
  successText: {
    fontSize: 48,
    color: '#52C41A',
  },
});

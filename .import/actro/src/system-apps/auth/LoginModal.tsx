import React from 'react';
import {
  View,
  Text,
  StyleSheet,
  Modal,
  TouchableOpacity,
  TouchableWithoutFeedback,
  ActivityIndicator,
} from 'react-native';
import { useTranslation } from 'react-i18next';
import { LoginContent } from './LoginContent';
import { AuthorizeContent } from './AuthorizeContent';
import { useLoginFlow, LoginStep } from '../../hooks/useLoginFlow';
import { useAppStore } from '../../actr-host/store/useAppStore';

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

    // login_options
    return (
      <LoginContent
        variant="light"
        logoSize={100}
        buttonMaxWidth={280}
        fontSize={{ tagline: 14, button: 15 }}
        spacing={{ logoTop: 10, spacer: 20, buttonBottom: 16, policyBottom: 0 }}
      />
    );
  };

  const getTitle = (currentStep: LoginStep) => {
    if (currentStep === 'authorizing') {
      return appInfo?.app_name || t('actr.app.officialService');
    }
    return `${t('actr.auth.login')} AskAway`;
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
              {/* Header */}
              <View style={styles.header}>
                <Text style={styles.title}>{getTitle(step)}</Text>
                <TouchableOpacity
                  style={styles.closeButton}
                  onPress={onClose}
                  hitSlop={{ top: 10, bottom: 10, left: 10, right: 10 }}
                >
                  <Text style={styles.closeIcon}>×</Text>
                </TouchableOpacity>
              </View>

              {/* Content */}
              <View style={styles.content}>
                {renderContent()}
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
    backgroundColor: '#fff',
    borderRadius: 20,
    overflow: 'hidden',
  },
  header: {
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'center',
    paddingVertical: 16,
    paddingHorizontal: 20,
    borderBottomWidth: StyleSheet.hairlineWidth,
    borderBottomColor: '#E5E5E5',
  },
  title: {
    fontSize: 17,
    fontWeight: '600',
    color: '#333',
  },
  closeButton: {
    position: 'absolute',
    right: 16,
    top: 12,
    width: 28,
    height: 28,
    borderRadius: 14,
    backgroundColor: '#F0F0F0',
    alignItems: 'center',
    justifyContent: 'center',
  },
  closeIcon: {
    fontSize: 20,
    color: '#666',
    marginTop: -2,
  },
  content: {
    paddingHorizontal: 24,
    paddingVertical: 20,
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

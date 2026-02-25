import React, { useState } from 'react';
import {
  View,
  Text,
  StyleSheet,
  Modal,
  TextInput,
  TouchableOpacity,
  Pressable,
  TouchableWithoutFeedback,
  Dimensions,
  ActivityIndicator,
  Animated,
  Easing,
  KeyboardAvoidingView,
  Platform,
  ScrollView,
  Alert,
} from 'react-native';
import { useTranslation } from 'react-i18next';
import { Icon } from './Icon';
import { submitFeedback, FeedbackContext } from '../../api/feedback';
import { useMessageStore } from '../store/useMessageStore';
import { IMessage } from '../../repository/MessageRepository';
import dayjs from 'dayjs';

interface Category {
  id: string;
  zh: string;
  key: string;
}

const CATEGORIES: Category[] = [
  { id: 'harmful', zh: '有害/不安全', key: 'actr.chat.harmful' },
  { id: 'sexual', zh: '色情/低俗', key: 'actr.chat.sexual' },
  { id: 'privacy', zh: '隐私相关', key: 'actr.chat.privacy' },
  { id: 'abusive', zh: '谩骂/攻击', key: 'actr.chat.abusive' },
  { id: 'illegal', zh: '违法/犯罪', key: 'actr.chat.illegal' },
  { id: 'fake', zh: '虚假信息', key: 'actr.chat.fake' },
  { id: 'copyright', zh: '侵犯著作权/名誉权', key: 'actr.chat.copyright' },
  { id: 'other', zh: '其他', key: 'actr.chat.other' },
];

interface FeedbackFormModalProps {
  visible: boolean;
  onClose: () => void;
  appId?: string;
  messageId?: string;
}

export const FeedbackFormModal: React.FC<FeedbackFormModalProps> = ({
  visible,
  onClose,
  appId,
  messageId,
}) => {
  const { t } = useTranslation();
  const [selectedIds, setSelectedIds] = useState<Set<string>>(new Set());
  const [details, setDetails] = useState('');
  const [status, setStatus] = useState<
    'idle' | 'submitting' | 'success' | 'error'
  >('idle');

  const messages =
    useMessageStore((state) => (appId ? state.messages[appId] : [])) || [];

  const animValue = React.useRef(new Animated.Value(0)).current;

  React.useEffect(() => {
    if (visible) {
      animValue.setValue(0);
      Animated.spring(animValue, {
        toValue: 1,
        useNativeDriver: true,
        tension: 40,
        friction: 9,
      }).start();
    }
  }, [visible]);

  const translateY = animValue.interpolate({
    inputRange: [0, 1],
    outputRange: [600, 0],
  });

  const toggleCategory = (id: string) => {
    const next = new Set(selectedIds);
    if (next.has(id)) {
      next.delete(id);
    } else {
      next.add(id);
    }
    setSelectedIds(next);
  };

  const handleSubmit = async () => {
    if (selectedIds.size === 0) {
      Alert.alert(t('common.hint'), t('actr.chat.selectCategoryTip') || '请至少选择一个举报分类');
      return;
    }

    setStatus('submitting');
    try {
      const selectedTypes = CATEGORIES.filter((c) => selectedIds.has(c.id)).map(
        (c) => t(c.key as any),
      );

      const contextPayload: FeedbackContext[] = [];
      const targetMsg = messages.find((m) => m.message_id === messageId);

      if (targetMsg) {
        if (targetMsg.question_content) {
          contextPayload.push({
            role: 'user',
            message_id: targetMsg.message_id,
            content: targetMsg.question_content,
            created_at: dayjs(targetMsg.question_time).format(
              'YYYY-MM-DD HH:mm:ss',
            ),
          });
        }
        if (targetMsg.answer_content) {
          contextPayload.push({
            role: 'assistant',
            message_id: targetMsg.message_id,
            content: targetMsg.answer_content,
            created_at: dayjs(targetMsg.answer_time).format(
              'YYYY-MM-DD HH:mm:ss',
            ),
          });
        }
      }

      const res = await submitFeedback({
        app_id: appId || '',
        user_id: '占位',
        type: selectedTypes,
        note: details,
        context: contextPayload,
        message_id: messageId || '',
      });

      if (res.success) {
        setStatus('success');
        handleClose();
      } else {
        setStatus('error');
      }
    } catch (error) {
      console.error('[Feedback] Submit failed:', error);
      setStatus('error');
    }
  };

  const handleClose = () => {
    setSelectedIds(new Set());
    setDetails('');
    setStatus('idle');
    onClose();
  };

  if (status === 'success') {
    return (
      <Modal visible={visible} transparent animationType='fade'>
        <View style={styles.backdrop}>
          <View style={styles.successContainer}>
            <Icon type='copy' size={40} color='#34C759' />
            <Text style={styles.successText}>{t('actr.chat.feedbackSuccess')}</Text>
          </View>
        </View>
      </Modal>
    );
  }

  return (
    <Modal
      visible={visible}
      transparent
      animationType='fade'
      onRequestClose={handleClose}
    >
      <KeyboardAvoidingView behavior='padding' style={{ flex: 1 }}>
        <TouchableWithoutFeedback onPress={handleClose}>
          <View style={styles.backdrop}>
            <TouchableWithoutFeedback>
              <Animated.View
                style={[
                  styles.container,
                  {
                    transform: [{ translateY }],
                  },
                ]}
              >
                <ScrollView
                  bounces={false}
                  showsVerticalScrollIndicator={false}
                  contentContainerStyle={styles.scrollContent}
                  keyboardShouldPersistTaps='handled'
                >
                  <View style={styles.header}>
                    <Text style={styles.title}>{t('actr.chat.feedbackTitle')}</Text>
                    <TouchableOpacity
                      onPress={handleClose}
                      style={styles.closeButton}
                    >
                      <Icon type='close' size={20} color='#999' />
                    </TouchableOpacity>
                  </View>

                  <View style={styles.section}>
                    <View style={styles.grid}>
                      {CATEGORIES.map((cat) => {
                        const isSelected = selectedIds.has(cat.id);
                        return (
                          <Pressable
                            key={cat.id}
                            onPress={() => toggleCategory(cat.id)}
                            style={[
                              styles.tag,
                              isSelected && styles.tagSelected,
                            ]}
                          >
                            <Text
                              style={[
                                styles.tagText,
                                isSelected && styles.tagTextSelected,
                              ]}
                              numberOfLines={1}
                            >
                              {t(cat.key as any)}
                            </Text>
                          </Pressable>
                        );
                      })}
                    </View>
                  </View>

                  <View style={styles.section}>
                    <View style={styles.feedbackInputWrapper}>
                      <TextInput
                        style={styles.input}
                        placeholder={t('actr.chat.feedbackPlaceholder')}
                        placeholderTextColor='#999'
                        multiline
                        numberOfLines={4}
                        value={details}
                        onChangeText={setDetails}
                        textAlignVertical='top'
                      />
                    </View>
                  </View>

                  {status === 'error' && (
                    <Text style={styles.errorText}>{t('actr.chat.feedbackFailed')}</Text>
                  )}

                  <View style={styles.footer}>
                    <TouchableOpacity
                      style={[styles.button, styles.cancelButton]}
                      onPress={handleClose}
                    >
                      <Text style={styles.cancelButtonText}>{t('common.cancel')}</Text>
                    </TouchableOpacity>
                    <TouchableOpacity
                      style={[
                        styles.button,
                        styles.submitButton,
                        status === 'submitting' && styles.buttonDisabled,
                      ]}
                      onPress={handleSubmit}
                      disabled={status === 'submitting'}
                    >
                      {status === 'submitting' ? (
                        <ActivityIndicator color='#fff' size='small' />
                      ) : (
                        <Text style={styles.submitButtonText}>{t('actr.chat.submit')}</Text>
                      )}
                    </TouchableOpacity>
                  </View>
                </ScrollView>
              </Animated.View>
            </TouchableWithoutFeedback>
          </View>
        </TouchableWithoutFeedback>
      </KeyboardAvoidingView>
    </Modal>
  );
};

const styles = StyleSheet.create({
  backdrop: {
    flex: 1,
    backgroundColor: 'rgba(0,0,0,0.4)',
    justifyContent: 'center',
    alignItems: 'center',
    paddingVertical: 40,
  },
  container: {
    width: '90%',
    maxWidth: 500,
    // maxHeight: '85%',
    backgroundColor: '#fff',
    borderRadius: 34,
    overflow: 'hidden',
    shadowColor: '#000',
    shadowOffset: { width: 0, height: 10 },
    shadowOpacity: 0.1,
    shadowRadius: 20,
    elevation: 5,
  },
  scrollContent: {
    padding: 24,
  },

  header: {
    flexDirection: 'row',
    justifyContent: 'space-between',
    alignItems: 'center',
    marginBottom: 24,
  },
  title: {
    fontSize: 20,
    fontWeight: '700',
    color: '#1A1A1A',
  },
  closeButton: {
    padding: 4,
  },
  section: {
    marginBottom: 20,
  },
  grid: {
    flexDirection: 'row',
    flexWrap: 'wrap',
    gap: 12,
  },
  tag: {
    width: '48.5%', // 2 columns with gap
    height: 48,
    backgroundColor: '#F5F5F7',
    borderRadius: 34,
    justifyContent: 'center',
    alignItems: 'center',
    paddingHorizontal: 8,
    borderWidth: 1,
    borderColor: 'transparent',
  },
  tagSelected: {
    backgroundColor: '#007AFF',
    borderColor: 'transparent',
  },
  tagText: {
    fontSize: 14,
    color: '#48484A',
    fontWeight: '500',
  },
  tagTextSelected: {
    color: '#fff',
  },
  input: {
    fontSize: 17,
    color: '#333',
    minHeight: 120,
    textAlignVertical: 'top',
    padding: 0,
  },
  feedbackInputWrapper: {
    backgroundColor: '#F5F5F7',
    borderRadius: 34,
    paddingHorizontal: 13,
    paddingTop: 18,
    paddingBottom: 9,
  },

  footer: {
    flexDirection: 'row',
    gap: 16,
    marginTop: 8,
  },
  button: {
    flex: 1,
    height: 52,
    borderRadius: 26,
    justifyContent: 'center',
    alignItems: 'center',
  },
  cancelButton: {
    backgroundColor: '#F2F2F7',
  },
  cancelButtonText: {
    fontSize: 16,
    fontWeight: '600',
    color: '#48484A',
  },
  submitButton: {
    backgroundColor: '#007AFF',
  },
  submitButtonText: {
    fontSize: 16,
    fontWeight: '600',
    color: '#fff',
  },
  buttonDisabled: {
    opacity: 0.5,
  },
  successContainer: {
    backgroundColor: '#fff',
    padding: 40,
    borderRadius: 24,
    alignItems: 'center',
    gap: 16,
  },
  successText: {
    fontSize: 18,
    fontWeight: '600',
    color: '#1A1A1A',
  },
  errorText: {
    color: '#FF3B30',
    fontSize: 14,
    marginBottom: 16,
    textAlign: 'center',
  },
});

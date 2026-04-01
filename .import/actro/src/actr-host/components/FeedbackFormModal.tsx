import React, { useState } from 'react';
import {
  View,
  StyleSheet,
  Modal,
  TextInput,
  TouchableOpacity,
  Pressable,
  TouchableWithoutFeedback,
  ActivityIndicator,
  Animated,
  KeyboardAvoidingView,
  ScrollView,
  Platform,
} from 'react-native';
import { useTranslation } from 'react-i18next';
import { Icon } from './Icon';
import { submitFeedback, FeedbackContext } from '../../api/feedback';
import { useMessageStore } from '../store/useMessageStore';
import dayjs from 'dayjs';
import { GlobalAlert } from '../../../src/components';
import { isPhone } from '../../utils/device';
import { AppText } from '../../components';
import { getTypographyStyle, spacing } from '../../theme/tokens';
import { HOST_SHADOWS } from '../styles/shadows';

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
  tabId?: string;
  appId?: string;
  messageId?: string;
}

export const FeedbackFormModal: React.FC<FeedbackFormModalProps> = ({
  visible,
  onClose,
  tabId,
  appId,
  messageId,
}) => {
  const { t } = useTranslation();

  const [selectedIds, setSelectedIds] = useState<Set<string>>(new Set());
  const [details, setDetails] = useState('');
  const [status, setStatus] = useState<
    'idle' | 'submitting' | 'success' | 'error'
  >('idle');

  const targetMsg = useMessageStore((state) =>
    tabId && messageId ? state.messageById[tabId]?.[messageId] : undefined,
  );

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

  // 中文注释：Android 的 Modal 场景下不做 KeyboardAvoidingView 避让，
  // 避免键盘弹起时触发整块弹窗先上跳再回落的闪动。
  const shouldUseKeyboardAvoidingView = Platform.OS === 'ios';

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
      GlobalAlert.alert(
        t('common.hint'),
        t('actr.chat.selectCategoryTip') || '请至少选择一个举报分类',
      );
      return;
    }

    setStatus('submitting');
    try {
      const selectedTypes = CATEGORIES.filter((c) => selectedIds.has(c.id)).map(
        (c) => t(c.key as any),
      );

      const contextPayload: FeedbackContext[] = [];

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

  const modalContent = (
    <TouchableWithoutFeedback onPress={handleClose}>
      <View style={[styles.backdrop, isPhone && styles.backdropPhone]}>
        <TouchableWithoutFeedback>
          <Animated.View
            style={[
              styles.container,
              isPhone && styles.containerPhone,
              {
                transform: [{ translateY }],
              },
            ]}
          >
            <ScrollView
              bounces={false}
              showsVerticalScrollIndicator={false}
              contentContainerStyle={[
                styles.scrollContent,
                isPhone && styles.scrollContentPhone,
              ]}
              keyboardShouldPersistTaps='handled'
            >
              <View style={[styles.header, isPhone && styles.headerPhone]}>
                <AppText style={[styles.title, isPhone && styles.titlePhone]}>
                  {t('actr.chat.feedbackTitle')}
                </AppText>
                {!isPhone && (
                  <TouchableOpacity
                    onPress={handleClose}
                    style={styles.closeButton}
                  >
                    <Icon type='close' size={20} color='#999' />
                  </TouchableOpacity>
                )}
              </View>

              <View style={styles.section}>
                <View style={[styles.grid, isPhone && styles.gridPhone]}>
                  {CATEGORIES.map((cat) => {
                    const isSelected = selectedIds.has(cat.id);
                    return (
                      <Pressable
                        key={cat.id}
                        onPress={() => toggleCategory(cat.id)}
                        style={[
                          styles.tag,
                          isPhone && styles.tagPhone,
                          isPhone && { width: undefined },
                          isSelected && styles.tagSelected,
                          isSelected && isPhone && styles.tagSelectedPhone,
                        ]}
                      >
                        <AppText
                          style={[
                            styles.tagText,
                            isPhone && styles.tagTextPhone,
                            isSelected && styles.tagTextSelected,
                          ]}
                          numberOfLines={1}
                        >
                          {t(cat.key as any)}
                        </AppText>
                      </Pressable>
                    );
                  })}
                </View>
              </View>

              <View style={styles.section}>
                <View
                  style={[
                    styles.feedbackInputWrapper,
                    isPhone && styles.feedbackInputWrapperPhone,
                  ]}
                >
                  <TextInput
                    style={[styles.input, isPhone && styles.inputPhone]}
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
                <AppText style={styles.errorText}>
                  {t('actr.chat.feedbackFailed')}
                </AppText>
              )}

              <View style={[styles.footer, isPhone && styles.footerPhone]}>
                <TouchableOpacity
                  style={[
                    styles.button,
                    styles.cancelButton,
                    isPhone && styles.buttonPhone,
                    isPhone && styles.cancelButtonPhone,
                  ]}
                  onPress={handleClose}
                >
                  <AppText
                    style={[
                      styles.cancelButtonText,
                      isPhone && styles.cancelButtonTextPhone,
                    ]}
                  >
                    {t('common.cancel')}
                  </AppText>
                </TouchableOpacity>
                <TouchableOpacity
                  style={[
                    styles.button,
                    styles.submitButton,
                    isPhone && styles.buttonPhone,
                    isPhone && styles.submitButtonPhone,
                    status === 'submitting' && styles.buttonDisabled,
                  ]}
                  onPress={handleSubmit}
                  disabled={status === 'submitting'}
                >
                  {status === 'submitting' ? (
                    <ActivityIndicator color='#fff' size='small' />
                  ) : (
                    <AppText
                      style={[
                        styles.submitButtonText,
                        isPhone && styles.submitButtonTextPhone,
                      ]}
                    >
                      {t('actr.chat.submit')}
                    </AppText>
                  )}
                </TouchableOpacity>
              </View>
            </ScrollView>
          </Animated.View>
        </TouchableWithoutFeedback>
      </View>
    </TouchableWithoutFeedback>
  );

  if (status === 'success') {
    return (
      <Modal visible={visible} transparent animationType='fade'>
        <View style={[styles.backdrop, isPhone && styles.backdropPhone]}>
          <View style={styles.successContainer}>
            <Icon type='copy' size={40} color='#34C759' />
            <AppText style={styles.successText}>
              {t('actr.chat.feedbackSuccess')}
            </AppText>
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
          <View style={[styles.backdrop, isPhone && styles.backdropPhone]}>
            <TouchableWithoutFeedback>
              <Animated.View
                style={[
                  styles.container,
                  isPhone && styles.containerPhone,
                  {
                    transform: [{ translateY }],
                  },
                ]}
              >
                <ScrollView
                  bounces={false}
                  showsVerticalScrollIndicator={false}
                  contentContainerStyle={[
                    styles.scrollContent,
                    isPhone && styles.scrollContentPhone,
                  ]}
                  keyboardShouldPersistTaps='handled'
                >
                  <View style={[styles.header, isPhone && styles.headerPhone]}>
                    <AppText
                      style={[styles.title, isPhone && styles.titlePhone]}
                    >
                      {t('actr.chat.feedbackTitle')}
                    </AppText>
                    {!isPhone && (
                      <TouchableOpacity
                        onPress={handleClose}
                        style={styles.closeButton}
                      >
                        <Icon type='close' size={20} color='#999' />
                      </TouchableOpacity>
                    )}
                  </View>

                  <View style={styles.section}>
                    <View style={[styles.grid, isPhone && styles.gridPhone]}>
                      {CATEGORIES.map((cat) => {
                        const isSelected = selectedIds.has(cat.id);
                        return (
                          <Pressable
                            key={cat.id}
                            onPress={() => toggleCategory(cat.id)}
                            style={[
                              styles.tag,
                              isPhone && styles.tagPhone,
                              isPhone && { width: undefined },
                              isSelected && styles.tagSelected,
                              isSelected && isPhone && styles.tagSelectedPhone,
                            ]}
                          >
                            <AppText
                              style={[
                                styles.tagText,
                                isPhone && styles.tagTextPhone,
                                isSelected && styles.tagTextSelected,
                              ]}
                              numberOfLines={1}
                            >
                              {t(cat.key as any)}
                            </AppText>
                          </Pressable>
                        );
                      })}
                    </View>
                  </View>

                  <View style={styles.section}>
                    <View
                      style={[
                        styles.feedbackInputWrapper,
                        isPhone && styles.feedbackInputWrapperPhone,
                      ]}
                    >
                      <TextInput
                        style={[styles.input, isPhone && styles.inputPhone]}
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
                    <AppText style={styles.errorText}>
                      {t('actr.chat.feedbackFailed')}
                    </AppText>
                  )}

                  <View style={[styles.footer, isPhone && styles.footerPhone]}>
                    <TouchableOpacity
                      style={[
                        styles.button,
                        styles.cancelButton,
                        isPhone && styles.buttonPhone,
                        isPhone && styles.cancelButtonPhone,
                      ]}
                      onPress={handleClose}
                    >
                      <AppText
                        style={[
                          styles.cancelButtonText,
                          isPhone && styles.cancelButtonTextPhone,
                        ]}
                      >
                        {t('common.cancel')}
                      </AppText>
                    </TouchableOpacity>
                    <TouchableOpacity
                      style={[
                        styles.button,
                        styles.submitButton,
                        isPhone && styles.buttonPhone,
                        isPhone && styles.submitButtonPhone,
                        status === 'submitting' && styles.buttonDisabled,
                      ]}
                      onPress={handleSubmit}
                      disabled={status === 'submitting'}
                    >
                      {status === 'submitting' ? (
                        <ActivityIndicator color='#fff' size='small' />
                      ) : (
                        <AppText
                          style={[
                            styles.submitButtonText,
                            isPhone && styles.submitButtonTextPhone,
                          ]}
                        >
                          {t('actr.chat.submit')}
                        </AppText>
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
  backdropPhone: {
    paddingVertical: 20,
  },
  container: {
    width: '90%',
    maxWidth: 500,
    backgroundColor: '#fff',
    borderRadius: 34,
    overflow: 'hidden',
    ...HOST_SHADOWS.modalSurface,
  },
  containerPhone: {
    width: '90%',
    borderRadius: 34,
  },
  scrollContent: {
    padding: spacing.xxl,
  },
  scrollContentPhone: {
    padding: 14,
  },

  header: {
    flexDirection: 'row',
    justifyContent: 'space-between',
    alignItems: 'center',
    marginBottom: 24,
  },
  headerPhone: {
    marginBottom: 0,
  },
  title: {
    ...getTypographyStyle('title2'),
    fontWeight: '700',
    color: '#1A1A1A',
  },
  titlePhone: {
    ...getTypographyStyle('title3'),
    color: '#292929',
    marginTop: 8,
    marginLeft: 8,
    marginRight: 8,
    marginBottom: 24,
  },
  closeButton: {
    padding: 4,
  },
  section: {
    marginBottom: spacing.xl,
  },
  grid: {
    flexDirection: 'row',
    flexWrap: 'wrap',
    gap: spacing.md,
  },
  gridPhone: {
    gap: 10,
  },
  tag: {
    width: '48.5%',
    height: 48,
    backgroundColor: '#F5F5F7',
    borderRadius: 34,
    justifyContent: 'center',
    alignItems: 'center',
    paddingHorizontal: 8,
    borderWidth: 1,
    borderColor: 'transparent',
  },
  tagPhone: {
    height: 28,
    paddingHorizontal: 10,
    paddingVertical: 4,
  },
  tagSelected: {
    backgroundColor: '#007AFF',
    borderColor: 'transparent',
  },
  tagSelectedPhone: {
    backgroundColor: '#34A8FB',
  },
  tagText: {
    ...getTypographyStyle('body'),
    color: '#48484A',
    fontWeight: '500',
  },
  tagTextPhone: {
    fontSize: 15,
  },
  tagTextSelected: {
    color: '#fff',
  },
  input: {
    ...getTypographyStyle('editorInputCompact'),
    color: '#333',
    minHeight: 120,
    textAlignVertical: 'top',
    padding: 0,
  },
  inputPhone: {
    ...getTypographyStyle('editorInputCompact'),
    minHeight: 71,
  },
  feedbackInputWrapper: {
    backgroundColor: '#F5F5F7',
    borderRadius: 34,
    paddingHorizontal: 13,
    paddingTop: 18,
    paddingBottom: 9,
  },
  feedbackInputWrapperPhone: {
    backgroundColor: '#E3E3E3',
    borderRadius: 26,
  },

  footer: {
    flexDirection: 'row',
    gap: 16,
    marginTop: 8,
  },
  footerPhone: {
    gap: 12,
    marginTop: 4,
  },
  button: {
    flex: 1,
    height: 52,
    borderRadius: 26,
    justifyContent: 'center',
    alignItems: 'center',
  },
  buttonPhone: {
    height: 48,
    borderRadius: 24,
  },
  cancelButton: {
    backgroundColor: '#F2F2F7',
  },
  cancelButtonPhone: {
    backgroundColor: '#D9D9D9',
  },
  cancelButtonText: {
    ...getTypographyStyle('bodyInput'),
    fontWeight: '600',
    color: '#48484A',
  },
  cancelButtonTextPhone: {
    fontSize: 15,
  },
  submitButton: {
    backgroundColor: '#007AFF',
  },
  submitButtonPhone: {
    backgroundColor: '#34A8FB',
  },
  submitButtonText: {
    ...getTypographyStyle('bodyInput'),
    fontWeight: '600',
    color: '#fff',
  },
  submitButtonTextPhone: {
    fontSize: 15,
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
    ...getTypographyStyle('statusTitle'),
    color: '#1A1A1A',
  },
  errorText: {
    color: '#FF3B30',
    ...getTypographyStyle('body'),
    marginBottom: spacing.lg,
    textAlign: 'center',
  },
});

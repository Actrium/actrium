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
import { Icon } from '../../../components';
import { submitFeedback, FeedbackContext } from '../../../api/feedback';
import { useMessageStore } from '../../store/useMessageStore';
import { useColors } from '../../../theme/useColors';
import dayjs from 'dayjs';
import { GlobalAlertModal } from '../../../components/GlobalAlertModal';
import { isPhone } from '../../../utils/device';
import { AppText } from '../../../components';
import { getTypographyStyle, spacing } from '../../../theme/tokens';
import { getHostShadows } from '../../styles/shadows';
import { useUserStore } from '../../store/useUserStore';
import type { ColorTokens } from '../../../theme/colors';

interface Category {
  id: string;
  key: string;
}

const CATEGORIES: Category[] = [
  { id: 'harmful', key: 'actr.chat.harmful' },
  { id: 'sexual', key: 'actr.chat.sexual' },
  { id: 'privacy', key: 'actr.chat.privacy' },
  { id: 'abusive', key: 'actr.chat.abusive' },
  { id: 'illegal', key: 'actr.chat.illegal' },
  { id: 'fake', key: 'actr.chat.fake' },
  { id: 'copyright', key: 'actr.chat.copyright' },
  { id: 'other', key: 'actr.chat.other' },
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
  const colors = useColors();
  const styles = React.useMemo(() => makeStyles(colors), [colors]);

  const [selectedIds, setSelectedIds] = useState<Set<string>>(new Set());
  const [details, setDetails] = useState('');
  const [status, setStatus] = useState<'idle' | 'submitting' | 'success' | 'error'>('idle');

  const targetMsg = useMessageStore((state) =>
    tabId && messageId ? state.messageById[tabId]?.[messageId] : undefined,
  );
  const user = useUserStore((state) => state.userInfo?.user);

  const [showCategoryTip, setShowCategoryTip] = React.useState(false);

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

  // Android It's... Modal We're not doing it. KeyboardAvoidingView (It is) to avoid,
  // Avoids flashing when the keyboard starts to bounce and triggers a whole window of bullets before jumping back.
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
      setShowCategoryTip(true);
      return;
    }

    setStatus('submitting');
    try {
      const selectedTypes = CATEGORIES.filter((c) => selectedIds.has(c.id)).map(
        (c) => t(c.key as any),
      );

      const contextPayload: FeedbackContext[] = [];

      if (targetMsg) {
        contextPayload.push({
          role: 'user',
          message_id: targetMsg.message_id,
          content:
            targetMsg.question_type === 'text' ? targetMsg.question_content : 'Voice message',
          created_at: dayjs(targetMsg.question_time).format('YYYY-MM-DD HH:mm:ss'),
        });

        contextPayload.push({
          role: 'assistant',
          message_id: targetMsg.message_id,
          content:
            targetMsg.answer_type === 'text' ? targetMsg.answer_content : 'Voice message',
          created_at: dayjs(targetMsg.answer_time).format('YYYY-MM-DD HH:mm:ss'),
        });
      }

      const res = await submitFeedback({
        app_id: appId || '',
        user_id: user?.cn || '',
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
              { transform: [{ translateY }] },
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
                  <TouchableOpacity onPress={handleClose} style={styles.closeButton}>
                    <Icon type='close' size={20} color={colors.textTertiary} />
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
                    {
                      backgroundColor: colors.bgInputPhone,
                    }
                  ]}
                >
                  <TextInput
                    style={[styles.input, isPhone && styles.inputPhone]}
                    placeholder={t('actr.chat.feedbackPlaceholder')}
                    placeholderTextColor={colors.textTertiary}
                    multiline
                    numberOfLines={4}
                    value={details}
                    onChangeText={setDetails}
                    textAlignVertical='top'
                  />
                </View>
              </View>

              {status === 'error' && (
                <AppText style={styles.errorText}>{t('actr.chat.feedbackFailed')}</AppText>
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
                    style={[styles.cancelButtonText, isPhone && styles.cancelButtonTextPhone]}
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
                    <ActivityIndicator color={colors.textInverse} size='small' />
                  ) : (
                    <AppText
                      style={[styles.submitButtonText, isPhone && styles.submitButtonTextPhone]}
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
            <Icon type='copy' size={40} color={colors.successIcon} />
            <AppText style={styles.successText}>{t('actr.chat.feedbackSuccess')}</AppText>
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
      {shouldUseKeyboardAvoidingView ? (
        <KeyboardAvoidingView behavior='padding' style={{ flex: 1 }}>
          {modalContent}
        </KeyboardAvoidingView>
      ) : (
        <View style={{ flex: 1 }}>{modalContent}</View>
      )}

      <GlobalAlertModal
        visible={showCategoryTip}
        title={t('common.hint')}
        message={t('actr.chat.selectCategoryTip')}
        onClose={() => setShowCategoryTip(false)}
        buttons={[
          { text: t('common.ok') || 'Sure.', onPress: () => setShowCategoryTip(false) },
        ]}
      />
    </Modal>
  );
};

const makeStyles = (colors: ColorTokens) => {
  const hostShadows = getHostShadows(colors);

  return StyleSheet.create({
  backdrop: {
    flex: 1,
    backgroundColor: colors.bgOverlay,
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
    backgroundColor: colors.bgSurface,
    borderRadius: 34,
    overflow: 'hidden',
    ...hostShadows.modalSurface,
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
    color: colors.textModalTitle,
  },
  titlePhone: {
    ...getTypographyStyle('title3'),
    color: colors.textModalTitlePhone,
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
    backgroundColor: colors.bgTag,
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
    backgroundColor: colors.bgTagSelected,
    borderColor: 'transparent',
  },
  tagSelectedPhone: {
    backgroundColor: colors.bgTagSelectedPhone,
  },
  tagText: {
    ...getTypographyStyle('body'),
    fontWeight: '500',
    color: colors.textSecondary,
  },
  tagTextPhone: {
    fontSize: 15,
  },
  tagTextSelected: {
    color: colors.textInverse,
  },
  input: {
    ...getTypographyStyle('editorInputCompact'),
    color: colors.textInput,
    minHeight: 120,
    textAlignVertical: 'top',
    padding: 0,
  },
  inputPhone: {
    ...getTypographyStyle('editorInputCompact'),
    minHeight: 71,
  },
  feedbackInputWrapper: {
    backgroundColor: colors.bgInput,
    borderRadius: 34,
    paddingHorizontal: 13,
    paddingTop: 18,
    paddingBottom: 9,
  },
  feedbackInputWrapperPhone: {
    backgroundColor: colors.bgInputPhone,
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
    backgroundColor: colors.bgCancelButton,
  },
  cancelButtonPhone: {
    backgroundColor: colors.bgCancelButtonPhone,
  },
  cancelButtonText: {
    ...getTypographyStyle('bodyInput'),
    fontWeight: '600',
    color: colors.textCancelButton,
  },
  cancelButtonTextPhone: {
    fontSize: 15,
  },
  submitButton: {
    backgroundColor: colors.bgSubmitButton,
  },
  submitButtonPhone: {
    backgroundColor: colors.bgSubmitButtonPhone,
  },
  submitButtonText: {
    ...getTypographyStyle('bodyInput'),
    fontWeight: '600',
    color: colors.textInverse,
  },
  submitButtonTextPhone: {
    fontSize: 15,
  },
  buttonDisabled: {
    opacity: 0.5,
  },
  successContainer: {
    backgroundColor: colors.bgSurface,
    padding: 40,
    borderRadius: 24,
    alignItems: 'center',
    gap: 16,
  },
  successText: {
    ...getTypographyStyle('statusTitle'),
    color: colors.textModalTitle,
  },
  errorText: {
    ...getTypographyStyle('body'),
    color: colors.danger,
    marginBottom: spacing.lg,
    textAlign: 'center',
  },
});
};

import React from 'react';
import { View, StyleSheet, Pressable } from 'react-native';
import { useTranslation } from 'react-i18next';
import { AppText, HostIconButton } from '../../../components';
import { getTypographyStyle, inputArea, spacing } from '../../../theme/tokens';
import { useColors } from '../../../theme/useColors';
import type { ColorTokens } from '../../../theme/colors';

interface DeleteButtonProps {
  selectedCount: number;
  onPress: () => void;
  onCancel: () => void;
}

export const DeleteButton: React.FC<DeleteButtonProps> = ({
  selectedCount,
  onPress,
  onCancel,
}) => {
  const { t } = useTranslation();
  const colors = useColors();
  const styles = React.useMemo(() => makeStyles(colors), [colors]);

  return (
    <View style={styles.footer}>
      <View style={styles.footerRow}>
        <Pressable
          style={({ pressed }) => [
            styles.deleteButton,
            styles.cancelButton,
            pressed && styles.buttonPressed,
          ]}
          onPress={onCancel}
        >
          <AppText style={styles.cancelButtonText}>
            {t('common.cancel')}
          </AppText>
        </Pressable>
        <Pressable
          style={({ pressed }) => [
            styles.deleteButton,
            pressed && styles.buttonPressed,
          ]}
          onPress={onPress}
        >
          <AppText style={styles.deleteButtonText}>
            {t('actr.input.deleteWithCount', { count: selectedCount })}
          </AppText>
        </Pressable>
      </View>
    </View>
  );
};

const makeStyles = (colors: ColorTokens) =>
  StyleSheet.create({
    footer: {
      height: 118,
      justifyContent: 'center',
      width: '100%',
      paddingLeft: spacing.xl,
      paddingRight: spacing.xl,
      backgroundColor: colors.bgChatPage,
    },
    footerRow: {
      width: '100%',
      flexDirection: 'row',
      alignItems: 'center',
      paddingHorizontal: spacing.lg,
      gap: spacing.md,
    },
    cancelButton: {
      backgroundColor: colors.bgSurface,
    },
    deleteButton: {
      flex: 1,
      height: inputArea.minHeight,
      borderRadius: inputArea.minHeight / 2,
      backgroundColor: colors.danger,
      alignItems: 'center',
      justifyContent: 'center',
    },
    buttonPressed: {
      opacity: 0.88,
    },
    cancelButtonText: {
      ...getTypographyStyle('bodyInput'),
      color: colors.textSecondary,
      fontWeight: '600',
    },
    deleteButtonText: {
      color: colors.textInverse,
      ...getTypographyStyle('bodyInput'),
      fontWeight: '600',
    },
  });

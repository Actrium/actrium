import React from 'react';
import { View, StyleSheet, Pressable } from 'react-native';
import LinearGradient from 'react-native-linear-gradient';
import { useTranslation } from 'react-i18next';
import { AppText } from '../../../components';
import { deleteActionBar, getTypographyStyle, spacing } from '../../../theme/tokens';
import { HOST_SHADOWS } from '../../styles/shadows';
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
  const gradientColors = React.useMemo(() => [
    colors.bgDeleteGradientStart,
    colors.bgDeleteGradientMid,
    colors.bgDeleteGradientEnd,
  ], [colors]);

  return (
    <>
      {/* 渐变背景遮罩 */}
      <View style={styles.overlay} pointerEvents="box-none">
        <LinearGradient
          colors={gradientColors}
          locations={[0, 0.5, 1]}
          style={styles.gradient}
        />
      </View>
      <View style={styles.container}>
        <View style={styles.buttonsRow}>
          <Pressable
            onPress={onCancel}
            style={({ pressed }) => [
              styles.cancelButton,
              pressed && styles.buttonPressed,
            ]}
          >
            <AppText style={styles.cancelButtonText}>{t('common.cancel')}</AppText>
          </Pressable>
          <Pressable
            onPress={onPress}
            style={({ pressed }) => [
              styles.button,
              pressed && styles.buttonPressed,
            ]}
          >
            <AppText style={styles.buttonText}>
              {t('actr.input.deleteWithCount', { count: selectedCount })}
            </AppText>
          </Pressable>
        </View>
      </View>
    </>
  );
};

const makeStyles = (colors: ColorTokens) => StyleSheet.create({
  overlay: {
    position: 'absolute',
    bottom: -130,
    left: 0,
    right: 0,
    height: 200,
    zIndex: 999,
  },
  gradient: {
    flex: 1,
    width: '100%',
  },
  container: {
    alignItems: 'center',
    justifyContent: 'center',
    zIndex: 1000,
  },
  buttonsRow: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: spacing.md,
  },
  cancelButton: {
    backgroundColor: colors.bgSurface,
    paddingHorizontal: deleteActionBar.buttonPaddingHorizontal,
    paddingVertical: deleteActionBar.buttonPaddingVertical,
    borderRadius: 24,
    minWidth: deleteActionBar.buttonMinWidth,
    alignItems: 'center',
    justifyContent: 'center',
    ...HOST_SHADOWS.inputPill,
  },
  cancelButtonText: {
    color: colors.textIconPrimary,
    ...getTypographyStyle(deleteActionBar.buttonTypographyVariant),
  },
  button: {
    backgroundColor: colors.danger,
    paddingHorizontal: deleteActionBar.buttonPaddingHorizontal,
    paddingVertical: deleteActionBar.buttonPaddingVertical,
    borderRadius: 24,
    minWidth: deleteActionBar.buttonMinWidth,
    alignItems: 'center',
    justifyContent: 'center',
    ...HOST_SHADOWS.inputPill,
  },
  buttonPressed: {
    opacity: 0.8,
  },
  buttonText: {
    color: colors.textInverse,
    ...getTypographyStyle(deleteActionBar.buttonTypographyVariant),
  },
});

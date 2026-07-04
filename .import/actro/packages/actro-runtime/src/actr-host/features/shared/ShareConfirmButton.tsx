import React from 'react';
import { View, StyleSheet, Pressable } from 'react-native';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import { useTranslation } from 'react-i18next';
import LinearGradient from 'react-native-linear-gradient';
import { AppText } from '../../../components';
import {
  getTypographyStyle,
  inputArea,
  selectionActionBar,
  spacing,
} from '../../../theme/tokens';
import { useColors } from '../../../theme/useColors';
import type { ColorTokens } from '../../../theme/colors';
import { isTablet } from '../../../utils/device';

interface ShareConfirmButtonProps {
  onPress: () => void;
  onCancel: () => void;
}

// 分享选择模式底部操作栏，包含取消和生成预览图按钮。
export const ShareConfirmButton: React.FC<ShareConfirmButtonProps> = ({
  onPress,
  onCancel,
}) => {
  const { t } = useTranslation();
  const insets = useSafeAreaInsets();
  const colors = useColors();
  const bottomPadding = isTablet
    ? 0
    : Math.max(insets.bottom, selectionActionBar.phoneMinBottomPadding);
  const styles = React.useMemo(
    () => makeStyles(colors, bottomPadding),
    [bottomPadding, colors],
  );

  return (
    <View style={styles.footer}>
      <LinearGradient
        // 前 1/3 区域完成透明到实体背景的过渡，后 2/3 保持纯色不再继续渐变。
        colors={[
          colors.bgDeleteGradientStart,
          colors.bgDeleteGradientEnd,
          colors.bgDeleteGradientEnd,
        ]}
        locations={[0, 0.33, 1]}
        style={styles.footerGradient}
      />
      <View style={styles.footerRow}>
        <Pressable
          style={({ pressed }) => [
            styles.actionButton,
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
            styles.actionButton,
            styles.shareButton,
            pressed && styles.buttonPressed,
          ]}
          onPress={onPress}
        >
          <AppText style={styles.shareButtonText}>
            {t('actr.chat.shareConfirm')}
          </AppText>
        </Pressable>
      </View>
    </View>
  );
};

const FOOTER_HEIGHT = selectionActionBar.height;
const FOOTER_TOP_SPACE = (FOOTER_HEIGHT - inputArea.minHeight) / 2;

const makeStyles = (colors: ColorTokens, bottomPadding: number) =>
  StyleSheet.create({
    footer: {
      height: isTablet
        ? FOOTER_HEIGHT
        : FOOTER_TOP_SPACE + inputArea.minHeight + bottomPadding,
      justifyContent: isTablet ? 'center' : 'flex-end',
      width: '100%',
      paddingLeft: spacing.xl,
      paddingRight: spacing.xl,
      paddingBottom: bottomPadding,
    },
    footerGradient: {
      ...StyleSheet.absoluteFillObject,
    },
    footerRow: {
      width: '100%',
      flexDirection: 'row',
      alignItems: 'center',
      paddingHorizontal: spacing.lg,
      gap: spacing.md,
    },
    actionButton: {
      flex: 1,
      height: inputArea.minHeight,
      borderRadius: inputArea.minHeight / 2,
      alignItems: 'center',
      justifyContent: 'center',
    },
    cancelButton: {
      backgroundColor: colors.bgChatPage,
    },
    shareButton: {
      backgroundColor: colors.accentBlue,
    },
    buttonPressed: {
      opacity: 0.88,
    },
    cancelButtonText: {
      ...getTypographyStyle('bodyInput'),
      color: colors.textSecondary,
      fontWeight: '600',
    },
    shareButtonText: {
      color: colors.textInverse,
      ...getTypographyStyle('bodyInput'),
      fontWeight: '600',
    },
  });

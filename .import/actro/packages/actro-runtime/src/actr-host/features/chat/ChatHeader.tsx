import React from 'react';
import { View, StyleSheet, useColorScheme } from 'react-native';
import LinearGradient from 'react-native-linear-gradient';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import { useTranslation } from 'react-i18next';
import { HostIconButton } from '../../../components';
import { CHAT_HEADER_HEIGHT } from '../../constants';
import { useNetworkStatus } from '../../hooks/networkStatus';
import { AppText } from '../../../components';
import { getTypographyStyle, spacing } from '../../../theme/tokens';
import { useColors } from '../../../theme/useColors';
import type { ColorTokens } from '../../../theme/colors';
import { useAppStore } from '../../store/useAppStore';

interface ChatHeaderProps {
  onLeftPress: () => void;
  onRightPress?: () => void;
  leftDisabled?: boolean;
  leftActive?: boolean;
  showLeftButton?: boolean;
  instanceName?: string;
  appId?: string;
  includeSafeTopInset?: boolean;
}

export const ChatHeader: React.FC<ChatHeaderProps> = React.memo(
  ({
    onLeftPress,
    onRightPress,
    leftDisabled = false,
    leftActive = false,
    showLeftButton = true,
    instanceName,
    appId,
    includeSafeTopInset = true,
  }) => {
    const { t } = useTranslation();
    const { top: topInsetRaw } = useSafeAreaInsets();
    const topInset = includeSafeTopInset ? topInsetRaw : 0;
    const hasAppId = Boolean(appId);
    const { showOfflineHint } = useNetworkStatus();
    const appStatus = useAppStore((state) =>
      appId ? state.apps[appId]?.app_status : undefined,
    );
    const shouldShowDeadHint =
      hasAppId && !showOfflineHint && appStatus === 'dead';

    const colors = useColors();
    const colorScheme = useColorScheme();
    const styles = React.useMemo(() => makeStyles(colors), [colors]);
    const headerButtonBackgroundColor =
      colorScheme === 'dark' ? undefined : '#fff';

    return (
      <View
        style={[
          styles.header,
          {
            height: CHAT_HEADER_HEIGHT + topInset,
            paddingTop: topInset,
          },
        ]}
      >
        <LinearGradient
          colors={styles.gradientColors}
          locations={[0, 0.7, 1]}
          style={StyleSheet.absoluteFill}
        />
        {showLeftButton ? (
          <HostIconButton
            iconType="extension_menu"
            iconSize={20}
            onPress={onLeftPress}
            disabled={leftDisabled}
            active={leftActive}
            backgroundColor={headerButtonBackgroundColor}
            activeBackgroundColor={colors.bgChatHeaderBtn}
            iconColor={colors.iconChatHeader}
            activeIconColor="#fff"
          />
        ) : (
          <View style={styles.headerButtonPlaceholder} />
        )}
        <View style={styles.titleContainer}>
          {instanceName && (
            <AppText style={styles.instanceName} numberOfLines={1}>
              {instanceName}
            </AppText>
          )}
          {/* 只有插件会话态才展示网络状态，和右侧 history 按钮保持同一语义边界。 */}
          {hasAppId && showOfflineHint ? (
            <AppText style={styles.networkErrorText}>
              {t('common.networkError')}
            </AppText>
          ) : shouldShowDeadHint ? (
            <AppText style={styles.deadStatusText}>已停止服务</AppText>
          ) : null}
        </View>
        {hasAppId ? (
          <HostIconButton
            iconType="history"
            iconSize={22}
            onPress={() => {}}
            style={[
              styles.headerButton,
              {
                opacity: 0,
                pointerEvents: 'none',
              },
            ]}
            backgroundColor={headerButtonBackgroundColor}
            iconColor={colors.iconChatHeader}
          />
        ) : onRightPress ? (
          <HostIconButton
            iconType="close"
            iconSize={16}
            onPress={onRightPress}
            style={styles.headerButton}
            backgroundColor={headerButtonBackgroundColor}
            iconColor={colors.iconChatHeader}
          />
        ) : (
          <View style={styles.headerButtonPlaceholder} />
        )}
      </View>
    );
  },
);

const makeStyles = (colors: ColorTokens) => ({
  gradientColors: [
    colors.bgHeader,
    colors.bgHeader,
    colors.bgHeaderGradientEnd,
  ] as string[],
  ...StyleSheet.create({
    header: {
      position: 'absolute',
      top: 0,
      left: 0,
      right: 0,
      flexDirection: 'row',
      justifyContent: 'space-between',
      alignItems: 'center',
      paddingHorizontal: spacing.lg,
      zIndex: 10,
    },
    headerButton: {
      // backgroundColor: colors.bgLoadDark,
    },
    headerButtonPlaceholder: {
      width: 44,
      height: 44,
    },
    titleContainer: {
      flex: 1,
      marginHorizontal: spacing.md,
      alignItems: 'center',
      justifyContent: 'center',
    },
    instanceName: {
      ...getTypographyStyle('bodyInput'),
      fontWeight: '600',
      color: colors.textPrimary,
      textAlign: 'center',
      maxWidth: '100%',
    },
    networkErrorText: {
      marginTop: 2,
      ...getTypographyStyle('caption1'),
      color: colors.danger,
    },
    deadStatusText: {
      marginTop: 2,
      ...getTypographyStyle('caption1'),
      color: colors.danger,
    },
  }),
});

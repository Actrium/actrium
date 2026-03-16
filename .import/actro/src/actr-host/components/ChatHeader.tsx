import React from 'react';
import { View, StyleSheet, Text } from 'react-native';
import LinearGradient from 'react-native-linear-gradient';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import { HostIconButton } from './HostIconButton';
import { CHAT_HEADER_HEIGHT } from '../constants';

interface ChatHeaderProps {
  onLeftPress: () => void;
  onRightPress: () => void;
  leftDisabled?: boolean;
  leftActive?: boolean;
  showLeftButton?: boolean;
  appName?: string;
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
    appName,
    appId,
    includeSafeTopInset = true,
  }) => {
    const { top: topInsetRaw } = useSafeAreaInsets();
    const topInset = includeSafeTopInset ? topInsetRaw : 0;
    const hasAppId = Boolean(appId);
    const [showCloseButton, setShowCloseButton] = React.useState(false);

    React.useEffect(() => {
      if (hasAppId) {
        setShowCloseButton(false);
        return;
      }
      const timer = setTimeout(() => setShowCloseButton(true), 500);
      return () => clearTimeout(timer);
    }, [hasAppId]);

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
          colors={['#F2F2F6', '#F2F2F6', 'rgba(242, 242, 246, 0)']}
          locations={[0, 0.7, 1]}
          style={StyleSheet.absoluteFill}
        />
        {showLeftButton ? (
          <HostIconButton
            iconType='extension_menu'
            iconSize={20}
            onPress={onLeftPress}
            disabled={leftDisabled}
            active={leftActive}
            style={styles.headerButton}
          />
        ) : (
          <View style={styles.headerButtonPlaceholder} />
        )}
        {appName && (
          <Text style={styles.appName} numberOfLines={1}>
            {appName}
          </Text>
        )}
        {hasAppId ? (
          <HostIconButton
            iconType='history'
            iconSize={22}
            onPress={() => {}}
            style={styles.headerButton}
          />
        ) : showCloseButton ? (
          <HostIconButton
            iconType='close'
            iconSize={16}
            onPress={onRightPress}
            style={styles.headerButton}
          />
        ) : (
          <View style={styles.headerButtonPlaceholder} />
        )}
      </View>
    );
  },
);

const styles = StyleSheet.create({
  header: {
    position: 'absolute',
    top: 0,
    left: 0,
    right: 0,
    flexDirection: 'row',
    justifyContent: 'space-between',
    alignItems: 'center',
    paddingHorizontal: 16,
    zIndex: 10,
  },
  headerButton: {
    // ...HOST_SHADOWS.headerButton,
  },
  headerButtonPlaceholder: {
    width: 44,
    height: 44,
  },
  appName: {
    flex: 1,
    marginHorizontal: 12,
    fontSize: 16,
    fontWeight: '600',
    color: '#0a0a0a',
    textAlign: 'center',
  },
});

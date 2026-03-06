import React from 'react';
import { View, TouchableOpacity, StyleSheet, Text } from 'react-native';
import LinearGradient from 'react-native-linear-gradient';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import { Icon } from './Icon';
import { HOST_SHADOWS } from '../styles/shadows';
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
          <TouchableOpacity
            style={[
              styles.headerButton,
              leftActive && styles.headerButtonActive,
              leftDisabled && styles.headerButtonDisabled,
            ]}
            onPress={onLeftPress}
            disabled={leftDisabled}
          >
            <Icon
              type='extension_menu'
              size={20}
              color={leftActive ? '#FFFFFF' : '#0a0a0a'}
            />
          </TouchableOpacity>
        ) : (
          <View style={styles.headerButtonPlaceholder} />
        )}
        {appName && (
          <Text style={styles.appName} numberOfLines={1}>
            {appName}
          </Text>
        )}
        <TouchableOpacity
          style={styles.headerButton}
          onPress={hasAppId ? () => {} : onRightPress}
        >
          <Icon
            type={hasAppId ? 'history' : 'close'}
            size={hasAppId ? 22 : 16}
            color='#0a0a0a'
          />
        </TouchableOpacity>
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
    width: 44,
    height: 44,
    borderRadius: 22,
    backgroundColor: '#DFDFE1',
    justifyContent: 'center',
    alignItems: 'center',
    // ...HOST_SHADOWS.headerButton,
  },
  headerButtonDisabled: {
    opacity: 0.5,
  },
  headerButtonActive: {
    backgroundColor: '#0088FF',
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

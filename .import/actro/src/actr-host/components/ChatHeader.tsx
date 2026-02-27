import React from 'react';
import { View, TouchableOpacity, StyleSheet, Text } from 'react-native';
import LinearGradient from 'react-native-linear-gradient';
import { Icon } from './Icon';
import { HOST_SHADOWS } from '../styles/shadows';

interface ChatHeaderProps {
  onLeftPress: () => void;
  onRightPress: () => void;
  leftDisabled?: boolean;
  leftActive?: boolean;
  appName?: string;
}

export const ChatHeader: React.FC<ChatHeaderProps> = React.memo(
  ({ onLeftPress, onRightPress, leftDisabled = false, leftActive = false, appName }) => {
    return (
      <View style={styles.header}>
        <LinearGradient
          colors={['#F2F2F6', 'rgba(242, 242, 246, 0)']}
          style={StyleSheet.absoluteFill}
        />
        <TouchableOpacity
          style={[
            styles.headerButton,
            leftActive && styles.headerButtonActive,
            leftDisabled && styles.headerButtonDisabled,
          ]}
          onPress={onLeftPress}
          disabled={leftDisabled}
        >
          <Icon type="extension_menu" size={20} color={leftActive ? '#FFFFFF' : '#0a0a0a'} />
        </TouchableOpacity>
        {appName && (
          <Text style={styles.appName} numberOfLines={1}>
            {appName}
          </Text>
        )}
        <TouchableOpacity
          style={styles.headerButton}
          onPress={onRightPress}
        >
          <Icon type="close" size={16} color="#0a0a0a" />
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
    height: 64,
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
    backgroundColor: '#FFFFFF',
    justifyContent: 'center',
    alignItems: 'center',
    ...HOST_SHADOWS.headerButton,
  },
  headerButtonDisabled: {
    opacity: 0.5,
  },
  headerButtonActive: {
    backgroundColor: '#0088FF',
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

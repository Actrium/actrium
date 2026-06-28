import React from 'react';
import { View, Image, StyleSheet, TouchableOpacity } from 'react-native';
import { Icon } from '../../components';
import { AppText } from '../../components';
import { getTypographyStyle, spacing, controlHeight } from '../../theme/tokens';

interface SettingsItemProps {
  icon: string | any;
  label: string;
  value?: string;
  showChevron?: boolean;
  onPress?: () => void;
  destructive?: boolean;
  iconColor?: string;
}

export const SettingsItem = ({
  icon,
  label,
  value,
  showChevron = true,
  onPress,
  destructive = false,
  iconColor = '#9e9ea7',
}: SettingsItemProps) => {
  return (
    <TouchableOpacity
      style={styles.itemContainer}
      onPress={onPress}
      activeOpacity={0.7}
    >
      {typeof icon === 'string' ? (
        <Icon type={icon} size={22} color={iconColor} />
      ) : (
        <Image source={icon} style={styles.itemIconImage} />
      )}
      <View style={styles.itemContent}>
        <AppText style={[styles.itemLabel, destructive && styles.destructiveText]}>
          {label}
        </AppText>
        {value && <AppText style={styles.itemValue}>{value}</AppText>}
        {showChevron && <Icon type='next' size={16} color='#fff' />}
      </View>
    </TouchableOpacity>
  );
};

const styles = StyleSheet.create({
  itemContainer: {
    flexDirection: 'row',
    alignItems: 'center',
    paddingHorizontal: spacing.lg,
    height: controlHeight.xl,
  },
  itemIconImage: {
    width: 22,
    height: 22,
    resizeMode: 'contain',
  },
  itemContent: {
    flex: 1,
    flexDirection: 'row',
    alignItems: 'center',
    marginLeft: 17,
    paddingRight: spacing.xxs,
  },
  itemLabel: {
    flex: 1,
    ...getTypographyStyle('title3'),
    color: '#FFFFFF',
    fontWeight: '400',
  },
  itemValue: {
    ...getTypographyStyle('title3'),
    color: '#8E8E93',
    marginRight: spacing.sm,
  },
  destructiveText: {
    color: '#FF684E',
  },
});

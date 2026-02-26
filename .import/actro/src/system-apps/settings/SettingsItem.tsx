import React from 'react';
import { View, Text, Image, StyleSheet, TouchableOpacity } from 'react-native';
import { Icon } from '../../actr-host/components/Icon';

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
        <Text style={[styles.itemLabel, destructive && styles.destructiveText]}>
          {label}
        </Text>
        {value && <Text style={styles.itemValue}>{value}</Text>}
        {showChevron && <Icon type='next' size={16} color='#fff' />}
      </View>
    </TouchableOpacity>
  );
};

const styles = StyleSheet.create({
  itemContainer: {
    flexDirection: 'row',
    alignItems: 'center',
    paddingHorizontal: 16,
    height: 52,
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
    paddingRight: 4,
  },
  itemLabel: {
    flex: 1,
    fontSize: 17,
    color: '#FFFFFF',
    fontWeight: '400',
  },
  itemValue: {
    fontSize: 17,
    color: '#8E8E93',
    marginRight: 8,
  },
  destructiveText: {
    fontSize: 17,
    color: '#FF684E',
  },
});

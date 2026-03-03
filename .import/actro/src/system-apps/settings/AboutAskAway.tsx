import React from 'react';
import {
  View,
  Text,
  ScrollView,
  StyleSheet,
  TouchableOpacity,
} from 'react-native';
import { Icon } from '../../actr-host/components/Icon';
import { SettingsBackHeader } from './SettingsBackHeader';

interface AboutAskAwayProps {
  onBack: () => void;
  isTablet: boolean;
}

export const AboutAskAway = ({ onBack, isTablet }: AboutAskAwayProps) => {
  return (
    <ScrollView
      style={styles.content}
      contentContainerStyle={[
        styles.scrollContent,
        {
          paddingHorizontal: isTablet ? 40 : 24,
        },
      ]}
      showsVerticalScrollIndicator={false}
    >
      <SettingsBackHeader onBack={onBack} title='关于 AskAway' marginBottom={24} />

      <View style={styles.groupCard}>
        <AboutItem label='关于 AskAway' />
        <View style={styles.divider} />
        <AboutItem label='用户协议' />
        <View style={styles.divider} />
        <AboutItem label='隐私政策' />
        <View style={styles.divider} />
        <AboutItem label='ICP 备案' />
      </View>
    </ScrollView>
  );
};

interface AboutItemProps {
  label: string;
}

const AboutItem = ({ label }: AboutItemProps) => {
  return (
    <TouchableOpacity style={styles.itemContainer} activeOpacity={0.7}>
      <Text style={styles.itemLabel}>{label}</Text>
      <Icon type='next' size={16} color='#fff' />
    </TouchableOpacity>
  );
};

const styles = StyleSheet.create({
  content: {
    flex: 1,
  },
  scrollContent: {
    paddingTop: 60,
    paddingBottom: 40,
  },
  groupCard: {
    backgroundColor: '#1C1C1E',
    borderRadius: 26,
    overflow: 'hidden',
  },
  itemContainer: {
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'space-between',
    paddingHorizontal: 20,
    height: 52,
  },
  itemLabel: {
    fontSize: 17,
    color: '#FFFFFF',
    fontWeight: '400',
  },
  divider: {
    height: 1,
    backgroundColor: '#2C2C2E',
    marginLeft: 20,
    marginRight: 12,
  },
});

import React from 'react';
import { View, Text, StyleSheet } from 'react-native';
import { HostIconButton } from '../../components';
import { AppText } from '../../components';
import { getTypographyStyle, controlHeight } from '../../theme/tokens';

interface SettingsBackHeaderProps {
  onBack: () => void;
  title?: string;
  marginBottom?: number;
}

export const SettingsBackHeader = ({
  onBack,
  title,
  marginBottom = 32,
}: SettingsBackHeaderProps) => {
  return (
    <View style={[styles.container, { marginBottom }]}>
      <HostIconButton
        size={44}
        backgroundColor='#3a3a3c'
        enablePressScale={false}
        style={styles.backButton}
        onPress={onBack}
        activeOpacity={0.7}
      >
        <Text style={styles.backButtonText}>‹</Text>
      </HostIconButton>
      {title ? (
        <View pointerEvents='none' style={styles.titleWrapper}>
          <AppText style={styles.title} numberOfLines={1}>
            {title}
          </AppText>
        </View>
      ) : null}
    </View>
  );
};

const styles = StyleSheet.create({
  container: {
    height: controlHeight.lg,
    justifyContent: 'center',
  },
  backButton: {
    backgroundColor: '#3a3a3c',
  },
  backButtonText: {
    fontSize: 32,
    color: '#fff',
    fontWeight: '300',
    marginTop: -4,
  },
  titleWrapper: {
    position: 'absolute',
    left: 0,
    right: 0,
    alignItems: 'center',
  },
  title: {
    ...getTypographyStyle('title2'),
    color: '#FFFFFF',
  },
});

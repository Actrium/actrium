import React from 'react';
import { View, TouchableOpacity, Text, StyleSheet } from 'react-native';

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
      <TouchableOpacity
        style={styles.backButton}
        onPress={onBack}
        activeOpacity={0.7}
      >
        <Text style={styles.backButtonText}>‹</Text>
      </TouchableOpacity>
      {title ? (
        <View pointerEvents='none' style={styles.titleWrapper}>
          <Text style={styles.title} numberOfLines={1}>
            {title}
          </Text>
        </View>
      ) : null}
    </View>
  );
};

const styles = StyleSheet.create({
  container: {
    height: 44,
    justifyContent: 'center',
  },
  backButton: {
    width: 44,
    height: 44,
    borderRadius: 22,
    backgroundColor: '#3a3a3c',
    alignItems: 'center',
    justifyContent: 'center',
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
    fontSize: 20,
    fontWeight: '600',
    color: '#FFFFFF',
  },
});

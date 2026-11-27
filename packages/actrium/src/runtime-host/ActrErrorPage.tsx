import React from 'react';
import { StyleSheet, Text, View, useColorScheme } from 'react-native';
import { describeActrError } from './actr-error';

export function ActrErrorPage({ code }: { code: unknown }) {
  const error = describeActrError(code);
  const dark = useColorScheme() === 'dark';
  const backgroundColor = dark ? '#1c1c1e' : '#f6f6f7';
  const color = dark ? '#f5f5f7' : '#1d1d1f';
  const secondaryColor = dark ? '#b3b3b7' : '#59595f';
  return (
    <View style={[styles.page, { backgroundColor }]}>
      <View style={styles.content}>
        <Text accessibilityRole="header" style={[styles.title, { color }]}>{error.title}</Text>
        <Text style={{ color }}>{error.message}</Text>
        <Text selectable style={[styles.code, { color: secondaryColor }]}>{error.code}</Text>
      </View>
    </View>
  );
}

const styles = StyleSheet.create({
  page: { flex: 1, justifyContent: 'center', alignItems: 'center', padding: 24 },
  content: { width: '100%', maxWidth: 420, gap: 8 },
  title: { fontSize: 21, fontWeight: '600' },
  code: { marginTop: 6, fontFamily: 'Menlo', fontSize: 11 },
});

import React from 'react';
import { View, StyleSheet } from 'react-native';
import { useTranslation } from 'react-i18next';
import { AppText, Icon } from '../../components';

export function BookmarkEmptyState() {
  const { t } = useTranslation();

  return (
    <View style={styles.container}>
      <Icon type='bookmark_empty' size={71} disableTint />
      <AppText style={styles.text}>
        {`${t('actr.bookmark.emptyHint')}\n${t('actr.bookmark.emptyHintAction')}`}
      </AppText>
    </View>
  );
}

const styles = StyleSheet.create({
  container: {
    flex: 1,
    justifyContent: 'center',
    alignItems: 'center',
    paddingHorizontal: 24,
  },
  text: {
    marginTop: 25,
    color: '#565656',
    fontSize: 20,
    lineHeight: 30,
    textAlign: 'center',
  },
});

import React, { useEffect } from 'react';
import {
  View,
  ScrollView,
  StyleSheet,
} from 'react-native';
import { GlobalSearchEvents } from '../../native-bridge';

interface GlobalSearchPhoneProps {
  renderResults: () => React.ReactNode;
  resultsHorizontalPadding: number;
  onSearchQueryChange: (query: string) => void;
}

export default function GlobalSearchPhone({
  renderResults,
  resultsHorizontalPadding,
  onSearchQueryChange,
}: GlobalSearchPhoneProps) {
  useEffect(() => {
    // Phone 端搜索框由宿主承接，
    // RN 页面只同步 Native 输入事件并刷新结果区域。
    const unsubscribe = GlobalSearchEvents.onSearchInputChanged((event) => {
      onSearchQueryChange(event.inputText ?? '');
    });

    return unsubscribe;
  }, [onSearchQueryChange]);

  return (
    <View style={styles.phoneContainer}>
      <ScrollView
        style={styles.content}
        contentContainerStyle={[
          styles.phoneScrollContent,
          { paddingHorizontal: resultsHorizontalPadding },
        ]}
        showsVerticalScrollIndicator={false}
        keyboardShouldPersistTaps='handled'
      >
        {renderResults()}
      </ScrollView>
    </View>
  );
}

const styles = StyleSheet.create({
  phoneContainer: {
    flex: 1,
    width: '100%',
    backgroundColor: '#000',
  },
  content: {
    flex: 1,
  },
  phoneScrollContent: {
    paddingTop: 60,
    paddingBottom: 100,
  },
});

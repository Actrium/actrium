import React, { useMemo } from 'react';
import { View, StyleSheet, Platform } from 'react-native';
import { useTranslation } from 'react-i18next';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import { WebView } from 'react-native-webview';
import { SettingsBackHeader } from './SettingsBackHeader';
import { AboutPageKey, getAboutPageUrl } from './aboutPages';
import { isAndroid } from '../../utils/device';

interface AboutWebViewPageProps {
  page: AboutPageKey;
  onBack: () => void;
}

export const AboutWebViewPage = ({ page, onBack }: AboutWebViewPageProps) => {
  const { i18n } = useTranslation();
  const insets = useSafeAreaInsets();
  const isIOS = Platform.OS === 'ios';
  const headerHeight = isIOS ? insets.top + 56 : 0;
  const headerPaddingTop = isIOS ? insets.top + 12 : 0;

  const pageUrl = useMemo(() => {
    return getAboutPageUrl(page, i18n.language || 'zh-Hans');
  }, [page, i18n.language]);

  return (
    <View style={styles.container}>
      {isIOS ? (
        <View
          style={[styles.iosHeader, { height: headerHeight, paddingTop: headerPaddingTop }]}
        >
          <SettingsBackHeader onBack={onBack} marginBottom={0} />
        </View>
      ) : (
        // Android Keep going with the old fixed head height, first press. iOS Standard adjusted second-level pages.
        <View
          style={{
            ...styles.header,
            ...(isAndroid ? { height: 84, paddingTop: 10 } : {}),
          }}
        >
          <SettingsBackHeader onBack={onBack} marginBottom={0} />
        </View>
      )}
      <WebView
        source={{ uri: pageUrl }}
        style={styles.webview}
        forceDarkOn={false}
        contentInsetAdjustmentBehavior='never'
      />
    </View>
  );
};

const styles = StyleSheet.create({
  container: {
    flex: 1,
    backgroundColor: '#FFFFFF',
  },
  header: {
    height: 104,
    paddingTop: 60,
    paddingHorizontal: 16,
    justifyContent: 'center',
    backgroundColor: '#FFFFFF',
  },
  iosHeader: {
    paddingHorizontal: 16,
    justifyContent: 'center',
  },
  webview: {
    flex: 1,
    backgroundColor: '#FFFFFF',
  },
});

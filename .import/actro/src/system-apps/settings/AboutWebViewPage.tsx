import React, { useMemo } from 'react';
import { View, StyleSheet } from 'react-native';
import { useTranslation } from 'react-i18next';
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

  const pageUrl = useMemo(() => {
    return getAboutPageUrl(page, i18n.language || 'zh-Hans');
  }, [page, i18n.language]);

  return (
    <View style={styles.container}>
      {/* 中文注释：当前项目的 safe-area shim 固定返回 0，这里与系统页其余页面保持一致，使用固定 60 顶部留白。 */}
      <View
        style={{
          ...styles.header,
          ...(isAndroid ? { height: 84, paddingTop: 10 } : {}),
        }}
      >
        <SettingsBackHeader onBack={onBack} marginBottom={0} />
      </View>
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
  webview: {
    flex: 1,
    backgroundColor: '#FFFFFF',
  },
});

import { useMemo } from 'react';
import {
  Keyboard,
  StyleSheet,
  Text,
  TouchableWithoutFeedback,
  View,
  useWindowDimensions,
} from 'react-native';

import { useAppStore } from '../store/useAppStore';
import { useTab } from '../contexts/TabContext';
import { useTabIdMapStore } from '../store/useTabIdMapStore';
import { getHostLayoutTokens } from '../../utils/layout';

export default function ChatEmpty() {
  const { width } = useWindowDimensions();
  const layoutTokens = useMemo(() => getHostLayoutTokens(width), [width]);
  const { tab } = useTab();
  const tabIdMap = useTabIdMapStore((state) => state.tabIdMap);
  const appId = tabIdMap[tab.id];
  const appInfo = useAppStore((state) => state.apps[appId]);
  const appInfoByLang = useMemo(() => {
    return appInfo?.language_contents['zh-Hans'];
  }, [appInfo?.app_id]);

  return (
    <TouchableWithoutFeedback
      onPress={Keyboard.dismiss}
      accessible={false}
    >
      <View
        style={[
          styles.centerContent,
          { maxWidth: layoutTokens.contentMaxWidth },
        ]}
      >
        <View style={styles.logoContainer}>
          <Text style={[styles.appName, { fontSize: layoutTokens.chatEmptyTitleSize }]}>
            {appInfo?.app_name}
          </Text>
          <Text style={styles.tabTitle}>
            <Text style={styles.tabTitleBold}>{appInfoByLang?.welcome_text}</Text>
          </Text>
        </View>
      </View>
    </TouchableWithoutFeedback>
  );
}

const styles = StyleSheet.create({
  centerContent: {
    flex: 1,
    width: '100%',
    alignSelf: 'center',
    justifyContent: 'center',
    alignItems: 'center',
  },
  logoContainer: {
    alignItems: 'center',
  },
  appName: {
    fontSize: 60,
    fontWeight: '300',
    color: '#1e2939',
    letterSpacing: 2,
  },
  tabTitle: {
    color: '#364153',
    marginTop: 16,
    fontSize: 14,
  },
  tabTitleBold: {
    fontWeight: '600',
  },
});

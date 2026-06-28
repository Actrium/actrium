import { useMemo } from 'react';
import {
  Keyboard,
  StyleSheet,
  Text,
  TouchableWithoutFeedback,
  View,
  useWindowDimensions,
} from 'react-native';

import { useAppStore } from '../../store/useAppStore';
import { useTab } from '../../contexts/TabContext';
import { useTabIdMapStore } from '../../store/useTabIdMapStore';
import { getHostLayoutTokens } from '../../../utils/layout';
import { AppText } from '../../../components';
import { getTypographyStyle, spacing } from '../../../theme/tokens';
import { useColors } from '../../../theme/useColors';
import type { ColorTokens } from '../../../theme/colors';

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
  const colors = useColors();
  const styles = useMemo(() => makeStyles(colors), [colors]);

  return (
    <TouchableWithoutFeedback onPress={Keyboard.dismiss} accessible={false}>
      <View style={[styles.centerContent, { maxWidth: layoutTokens.contentMaxWidth }]}>
        <View style={styles.logoContainer}>
          <AppText
            variant={layoutTokens.isTablet ? 'emptyTitle' : 'emptyTitleCompact'}
            style={styles.appName}
          >
            {appInfo?.instance_name}
          </AppText>
          <AppText style={styles.tabTitle}>
            <Text style={styles.tabTitleBold}>{appInfoByLang?.welcome_text}</Text>
          </AppText>
        </View>
      </View>
    </TouchableWithoutFeedback>
  );
}

const makeStyles = (colors: ColorTokens) => StyleSheet.create({
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
    color: colors.textAppName,
  },
  tabTitle: {
    marginTop: spacing.lg,
    ...getTypographyStyle('body'),
    color: colors.textWelcome,
  },
  tabTitleBold: {
    fontWeight: '600',
  },
});

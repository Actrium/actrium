import React from 'react';
import {
  GestureResponderEvent,
  PanResponder,
  PanResponderGestureState,
  Platform,
  ScrollView,
  StyleSheet,
  TouchableOpacity,
  View,
} from 'react-native';
import { useTranslation } from 'react-i18next';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import { AppText, Icon } from '../../../components';
import { useColors } from '../../../theme/useColors';
import type { ColorTokens } from '../../../theme/colors';

interface MobileAttachmentPanelProps {
  expanded: boolean;
  showSearchEntry: boolean;
  showNewlineEntry: boolean;
  isWebSearchActive: boolean;
  isEnterForNewLine: boolean;
  onPickCamera: () => void;
  onPickImage: () => void;
  onPickFile: () => void;
  onPickLocation: () => void;
  onToggleWebSearch: () => void;
  onToggleEnterForNewLine: () => void;
}

interface PrimaryAction {
  key: string;
  icon: string;
  label: string;
  onPress: () => void;
}

interface SecondaryAction {
  key: string;
  icon: string;
  title: string;
  subtitle: string;
  active: boolean;
  onPress: () => void;
}

const PANEL_HORIZONTAL_PADDING = 16;
const PAGE_HORIZONTAL_PADDING = PANEL_HORIZONTAL_PADDING + 2;
const PANEL_TOP_PADDING = 16;
const PANEL_ROW_HEIGHT = 60;
const PANEL_DOT_MARGIN_TOP = 18;
const PANEL_DOT_SIZE = 8;
const SECONDARY_ITEM_GAP = 12;
// 中文注释：安卓字体渲染偏大，这里单独收窄附件面板文案字号与行高。
const PRIMARY_TEXT_FONT_SIZE = Platform.OS === 'android' ? 10 : 12;
const PRIMARY_TEXT_LINE_HEIGHT = Platform.OS === 'android' ? 14 : 16;
const SECONDARY_TITLE_FONT_SIZE = Platform.OS === 'android' ? 15 : 17;
const SECONDARY_TITLE_LINE_HEIGHT = Platform.OS === 'android' ? 19 : 21;
const SECONDARY_SUBTITLE_FONT_SIZE = Platform.OS === 'android' ? 11 : 13;
const SECONDARY_SUBTITLE_LINE_HEIGHT = Platform.OS === 'android' ? 14 : 16;

// 中文注释：统一计算附件面板底部留白，安卓兜住手势导航条的最小安全距离。
export const getMobileAttachmentPanelBottomPadding = (bottomInset: number) =>
  Platform.OS === 'android' ? Math.max(bottomInset, 22) : bottomInset;

// 中文注释：统一计算附件面板总高度，保证内部布局和外层动画位移使用同一套尺寸。
export const getMobileAttachmentPanelHeight = (bottomInset: number) =>
  PANEL_TOP_PADDING +
  PANEL_ROW_HEIGHT +
  PANEL_DOT_MARGIN_TOP +
  PANEL_DOT_SIZE +
  getMobileAttachmentPanelBottomPadding(bottomInset);

export default function MobileAttachmentPanel({
  expanded,
  showSearchEntry,
  showNewlineEntry,
  isWebSearchActive,
  isEnterForNewLine,
  onPickCamera,
  onPickImage,
  onPickFile,
  onPickLocation,
  onToggleWebSearch,
  onToggleEnterForNewLine,
}: MobileAttachmentPanelProps) {
  const { t } = useTranslation();
  const insets = useSafeAreaInsets();
  const colors = useColors();
  const panelBottomPadding = getMobileAttachmentPanelBottomPadding(insets.bottom);
  const styles = React.useMemo(
    () => makeStyles(colors, panelBottomPadding),
    [colors, panelBottomPadding],
  );
  const [panelWidth, setPanelWidth] = React.useState(0);
  const [page, setPage] = React.useState(0);
  const pagerRef = React.useRef<ScrollView | null>(null);
  const pageRef = React.useRef(0);

  React.useEffect(() => {
    pageRef.current = page;
  }, [page]);

  const firstPageActions = React.useMemo<PrimaryAction[]>(
    () => [
      {
        key: 'camera',
        icon: 'camera',
        label: t('actr.input.camera'),
        onPress: onPickCamera,
      },
      {
        key: 'photos',
        icon: 'image',
        label: t('actr.input.photos'),
        onPress: onPickImage,
      },
      {
        key: 'files',
        icon: 'file',
        label: t('actr.input.files'),
        onPress: onPickFile,
      },
      {
        key: 'location',
        icon: 'geolocation',
        label: t('actr.input.location'),
        onPress: onPickLocation,
      },
    ],
    [onPickCamera, onPickFile, onPickImage, onPickLocation, t],
  );

  const secondPageActions = React.useMemo<SecondaryAction[]>(
    () =>
      [
        showSearchEntry
          ? {
              key: 'web-search',
              icon: 'network',
              title: t('actr.input.webSearch'),
              subtitle: t('actr.input.webSearchTip'),
              active: isWebSearchActive,
              onPress: onToggleWebSearch,
            }
          : null,
        showNewlineEntry
          ? {
              key: 'enter-setting',
              icon: 'enter',
              title: t('actr.input.enterToNewLine'),
              subtitle: t('actr.input.enterToNewLineTip'),
              active: isEnterForNewLine,
              onPress: onToggleEnterForNewLine,
            }
          : null,
      ].filter((item): item is SecondaryAction => item !== null),
    [
      isEnterForNewLine,
      isWebSearchActive,
      onToggleEnterForNewLine,
      onToggleWebSearch,
      showNewlineEntry,
      showSearchEntry,
      t,
    ],
  );

  // 中文注释：第二页两个卡片直接按页宽二等分，避免状态切换时 flex 重新测量导致宽度抖动。
  const secondaryItemWidth = React.useMemo(() => {
    if (panelWidth <= 0) {
      return undefined;
    }

    return (panelWidth - PAGE_HORIZONTAL_PADDING * 2 - SECONDARY_ITEM_GAP) / 2;
  }, [panelWidth]);

  React.useEffect(() => {
    if (!expanded) {
      return;
    }
    setPage(0);
    requestAnimationFrame(() => {
      pagerRef.current?.scrollTo({ x: 0, animated: false });
    });
  }, [expanded]);

  const handleScrollEnd = React.useCallback(
    (event: any) => {
      if (panelWidth <= 0) {
        return;
      }
      const nextPage = Math.round(
        event.nativeEvent.contentOffset.x / panelWidth,
      );
      setPage(nextPage);
    },
    [panelWidth],
  );

  const panResponder = React.useMemo(
    () =>
      PanResponder.create({
        onStartShouldSetPanResponder: () => false,
        onStartShouldSetPanResponderCapture: () => false,
        onMoveShouldSetPanResponder: (
          _: GestureResponderEvent,
          gestureState: PanResponderGestureState,
        ) =>
          Math.abs(gestureState.dx) > 8 &&
          Math.abs(gestureState.dx) > Math.abs(gestureState.dy),
        onMoveShouldSetPanResponderCapture: (
          _: GestureResponderEvent,
          gestureState: PanResponderGestureState,
        ) =>
          Math.abs(gestureState.dx) > 8 &&
          Math.abs(gestureState.dx) > Math.abs(gestureState.dy),
        onPanResponderRelease: (
          _: GestureResponderEvent,
          gestureState: PanResponderGestureState,
        ) => {
          if (panelWidth <= 0) {
            return;
          }

          const swipeThreshold = 36;
          let nextPage = pageRef.current;

          if (gestureState.dx <= -swipeThreshold) {
            nextPage = Math.min(1, pageRef.current + 1);
          } else if (gestureState.dx >= swipeThreshold) {
            nextPage = Math.max(0, pageRef.current - 1);
          }

          pagerRef.current?.scrollTo({
            x: nextPage * panelWidth,
            animated: true,
          });
          setPage(nextPage);
        },
      }),
    [panelWidth],
  );

  return (
    <View
      style={styles.panel}
      {...panResponder.panHandlers}
      onLayout={(event) => {
        const nextWidth = event.nativeEvent.layout.width;
        if (Math.abs(nextWidth - panelWidth) > 1) {
          setPanelWidth(nextWidth);
        }
      }}
    >
      <View style={styles.body}>
        <ScrollView
          ref={pagerRef}
          horizontal
          pagingEnabled
          showsHorizontalScrollIndicator={false}
          bounces={false}
          onMomentumScrollEnd={handleScrollEnd}
        >
          <View style={[styles.page, { width: panelWidth || undefined }]}>
            <View style={styles.firstPageRow}>
              {firstPageActions.map((item) => (
                <TouchableOpacity
                  key={item.key}
                  style={styles.primaryItem}
                  activeOpacity={0.8}
                  onPress={item.onPress}
                >
                  <Icon
                    type={item.icon}
                    size={22}
                    color={colors.iconMobileAttachment}
                  />
                  <AppText
                    style={styles.primaryText}
                    numberOfLines={1}
                    ellipsizeMode="tail"
                  >
                    {item.label}
                  </AppText>
                </TouchableOpacity>
              ))}
            </View>
          </View>

          <View style={[styles.page, { width: panelWidth || undefined }]}>
            <View style={styles.secondPageRow}>
              {secondPageActions.map((item) => (
                <TouchableOpacity
                  key={item.key}
                  style={[
                    styles.secondaryItem,
                    secondaryItemWidth != null && {
                      width: secondaryItemWidth,
                    },
                    item.active && styles.secondaryItemActive,
                  ]}
                  activeOpacity={0.8}
                  onPress={item.onPress}
                >
                  <View style={styles.secondaryIconWrap}>
                    <Icon
                      type={item.icon}
                      size={22}
                      color={
                        item.active
                          ? colors.iconMobileAttachmentActive
                          : colors.iconMobileAttachment
                      }
                    />
                  </View>
                  <View style={styles.secondaryTextBlock}>
                    <AppText
                      style={[
                        styles.secondaryTitle,
                        item.active && styles.secondaryTitleActive,
                      ]}
                      numberOfLines={1}
                      ellipsizeMode="tail"
                    >
                      {item.title}
                    </AppText>
                    <AppText
                      style={[
                        styles.secondarySubtitle,
                        item.active && styles.secondarySubtitleActive,
                      ]}
                      numberOfLines={1}
                      ellipsizeMode="tail"
                    >
                      {item.subtitle}
                    </AppText>
                  </View>
                </TouchableOpacity>
              ))}
            </View>
          </View>
        </ScrollView>

        <View style={styles.pagerDots}>
          {[0, 1].map((pageIndex) => (
            <View
              key={pageIndex}
              style={[
                styles.pagerDot,
                page === pageIndex && styles.pagerDotActive,
              ]}
            />
          ))}
        </View>
      </View>
    </View>
  );
}

const makeStyles = (colors: ColorTokens, panelBottomPadding: number) =>
  StyleSheet.create({
    panel: {
      marginTop: 0,
      paddingTop: PANEL_TOP_PADDING,
      paddingBottom: panelBottomPadding,
    },
    body: {},
    page: {
      width: '100%',
      paddingHorizontal: PAGE_HORIZONTAL_PADDING,
    },
    firstPageRow: {
      flexDirection: 'row',
      justifyContent: 'space-between',
      gap: 12,
    },
    primaryItem: {
      flex: 1,
      height: PANEL_ROW_HEIGHT,
      borderRadius: 30,
      backgroundColor: colors.bgMobileAttachmentItem,
      alignItems: 'center',
      justifyContent: 'center',
    },
    primaryText: {
      fontSize: PRIMARY_TEXT_FONT_SIZE,
      lineHeight: PRIMARY_TEXT_LINE_HEIGHT,
      color: colors.textMobileAttachmentPrimary,
      marginTop: 4,
      fontWeight: '500',
    },
    secondPageRow: {
      flexDirection: 'row',
      width: '100%',
      justifyContent: 'space-between',
      gap: SECONDARY_ITEM_GAP,
    },
    secondaryItem: {
      flexShrink: 0,
      height: PANEL_ROW_HEIGHT,
      borderRadius: 30,
      backgroundColor: colors.bgMobileAttachmentItem,
      flexDirection: 'row',
      alignItems: 'center',
      paddingHorizontal: 18,
    },
    secondaryItemActive: {
      backgroundColor: colors.bgMobileAttachmentItemActive,
    },
    secondaryIconWrap: {
      width: 28,
      alignItems: 'center',
      justifyContent: 'center',
      marginRight: 12,
    },
    secondaryTextBlock: {
      flex: 1,
      minWidth: 0,
    },
    secondaryTitle: {
      fontSize: SECONDARY_TITLE_FONT_SIZE,
      lineHeight: SECONDARY_TITLE_LINE_HEIGHT,
      color: colors.textMobileAttachmentSecondary,
      fontWeight: '500',
    },
    secondaryTitleActive: {
      color: colors.textMobileAttachmentActive,
    },
    secondarySubtitle: {
      fontSize: SECONDARY_SUBTITLE_FONT_SIZE,
      lineHeight: SECONDARY_SUBTITLE_LINE_HEIGHT,
      color: colors.textMobileAttachmentSubtitle,
      marginTop: 2,
    },
    secondarySubtitleActive: {
      color: colors.textMobileAttachmentActiveSubtitle,
    },
    pagerDots: {
      flexDirection: 'row',
      alignItems: 'center',
      justifyContent: 'center',
      gap: 8,
      marginTop: PANEL_DOT_MARGIN_TOP,
    },
    pagerDot: {
      width: PANEL_DOT_SIZE,
      height: PANEL_DOT_SIZE,
      borderRadius: 4,
      backgroundColor: 'rgba(0,0,0,.3)',
    },
    pagerDotActive: {
      backgroundColor: '#000000',
    },
  });

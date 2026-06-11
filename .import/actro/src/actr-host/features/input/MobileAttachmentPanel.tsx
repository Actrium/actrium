import React from 'react';
import {
  Platform,
  StyleSheet,
  TouchableOpacity,
  View,
} from 'react-native';
import { useTranslation } from 'react-i18next';
import {
  Gesture,
  GestureDetector,
} from 'react-native-gesture-handler';
import Animated, {
  runOnJS,
  useAnimatedStyle,
  useSharedValue,
  withTiming,
} from 'react-native-reanimated';
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
const PAGE_COUNT = 2;
const PAGE_SWIPE_ACTIVATE_OFFSET_X = 10;
const PAGE_SWIPE_FAIL_OFFSET_Y = 8;
const PAGE_SWIPE_VELOCITY_THRESHOLD = 500;
const PAGE_SWIPE_ANIMATION_DURATION = 220;
// 安卓字体渲染偏大，这里单独收窄附件面板文案字号与行高。
const PRIMARY_TEXT_FONT_SIZE = Platform.OS === 'android' ? 10 : 12;
const PRIMARY_TEXT_LINE_HEIGHT = Platform.OS === 'android' ? 14 : 16;
const SECONDARY_TITLE_FONT_SIZE = Platform.OS === 'android' ? 15 : 17;
const SECONDARY_TITLE_LINE_HEIGHT = Platform.OS === 'android' ? 19 : 21;
const SECONDARY_SUBTITLE_FONT_SIZE = Platform.OS === 'android' ? 11 : 13;
const SECONDARY_SUBTITLE_LINE_HEIGHT = Platform.OS === 'android' ? 14 : 16;

// 统一计算附件面板底部留白，安卓兜住手势导航条的最小安全距离。
export const getMobileAttachmentPanelBottomPadding = (bottomInset: number) =>
  Platform.OS === 'android' ? Math.max(bottomInset, 22) : bottomInset;

// 统一计算附件面板总高度，保证内部布局和外层动画位移使用同一套尺寸。
export const getMobileAttachmentPanelHeight = (bottomInset: number) =>
  PANEL_TOP_PADDING +
  PANEL_ROW_HEIGHT +
  PANEL_DOT_MARGIN_TOP +
  PANEL_DOT_SIZE +
  getMobileAttachmentPanelBottomPadding(bottomInset);

function clamp(value: number, min: number, max: number) {
  'worklet';
  return Math.min(Math.max(value, min), max);
}

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
  const translateX = useSharedValue(0);
  const gestureStartTranslateX = useSharedValue(0);

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

  // 第二页两个卡片直接按页宽二等分，避免状态切换时 flex 重新测量导致宽度抖动。
  const secondaryItemWidth = React.useMemo(() => {
    if (panelWidth <= 0) {
      return undefined;
    }

    return (panelWidth - PAGE_HORIZONTAL_PADDING * 2 - SECONDARY_ITEM_GAP) / 2;
  }, [panelWidth]);

  React.useEffect(() => {
    setPage(0);
    translateX.value = 0;
  }, [expanded, translateX]);

  const animateToPage = React.useCallback(
    (nextPage: number) => {
      if (panelWidth <= 0) {
        setPage(0);
        translateX.value = 0;
        return;
      }

      const clampedPage = Math.min(Math.max(nextPage, 0), PAGE_COUNT - 1);
      setPage(clampedPage);
      translateX.value = withTiming(-clampedPage * panelWidth, {
        duration: PAGE_SWIPE_ANIMATION_DURATION,
      });
    },
    [panelWidth, translateX],
  );

  React.useEffect(() => {
    animateToPage(0);
  }, [animateToPage, panelWidth]);

  const panGesture = React.useMemo(
    () =>
      Gesture.Pan()
        .activeOffsetX([
          -PAGE_SWIPE_ACTIVATE_OFFSET_X,
          PAGE_SWIPE_ACTIVATE_OFFSET_X,
        ])
        .failOffsetY([
          -PAGE_SWIPE_FAIL_OFFSET_Y,
          PAGE_SWIPE_FAIL_OFFSET_Y,
        ])
        .onStart(() => {
          gestureStartTranslateX.value = translateX.value;
        })
        .onUpdate((event) => {
          if (panelWidth <= 0) {
            return;
          }

          // 分页位移完全由手势驱动，拖动过程直接更新容器平移，避免 Android ScrollView 在抬手阶段停在中间。
          translateX.value = clamp(
            gestureStartTranslateX.value + event.translationX,
            -panelWidth * (PAGE_COUNT - 1),
            0,
          );
        })
        .onEnd((event) => {
          if (panelWidth <= 0) {
            runOnJS(setPage)(0);
            translateX.value = 0;
            return;
          }

          const progress = -translateX.value / panelWidth;
          let nextPage = Math.round(progress);

          if (event.velocityX <= -PAGE_SWIPE_VELOCITY_THRESHOLD) {
            nextPage = Math.min(PAGE_COUNT - 1, Math.ceil(progress));
          } else if (event.velocityX >= PAGE_SWIPE_VELOCITY_THRESHOLD) {
            nextPage = Math.max(0, Math.floor(progress));
          }

          const clampedPage = Math.min(Math.max(nextPage, 0), PAGE_COUNT - 1);
          translateX.value = withTiming(-clampedPage * panelWidth, {
            duration: PAGE_SWIPE_ANIMATION_DURATION,
          });
          runOnJS(setPage)(clampedPage);
        }),
    [gestureStartTranslateX, panelWidth, translateX],
  );

  const pagerAnimatedStyle = useAnimatedStyle(() => ({
    transform: [{ translateX: translateX.value }],
  }));

  return (
    <View
      style={styles.panel}
      onLayout={(event) => {
        const nextWidth = event.nativeEvent.layout.width;
        if (Math.abs(nextWidth - panelWidth) > 1) {
          setPanelWidth(nextWidth);
        }
      }}
    >
      <View style={styles.body}>
        <GestureDetector gesture={panGesture}>
          <View style={styles.pagerViewport}>
            <Animated.View style={[styles.pagerTrack, pagerAnimatedStyle]}>
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
            </Animated.View>
          </View>
        </GestureDetector>

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
    pagerViewport: {
      overflow: 'hidden',
    },
    pagerTrack: {
      flexDirection: 'row',
      width: '200%',
    },
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

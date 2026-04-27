import React from 'react';
import {
  Image,
  Platform,
  StyleSheet,
  TouchableWithoutFeedback,
  View,
} from 'react-native';
import { KeyboardStickyView, useReanimatedKeyboardAnimation } from 'react-native-keyboard-controller';
import Reanimated, { useAnimatedStyle } from 'react-native-reanimated';
import { AppText, HostIconButton, SafeBottomView } from '../../../components';
import { exitFullscreen } from '../../../native-bridge';
import { LoadAppCameraPanel } from './LoadAppCameraPanel';
import { useLoadApp } from './useLoadApp';
import { HOST_SHADOWS } from '../../styles/shadows';
import { getTypographyStyle, inputArea, loadApp } from '../../../theme/tokens';
import { useColors } from '../../../theme/useColors';
import type { ColorTokens } from '../../../theme/colors';

const scanAreaSize = 252;
const scanFrameOffset = 4;
const openCameraBtnHeight = 66;
const openCameraBtnFontSize = 18;

const LoadAppPhone = () => {
  const {
    cameraActive,
    setCameraActive,
    setInputExpanded,
    inputRef,
    dismissKeyboard,
    ensureNetworkPermission,
    handleScannedUrl,
    showLoading,
    tabCtx,
    renderInput,
    renderLoadingView,
  } = useLoadApp();

  const colors = useColors();
  const styles = React.useMemo(() => makeStyles(colors), [colors]);
  const { height: keyboardHeight } = useReanimatedKeyboardAnimation();
  const handleExitTab = React.useCallback(async () => {
    try {
      await exitFullscreen();
    } catch (error) {
      console.error('[LoadAppPhone] Failed to exit fullscreen', error);
    }
  }, []);

  const phoneScanSectionAnimatedStyle = useAnimatedStyle(() => {
    if (Platform.OS !== 'ios') {
      return {
        paddingBottom: 0,
      };
    }

    return {
      // 中文注释：输入 Dock 由 StickyView 单独吸附后，内容区也要同步让出键盘高度，
      // 否则扫码区仍按原高度布局，看起来就像“内容没有变化，只是输入框飘起来了”。
      paddingBottom: Math.max(-keyboardHeight.value, 0),
    };
  }, []);

  if (showLoading) {
    return <View style={styles.phoneContainer}>{renderLoadingView()}</View>;
  }

  return (
    <View style={styles.phoneContainer}>
      <TouchableWithoutFeedback onPress={dismissKeyboard} accessible={false}>
        <View style={styles.phoneContent}>
          {/* 非输入框区域：带背景图，底部圆角 */}
          <Reanimated.View
            style={[styles.phoneScanSection, phoneScanSectionAnimatedStyle]}
          >
            <Image
              source={require('../../../assets/load/load_bg.png')}
              style={styles.bgImage}
              resizeMode='stretch'
            />
            <View style={styles.phoneScanArea}>
              <View style={styles.scanContent}>
                <View style={styles.scanFrameWrapper}>
                  <View style={styles.scanBox}>
                    <LoadAppCameraPanel
                      tabId={tabCtx.tab.id}
                      cameraActive={cameraActive}
                      onCameraActiveChange={setCameraActive}
                      onBeforeOpenCamera={() => {
                        setInputExpanded(false);
                        inputRef.current?.blur();
                        dismissKeyboard();
                      }}
                      onAfterOpenCameraPermissionDenied={() => {
                        setCameraActive(false);
                        setInputExpanded(true);
                      }}
                      onAfterCameraPermissionGranted={() => {
                        void ensureNetworkPermission();
                      }}
                      onScanUrl={handleScannedUrl}
                      openCameraBtnHeight={openCameraBtnHeight}
                      openCameraBtnFontSize={openCameraBtnFontSize}
                    />
                  </View>
                  {/* scan.png 叠加在扫码区外层，类似外边框 */}
                  <View style={styles.scanOverlay} pointerEvents='none'>
                    <Image
                      source={require('../../../assets/load/scan.png')}
                      style={styles.scanOverlayImage}
                      resizeMode='contain'
                    />
                  </View>
                </View>

                <View style={styles.scanHintBlock}>
                  <AppText style={styles.scanHintTitle}>
                    请扫码 AskAway 二维码
                  </AppText>
                  <AppText style={styles.scanHintSubtitle}>
                    进入应用
                  </AppText>
                </View>
              </View>
            </View>
          </Reanimated.View>

          {/* 中文注释：Phone 端输入区需要直接跟随键盘吸附，避免仅靠根容器 padding 时被上方 flex 布局“吃掉”位移。 */}
          <KeyboardStickyView
            enabled={Platform.OS === 'ios'}
            style={styles.phoneInputDock}
          >
            <SafeBottomView style={styles.phoneInputSafeArea} minBottom={12}>
              <View style={styles.phoneInputRow}>
                <View style={styles.phoneInputSection}>
                  {renderInput(styles.phoneInput, styles.phoneInputContainer)}
                </View>
                <HostIconButton
                  iconType='tab_preview'
                  size={inputArea.sideButtonSize}
                  iconSize={inputArea.sideButtonIconSize}
                  iconColor={colors.textIconStrong}
                  onPress={() => {
                    inputRef.current?.blur();
                    void handleExitTab();
                  }}
                  style={styles.phoneSendButton}
                />
              </View>
            </SafeBottomView>
          </KeyboardStickyView>
        </View>
      </TouchableWithoutFeedback>
    </View>
  );
};

export default LoadAppPhone;

const makeStyles = (colors: ColorTokens) =>
  StyleSheet.create({
    phoneContainer: {
      flex: 1,
      width: '100%',
      backgroundColor: colors.bgScanArea,
    },
    phoneContent: {
      flex: 1,
      width: '100%',
    },
    phoneScanSection: {
      flex: 1,
      width: '100%',
      overflow: 'hidden',
    },
    bgImage: {
      ...StyleSheet.absoluteFillObject,
      width: '100%',
      height: '100%',
    },
    phoneScanArea: {
      flex: 1,
      width: '100%',
      alignItems: 'center',
      justifyContent: 'center',
    },
    scanContent: {
      alignItems: 'center',
      justifyContent: 'center',
    },
    scanFrameWrapper: {
      width: scanAreaSize,
      height: scanAreaSize,
    },
    scanBox: {
      ...StyleSheet.absoluteFillObject,
      borderRadius: 26,
      overflow: 'hidden',
      // 中文注释：扫码内容层需要盖过 scan.png，避免打开相机按钮被外框图片压住。
      zIndex: 2,
    },
    scanOverlay: {
      position: 'absolute',
      left: -scanFrameOffset,
      top: -scanFrameOffset,
      width: scanAreaSize + scanFrameOffset * 2,
      height: scanAreaSize + scanFrameOffset * 2,
      zIndex: 1,
    },
    scanOverlayImage: {
      width: scanAreaSize + scanFrameOffset * 2,
      height: scanAreaSize + scanFrameOffset * 2,
    },
    scanHintBlock: {
      marginTop: 26,
      alignItems: 'center',
    },
    scanHintTitle: {
      fontSize: 17,
      lineHeight: 22,
      color: colors.textPrimary,
      textAlign: 'center',
    },
    scanHintSubtitle: {
      marginTop: 4,
      fontSize: 20,
      lineHeight: 26,
      color: colors.textPrimary,
      textAlign: 'center',
    },
    phoneInputDock: {
      marginTop: 18,
      width: '100%',
      backgroundColor: colors.bgScanArea,
    },
    phoneInputSafeArea: {
      width: '100%',
      backgroundColor: colors.bgScanArea,
    },
    phoneInputRow: {
      width: '100%',
      flexDirection: 'row',
      alignItems: 'flex-end',
      gap: 10,
      paddingHorizontal: 26,
      paddingTop: 6,
      paddingBottom: 10,
    },
    phoneInputSection: {
      flex: 1,
      alignItems: 'center',
      justifyContent: 'flex-end',
    },
    phoneSendButton: {
      borderWidth: 1,
      borderColor: colors.borderReservedButton,
      backgroundColor: colors.bgInputPill,
      ...HOST_SHADOWS.inputPill,
    },
    phoneInputContainer: {
      position: 'relative',
      width: '100%',
      height: loadApp.inputHeight,
      backgroundColor: colors.bgLoadDark,
      borderRadius: 24,
      ...HOST_SHADOWS.inputPill,
    },
    phoneInput: {
      width: '100%',
      height: loadApp.inputHeight,
      borderRadius: 24,
      paddingLeft: 39,
      paddingRight: 60,
      paddingVertical: 0,
      color: colors.textSecondary,
      ...getTypographyStyle(loadApp.inputTypographyVariant),
    },
  });

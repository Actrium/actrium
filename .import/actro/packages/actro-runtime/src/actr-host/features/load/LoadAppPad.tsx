import React, { useState } from 'react';
import {
  Platform,
  ScrollView,
  StyleSheet,
  TouchableWithoutFeedback,
  View,
} from 'react-native';
import { LoadAppCameraPanel } from './LoadAppCameraPanel';
import { useLoadApp } from './useLoadApp';
import { getHostShadows } from '../../styles/shadows';
import { getTypographyStyle, loadApp } from '../../../theme/tokens';
import { useColors } from '../../../theme/useColors';
import type { ColorTokens } from '../../../theme/colors';

const scanMaxSize = 500;
const padInputMaxWidth = 600;
const padInputBottomOffset = 42;
const containerGap = 36;
const scanBorderRadius = 34;
const inputBorderRadius = 24;

const LoadAppPad = () => {
  const {
    cameraActive,
    cameraDesiredActive,
    setCameraActive,
    dismissKeyboard,
    ensureNetworkPermission,
    handleScannedUrl,
    showLoading,
    keyboardInset,
    tabCtx,
    renderInput,
    renderLoadingView,
  } = useLoadApp();

  const colors = useColors();
  const styles = React.useMemo(() => makeStyles(colors), [colors]);
  const [padContainerWidth, setPadContainerWidth] = useState(0);

  // Android 宿主已经通过 windowSoftInputMode 处理键盘上移，
  // Pad 端不要再额外叠加 RN 的 keyboardInset，否则扫码区和输入框会被重复抬高。
  const effectiveKeyboardInset = Platform.OS === 'android' ? 0 : keyboardInset;
  const padInputOccupiedHeight =
    loadApp.inputHeight + padInputBottomOffset + effectiveKeyboardInset;
  const padScanSize =
    padContainerWidth > 0 ? Math.min(padContainerWidth * 0.9, scanMaxSize) : scanMaxSize;

  if (showLoading) {
    return <View style={styles.padContainer}>{renderLoadingView()}</View>;
  }

  return (
    <TouchableWithoutFeedback onPress={dismissKeyboard} accessible={false}>
      <View
        style={styles.padContainer}
        onLayout={(event) => {
          const { width } = event.nativeEvent.layout;
          if (width !== padContainerWidth) {
            // 扫码区需要显式使用同一个数值作为宽高，
            // 这里读取 Pad 可用宽度后再计算 90% 宽度，避免仅靠 aspectRatio 时被其它布局链路拉成长方形。
            setPadContainerWidth(width);
          }
        }}
      >
        {/* Pad 端扫码区保持主体居中，输入框改为底部独立定位，避免受整体居中布局影响。 */}
        <ScrollView
          contentContainerStyle={[
            styles.scrollContent,
            styles.padScrollContent,
            {
              paddingBottom: padInputOccupiedHeight,
            },
          ]}
          keyboardShouldPersistTaps='handled'
        >
          <View
            style={[
              styles.LoadAppContainer,
              {
                gap: containerGap,
                // Pad 端主体是整块居中内容，键盘弹起后需要把“中心点”整体上移，
                // 仅加底部 padding 不会改变居中块的位置，所以这里按键盘高度的一半上移主体。
                transform: [{ translateY: -(effectiveKeyboardInset / 2) }],
              },
            ]}
          >
            {/* camera scan qrcode */}
            <View
              style={[
                styles.scanArea,
                {
                  width: padScanSize,
                  height: padScanSize,
                  borderRadius: scanBorderRadius,
                },
              ]}
            >
              <LoadAppCameraPanel
                tabId={tabCtx.tab.id}
                cameraDesiredActive={cameraDesiredActive}
                cameraActive={cameraActive}
                onCameraActiveChange={setCameraActive}
                onAfterCameraPermissionGranted={() => {
                  void ensureNetworkPermission();
                }}
                onScanUrl={handleScannedUrl}
              />
            </View>
          </View>
        </ScrollView>

        <View
          style={[
            styles.padInputDock,
            { bottom: padInputBottomOffset + effectiveKeyboardInset },
          ]}
        >
          {renderInput(
            [
              styles.padInput,
              {
                width: '100%',
                borderRadius: inputBorderRadius,
              },
            ],
            [
              styles.padInputContainer,
              {
                width: '90%',
                maxWidth: padInputMaxWidth,
                borderRadius: inputBorderRadius,
              },
            ],
          )}
        </View>
      </View>
    </TouchableWithoutFeedback>
  );
};

export default LoadAppPad;

const makeStyles = (colors: ColorTokens) => {
  const hostShadows = getHostShadows(colors);

  return StyleSheet.create({
    scrollContent: {
      flexGrow: 1,
    },
    padContainer: {
      flex: 1,
      width: '100%',
      backgroundColor: colors.bgScanArea,
    },
    padScrollContent: {
      paddingBottom: 120,
    },
    LoadAppContainer: {
      flex: 1,
      alignItems: 'center',
      justifyContent: 'center',
      flexDirection: 'column',
    },
    scanArea: {
      overflow: 'hidden',
      backgroundColor: colors.bgScanArea,
      ...hostShadows.softSurface,
    },
    padInput: {
      paddingLeft: 39,
      paddingRight: 34,
      height: loadApp.inputHeight,
      paddingVertical: 0,
      color: colors.textSecondary,
      ...getTypographyStyle(loadApp.inputTypographyVariant),
    },
    padInputDock: {
      position: 'absolute',
      left: 0,
      right: 0,
      alignItems: 'center',
    },
    padInputContainer: {
      height: loadApp.inputHeight,
      backgroundColor: colors.bgLoadDark,
      ...hostShadows.inputPill,
    },
  });
};

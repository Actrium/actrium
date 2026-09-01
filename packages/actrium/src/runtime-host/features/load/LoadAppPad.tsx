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

  // Android The host has passed. windowSoftInputMode Process keyboard up,
  // Pad Do not add any extra supersing RN It's... keyboardInset, otherwise the scan and input frames will be raised over and over again.
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
            // The scan area needs to use the same value as the width, and the same value is used for the top.
            // Read here Pad Calculate after the width is available 90% Wide, avoid relying on aspectRatio The squares are being raised by other layout links.
            setPadContainerWidth(width);
          }
        }}
      >
        {/* Pad End sweep area is kept centered, and the input box is then placed independently at the bottom to avoid being influenced by the overall centered layout. */}
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
                // Pad The end subject is the whole centered content, and the keyboard needs to be loaded and put“Centre”The whole thing is moving up.
                // Add Bottom only padding This does not change the position of the center, so you press the keyboard height to move up half the main subject.
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

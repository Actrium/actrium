import React from 'react';
import {
  Animated,
  Image,
  Platform,
  StyleSheet,
  TextStyle,
  TouchableWithoutFeedback,
  View,
} from 'react-native';
import { useTranslation } from 'react-i18next';
import { KeyboardStickyView } from 'react-native-keyboard-controller';
import { AppText, HostIconButton, SafeBottomView } from '../../../components';
import { LoadAppCameraPanel } from './LoadAppCameraPanel';
import { useLoadApp } from './useLoadApp';
import { getHostShadows } from '../../styles/shadows';
import { getTypographyStyle, loadApp } from '../../../theme/tokens';
import { useColors } from '../../../theme/useColors';
import type { ColorTokens } from '../../../theme/colors';

const scanAreaSize = 252;
const scanFrameOffset = 4;
const androidInputTextStyle: TextStyle =
  Platform.OS === 'android' ? { textAlignVertical: 'center', includeFontPadding: false } : {};

const LoadAppPhone = () => {
  const {
    cameraActive,
    cameraDesiredActive,
    setCameraActive,
    inputExpanded,
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
  const { t } = useTranslation();
  const styles = React.useMemo(() => makeStyles(colors), [colors]);
  const scanSectionOpacity = React.useRef(
    new Animated.Value(inputExpanded ? 0 : 1),
  ).current;

  React.useEffect(() => {
    // The sweep area is mounted, only in a transparent way.
    // Avoiding direct unloading leads to a flash of content while keeping the layout stable.
    Animated.timing(scanSectionOpacity, {
      toValue: inputExpanded ? 0 : 1,
      duration: 40,
      useNativeDriver: true,
    }).start();
  }, [inputExpanded, scanSectionOpacity]);

  if (showLoading) {
    return <View style={styles.phoneContainer}>{renderLoadingView()}</View>;
  }

  return (
    <View style={styles.phoneContainer}>
      <TouchableWithoutFeedback onPress={dismissKeyboard} accessible={false}>
        <View style={styles.phoneContent}>
          {/* Non-Input Box Area: with Background Chart, Bottom Round Point */}
          <View style={styles.phoneScanSection}>
            <Animated.View
              style={[styles.phoneScanFadeLayer, { opacity: scanSectionOpacity }]}
              pointerEvents={inputExpanded ? 'none' : 'auto'}
            >
              <>
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
                          cameraDesiredActive={cameraDesiredActive}
                          cameraActive={cameraActive}
                          onCameraActiveChange={setCameraActive}
                          onAfterOpenCameraPermissionDenied={() => {
                            setCameraActive(false);
                            setInputExpanded(true);
                          }}
                          onAfterCameraPermissionGranted={() => {
                            void ensureNetworkPermission();
                          }}
                          onScanUrl={handleScannedUrl}
                        />
                      </View>
                      {/* scan.png Superimpose it on the outer level of the sweep, like the outer frame. */}
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
                        {t('actr.app.scanHint')}
                      </AppText>
                    </View>
                  </View>
                </View>
              </>
            </Animated.View>
          </View>

          {/* Comment in Chinese:Phone End input area needs to follow the keyboard directly to adsorb, avoiding root containers only padding Time above flex Layout“Eat.”Offset. */}
          <KeyboardStickyView
            enabled={Platform.OS === 'ios'}
            style={styles.phoneInputDock}
          >
            <SafeBottomView style={styles.phoneInputSafeArea} minBottom={12}>
              <View
                style={[
                  styles.phoneInputRow,
                  inputExpanded ? styles.phoneInputRowExpanded : styles.phoneInputRowCollapsed,
                ]}
              >
                {inputExpanded ? (
                  <>
                    <View style={styles.phoneInputSection}>
                      {renderInput(styles.phoneInput, styles.phoneInputContainer, {
                        showPrefixIcon: false,
                      })}
                    </View>
                    <HostIconButton
                      iconType='close'
                      size={loadApp.inputHeight}
                      iconSize={16}
                      iconColor={colors.textIconStrong}
                      onPress={() => {
                        // The government has also been able to reduce the number of people in the country, and the number of people in the country is still high.
                        // Promise. Phone The end returns the default button and restores the top sweep area content.
                        inputRef.current?.blur();
                        setInputExpanded(false);
                      }}
                      style={styles.phoneActionButton}
                    />
                  </>
                ) : (
                  <HostIconButton
                    iconType='keyboard'
                    size={loadApp.inputHeight}
                    iconSize={22}
                    iconColor={colors.textIconStrong}
                    onPress={() => {
                      setInputExpanded(true);
                    }}
                    style={styles.phoneActionButton}
                  />
                )}
              </View>
            </SafeBottomView>
          </KeyboardStickyView>
        </View>
      </TouchableWithoutFeedback>
    </View>
  );
};

export default LoadAppPhone;

const makeStyles = (colors: ColorTokens) => {
  const hostShadows = getHostShadows(colors);

  return StyleSheet.create({
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
    phoneScanFadeLayer: {
      flex: 1,
      width: '100%',
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
      // Sweeping the content layer needs to be covered. scan.png, avoid opening the camera button to be pressed by frame pictures.
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
      minHeight: loadApp.inputHeight + 16,
      paddingHorizontal: 10,
      paddingTop: 6,
      paddingBottom: 10,
    },
    phoneInputRowExpanded: {
      flexDirection: 'row',
      alignItems: 'flex-end',
      gap: 10,
    },
    phoneInputRowCollapsed: {
      alignItems: 'center',
      justifyContent: 'flex-end',
    },
    phoneInputSection: {
      flex: 1,
      alignItems: 'center',
      justifyContent: 'flex-end',
    },
    phoneActionButton: {
      borderWidth: 1,
      borderColor: colors.borderReservedButton,
      backgroundColor: colors.bgInputPill,
      ...hostShadows.inputPill,
    },
    phoneInputContainer: {
      position: 'relative',
      width: '100%',
      height: loadApp.inputHeight,
      backgroundColor: colors.bgLoadDark,
      borderRadius: 24,
      ...hostShadows.inputPill,
    },
    phoneInput: {
      width: '100%',
      height: loadApp.inputHeight,
      borderRadius: 24,
      paddingLeft: 16,
      paddingRight: 60,
      paddingVertical: 0,
      color: '#000000',
      textAlignVertical: 'center',
      ...androidInputTextStyle,
      ...getTypographyStyle(loadApp.inputTypographyVariant),
    },
  });
};

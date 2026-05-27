import React, { useState, useRef } from 'react';
import {
  View,
  TouchableOpacity,
  StyleSheet,
  TouchableWithoutFeedback,
  Modal,
  Linking,
  Platform,
  PermissionsAndroid,
} from 'react-native';
import MapView, { Region } from 'react-native-maps';
import { useTranslation } from 'react-i18next';
import ViewShot from 'react-native-view-shot';
import { Icon } from '../../../components';
import type Geolocation from '@react-native-community/geolocation';
import { GlobalAlert } from '../../../components';
import { AppText } from '../../../components';
import { getTypographyStyle, spacing } from '../../../theme/tokens';
import { getHostShadows } from '../../styles/shadows';
import { useColors } from '../../../theme/useColors';
import type { ColorTokens } from '../../../theme/colors';

interface LocationPickerProps {
  visible: boolean;
  onClose: () => void;
  // 中文注释：根据调用场景切换地图选择器的弹层锚点，Phone 端贴底展示，Pad 端保持居中。
  presentation?: 'center' | 'bottom';
  onSelect: (data: {
    latitude: number;
    longitude: number;
    snapshot: string;
  }) => void;
}

const LocationPicker: React.FC<LocationPickerProps> = ({
  visible,
  onClose,
  presentation = 'center',
  onSelect,
}) => {
  const { t } = useTranslation();
  const colors = useColors();
  const styles = React.useMemo(() => makeStyles(colors), [colors]);
  const [mapRegion, setMapRegion] = useState<Region | null>(null);
  const mapShotRef = useRef<ViewShot>(null);
  const currentRegionRef = useRef<Region | null>(null);
  const getGeolocation = (): typeof Geolocation | null => {
    try {
      return require('@react-native-community/geolocation').default as typeof Geolocation;
    } catch (error) {
      console.error('[LocationPicker] Geolocation native module unavailable:', error);
      return null;
    }
  };

  // 当弹窗打开时，尝试获取当前位置
  React.useEffect(() => {
    if (visible) {
      // 中文注释：每次重新打开弹窗都重置当前实例的地图状态，确保不同 tab 的新实例不会沿用旧 region。
      setMapRegion(null);
      currentRegionRef.current = null;
      if (Platform.OS === 'android') {
        handleAndroidLocation();
      } else {
        handleInitialLocation();
      }
    } else {
      setMapRegion(null);
      currentRegionRef.current = null;
    }
  }, [visible]);

  // Android: 直接申请权限获取经纬度，不显示地图
  const handleAndroidLocation = async () => {
    try {
      const geolocation = getGeolocation();
      if (!geolocation) {
        GlobalAlert.alert(
          t('common.error', '错误'),
          t('actr.input.locationFailed', '无法获取当前位置'),
        );
        onClose();
        return;
      }

      const granted = await PermissionsAndroid.request(
        PermissionsAndroid.PERMISSIONS.ACCESS_FINE_LOCATION,
        {
          title: t('actr.input.locationPermissionTitle'),
          message: t('actr.input.locationPermissionMsg'),
          buttonNeutral: t('common.later', '稍后'),
          buttonNegative: t('common.cancel', '取消'),
          buttonPositive: t('common.confirm', '确定'),
        },
      );

      if (granted !== PermissionsAndroid.RESULTS.GRANTED) {
        GlobalAlert.alert(
          t('actr.input.locationPermissionTitle'),
          t('actr.input.locationPermissionMsg'),
          [
            {
              text: t('common.cancel'),
              style: 'cancel',
              onPress: () => onClose(),
            },
            {
              text: t('actr.voice.goToSettings'),
              onPress: () => {
                Linking.openSettings();
                onClose();
              },
            },
          ],
        );
        return;
      }

      geolocation.getCurrentPosition(
        (position) => {
          onSelect({
            latitude: position.coords.latitude,
            longitude: position.coords.longitude,
            snapshot: '',
          });
        },
        (error) => {
          console.warn('[Geolocation] Android Error:', error);
          GlobalAlert.alert(
            t('common.error', '错误'),
            t('actr.input.locationFailed', '无法获取当前位置'),
          );
          onClose();
        },
        {
          enableHighAccuracy: false,
          timeout: 10000,
          maximumAge: 5000,
        },
      );
    } catch (err) {
      console.warn('[Geolocation] Android permission error:', err);
      onClose();
    }
  };

  const handleInitialLocation = async () => {
    const geolocation = getGeolocation();
    if (!geolocation) {
      GlobalAlert.alert(
        t('common.error', '错误'),
        t('actr.input.locationFailed', '无法获取当前位置'),
      );
      onClose();
      return;
    }

    // 中文注释：iOS 已授权后再次调用 requestAuthorization 不保证继续回调；
    // 这里不能 await 它，否则新 tab 的首次打开可能一直卡住，后续 getCurrentPosition 根本不会执行。
    geolocation.requestAuthorization(
      () => {},
      () => {},
    );

    geolocation.getCurrentPosition(
      (position) => {
        const region = {
          latitude: position.coords.latitude,
          longitude: position.coords.longitude,
          latitudeDelta: 0.005,
          longitudeDelta: 0.005,
        };
        setMapRegion(region);
        currentRegionRef.current = region;
      },
      (error) => {
        console.warn('[Geolocation] Error:', error);
        if ((error as { code?: number }).code === 1) {
          GlobalAlert.alert(
            t('actr.input.locationPermissionTitle'),
            t('actr.input.locationPermissionMsg'),
            [
              {
                text: t('common.cancel'),
                style: 'cancel',
                onPress: () => onClose(),
              },
              {
                text: t('actr.voice.goToSettings'),
                onPress: () => {
                  Linking.openSettings();
                  onClose();
                },
              },
            ],
          );
          return;
        }

        const fallbackRegion = {
          latitude: 39.9042,
          longitude: 116.4074,
          latitudeDelta: 0.01,
          longitudeDelta: 0.01,
        };
        setMapRegion(fallbackRegion);
        currentRegionRef.current = fallbackRegion;
      },
      { enableHighAccuracy: true, timeout: 8000, maximumAge: 5000 },
    );
  };

  const handleConfirm = async () => {
    if (!mapShotRef.current) return;

    const selectedRegion = currentRegionRef.current || mapRegion;
    if (!selectedRegion) return;

    try {
      // 直接截图整个地图容器，让地图和悬浮图钉一起进入快照，避免原生 Marker 干扰底图标识

      const snapshot = await mapShotRef.current.capture?.();
      if (!snapshot) {
        throw new Error('Location view snapshot is empty');
      }

      const snapshotUri = snapshot.startsWith('file://')
        ? snapshot
        : `file://${snapshot}`;
      console.log('[LocationPicker] Snapshot with Region URI:', snapshotUri);

      onSelect({
        latitude: selectedRegion.latitude,
        longitude: selectedRegion.longitude,
        snapshot: snapshotUri,
      });
    } catch (error) {
      console.error('Failed to take snapshot:', error);
      GlobalAlert.alert(t('common.error'), t('actr.chat.snapshotFailed') || '无法生成地图快照');
    }
  };

  // Android 不渲染地图弹窗
  if (Platform.OS === 'android') {
    return null;
  }

  // iOS: 原有地图选点 UI
  return (
    <Modal
      visible={visible}
      animationType='fade'
      transparent={true}
      onRequestClose={onClose}
    >
      <TouchableWithoutFeedback onPress={onClose}>
        <View style={styles.modalOverlay}>
          <TouchableWithoutFeedback>
            <View
              style={[
                styles.mapModalContent,
                // 中文注释：Phone 端改成底部抽屉式承载地图，Pad 端继续使用居中卡片。
                presentation === 'bottom'
                  ? styles.mapModalContentBottom
                  : styles.mapModalContentCenter,
              ]}
            >
              <ViewShot
                ref={mapShotRef}
                style={styles.mapContainer}
                options={{
                  format: 'png',
                  quality: 1,
                  result: 'tmpfile',
                }}
              >
                {mapRegion && (
                  <>
                    <MapView
                      style={styles.map}
                      mapType='standard'
                      initialRegion={mapRegion}
                      onRegionChange={(r) => {
                        // 记录当前地图中心点，确认时按这个中心点输出经纬度
                        currentRegionRef.current = r;
                      }}
                      onRegionChangeComplete={(r) => {
                        currentRegionRef.current = r;
                      }}
                    />
                    {/* 保持屏幕中心悬浮图钉，交互仍然是“拖地图选中心点” */}
                    <View
                      style={styles.centerMarkerContainer}
                      pointerEvents='none'
                    >
                      <View style={styles.centerMarker}>
                        <Icon type='location_pin' size={84} disableTint />
                      </View>
                    </View>
                  </>
                )}

                <View style={styles.mapActionOverlay}>
                  <TouchableOpacity
                    style={styles.confirmButton}
                    onPress={handleConfirm}
                  >
                    <AppText style={styles.confirmButtonText}>{t('common.confirm')}</AppText>
                  </TouchableOpacity>
                </View>
              </ViewShot>
            </View>
          </TouchableWithoutFeedback>
        </View>
      </TouchableWithoutFeedback>
    </Modal>
  );
};

const makeStyles = (colors: ColorTokens) => {
  const hostShadows = getHostShadows(colors);

  return StyleSheet.create({
    modalOverlay: {
      flex: 1,
      backgroundColor: colors.bgOverlay,
      justifyContent: 'center',
      alignItems: 'center',
    },
    mapModalContent: {
      height: 400,
      backgroundColor: colors.bgSurface,
      overflow: 'hidden',
      ...hostShadows.mapSurface,
    },
    mapModalContentCenter: {
      width: 400,
      borderRadius: 34,
    },
    mapModalContentBottom: {
      width: '100%',
      alignSelf: 'stretch',
      marginTop: 'auto',
      borderTopLeftRadius: 34,
      borderTopRightRadius: 34,
      borderBottomLeftRadius: 0,
      borderBottomRightRadius: 0,
    },
    mapContainer: {
      flex: 1,
    },
    map: {
      ...StyleSheet.absoluteFillObject,
    },
    centerMarkerContainer: {
      ...StyleSheet.absoluteFillObject,
      justifyContent: 'center',
      alignItems: 'center',
      // 确保图标在地图正中心，稍微往上偏一点点符合视觉习惯
    },
    centerMarker: {
      marginTop: -24,
    },
    mapActionOverlay: {
      position: 'absolute',
      bottom: spacing.xxl,
      left: 0,
      right: 0,
      alignItems: 'center',
      justifyContent: 'center',
    },
    confirmButton: {
      backgroundColor: colors.accentBlue,
      width: 140,
      height: 48,
      borderRadius: 34,
      justifyContent: 'center',
      alignItems: 'center',
      ...hostShadows.mapActionButton,
    },
    confirmButtonText: {
      color: colors.textInverse,
      ...getTypographyStyle('bodyInput'),
      fontWeight: 'bold',
    },
  });
};

export default LocationPicker;

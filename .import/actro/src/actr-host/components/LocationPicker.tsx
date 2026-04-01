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
import { Icon } from './Icon';
import type Geolocation from '@react-native-community/geolocation';
import { GlobalAlert } from '../../../src/components';
import { AppText } from '../../components';
import { getTypographyStyle, spacing } from '../../theme/tokens';
import { HOST_SHADOWS } from '../styles/shadows';

interface LocationPickerProps {
  visible: boolean;
  onClose: () => void;
  onSelect: (data: {
    latitude: number;
    longitude: number;
    snapshot: string;
  }) => void;
}

const LocationPicker: React.FC<LocationPickerProps> = ({
  visible,
  onClose,
  onSelect,
}) => {
  const { t } = useTranslation();
  const [mapRegion, setMapRegion] = useState<Region | null>(null);
  const mapRef = useRef<MapView>(null);
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
      if (Platform.OS === 'android') {
        handleAndroidLocation();
      } else {
        handleInitialLocation();
      }
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

    const status = await new Promise<'granted' | 'denied'>((resolve) => {
      geolocation.requestAuthorization(
        () => resolve('granted'),
        () => resolve('denied'),
      );
    });
    if (status !== 'granted') {
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
    if (!mapRef.current) return;

    const selectedRegion = currentRegionRef.current || mapRegion;
    if (!selectedRegion) return;

    try {
      const snapshot = await mapRef.current.takeSnapshot({
        // 🌟 关键：显式传递 region 强制快照器定位到正确区域
        region: {
          latitude: selectedRegion.latitude,
          longitude: selectedRegion.longitude,
          latitudeDelta: selectedRegion.latitudeDelta || 0.005,
          longitudeDelta: selectedRegion.longitudeDelta || 0.005,
        },
        format: 'png',
        quality: 1,
        result: 'file',
      });

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
            <View style={styles.mapModalContent}>
              <View style={styles.mapContainer}>
                {mapRegion && (
                  <>
                    <MapView
                      ref={mapRef}
                      style={styles.map}
                      mapType='standard'
                      initialRegion={mapRegion}
                      onRegionChangeComplete={(r) => {
                        currentRegionRef.current = r;
                      }}
                    />
                    {/* 使用悬浮中心点代替 Marker，确保在快照时 100% 可见且不干扰地图渲染 */}
                    <View
                      style={styles.centerMarkerContainer}
                      pointerEvents='none'
                    >
                      <View style={styles.centerMarker}>
                        <Icon type='geolocation' size={24} color='#34A8FB' />
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
              </View>
            </View>
          </TouchableWithoutFeedback>
        </View>
      </TouchableWithoutFeedback>
    </Modal>
  );
};

const styles = StyleSheet.create({
  modalOverlay: {
    flex: 1,
    backgroundColor: 'rgba(0, 0, 0, 0.4)',
    justifyContent: 'center',
    alignItems: 'center',
  },
  mapModalContent: {
    width: 400,
    height: 400,
    backgroundColor: '#fff',
    borderRadius: 34,
    overflow: 'hidden',
    ...HOST_SHADOWS.mapSurface,
  },
  mapContainer: {
    flex: 1,
  },
  map: {
    ...StyleSheet.absoluteFillObject,
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
    backgroundColor: '#34A8FB',
    width: 140,
    height: 48,
    borderRadius: 34,
    justifyContent: 'center',
    alignItems: 'center',
    ...HOST_SHADOWS.mapActionButton,
  },
  confirmButtonText: {
    color: '#fff',
    ...getTypographyStyle('bodyInput'),
    fontWeight: 'bold',
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
});

export default LocationPicker;

import React, { useState, useRef } from 'react';
import {
  View,
  Text,
  TouchableOpacity,
  StyleSheet,
  TouchableWithoutFeedback,
  Modal,
  Alert,
  Linking,
} from 'react-native';
import MapView, { Marker, Region } from 'react-native-maps';
import Geolocation from 'react-native-geolocation-service';
import { useTranslation } from 'react-i18next';
import { Icon } from './Icon';

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

  // 当弹窗打开时，尝试获取当前位置
  React.useEffect(() => {
    if (visible) {
      handleInitialLocation();
    }
  }, [visible]);

  const handleInitialLocation = async () => {
    const status = await Geolocation.requestAuthorization('whenInUse');
    if (status !== 'granted') {
      Alert.alert(
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

    Geolocation.getCurrentPosition(
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
      Alert.alert(t('common.error'), t('actr.chat.snapshotFailed') || '无法生成地图快照');
    }
  };

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
                    <Text style={styles.confirmButtonText}>{t('common.confirm')}</Text>
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
    elevation: 5,
    shadowColor: '#000',
    shadowOffset: { width: 0, height: 2 },
    shadowOpacity: 0.25,
    shadowRadius: 10,
  },
  mapContainer: {
    flex: 1,
  },
  map: {
    ...StyleSheet.absoluteFillObject,
  },
  mapActionOverlay: {
    position: 'absolute',
    bottom: 24,
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
    shadowColor: '#000',
    shadowOffset: { width: 0, height: 2 },
    shadowOpacity: 0.2,
    shadowRadius: 4,
  },
  confirmButtonText: {
    color: '#fff',
    fontSize: 16,
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

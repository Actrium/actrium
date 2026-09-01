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
  // The bombbed anchor of the map selector is based on the call scene to switch the map selection.Phone The bottom of the endpost is displayed.Pad Ends to centre.
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

  // Try to get the current position when the bullet window opens
  React.useEffect(() => {
    if (visible) {
      // Every time a bullet window is reopened, the current example is reset to make sure that it is different. tab New examples don't follow old ones. region.
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

  // Android: Direct application permission to obtain longitude without displaying maps
  const handleAndroidLocation = async () => {
    try {
      const geolocation = getGeolocation();
      if (!geolocation) {
        GlobalAlert.alert(
          t('common.error', 'Error'),
          t('actr.input.locationFailed', 'Could not close temporary folder: %s'),
        );
        onClose();
        return;
      }

      const granted = await PermissionsAndroid.request(
        PermissionsAndroid.PERMISSIONS.ACCESS_FINE_LOCATION,
        {
          title: t('actr.input.locationPermissionTitle'),
          message: t('actr.input.locationPermissionMsg'),
          buttonNeutral: t('common.later', 'Later.'),
          buttonNegative: t('common.cancel', 'Cancel'),
          buttonPositive: t('common.confirm', 'Sure.'),
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
            t('common.error', 'Error'),
            t('actr.input.locationFailed', 'Could not close temporary folder: %s'),
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
        t('common.error', 'Error'),
        t('actr.input.locationFailed', 'Could not close temporary folder: %s'),
      );
      onClose();
      return;
    }

    // iOS Call again after authorized requestAuthorization No guarantees of continued recall;
    // Not here. await It's new. tab The first opening could be stuck, after. getCurrentPosition It's not going to be implemented at all.
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
      // Directly screen the entire map container, and get the map and the suspended pedals together into the snapshot, avoiding the original. Marker Interference bottom map identification

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
      GlobalAlert.alert(t('common.error'), t('actr.chat.snapshotFailed') || 'Could not generate map snapshot');
    }
  };

  // Android Unrender map bullet window
  if (Platform.OS === 'android') {
    return null;
  }

  // iOS: Previous Map Selected Points UI
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
                // Phone The end is changed to a bottom drawer carrying map.Pad End continues to use center cards.
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
                        // Record the current map centre point and, when confirmed, output longitudes at this centre point
                        currentRegionRef.current = r;
                      }}
                      onRegionChangeComplete={(r) => {
                        currentRegionRef.current = r;
                      }}
                    />
                    {/* Keep the strut on the screen center. The interaction is still.“Drag Map Central” */}
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
      // Make sure the icon is in the center of the map, and it's a little more up to fit the visual habits.
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

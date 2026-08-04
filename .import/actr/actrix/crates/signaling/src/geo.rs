//! Geographic location and distance calculation utilities
//!
//! Provides the Haversine formula to compute the great-circle distance between two points on Earth

use std::f64::consts::PI;

/// Earth radius (kilometers)
const EARTH_RADIUS_KM: f64 = 6371.0;

/// Compute the distance between two geographic coordinates using the Haversine formula
///
/// # Arguments
/// * `lat1` - Point 1 latitude (degrees)
/// * `lon1` - Point 1 longitude (degrees)
/// * `lat2` - Point 2 latitude (degrees)
/// * `lon2` - Point 2 longitude (degrees)
///
/// # Returns
/// Great-circle distance between the two points (kilometers)
///
/// # Example
/// ```
/// use signaling::geo::haversine_distance;
///
/// // Distance from Beijing to Shanghai
/// let beijing = (39.9042, 116.4074);
/// let shanghai = (31.2304, 121.4737);
/// let distance = haversine_distance(beijing.0, beijing.1, shanghai.0, shanghai.1);
/// assert!((distance - 1067.0).abs() < 10.0); // approximately 1067 km
/// ```
pub fn haversine_distance(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    // Convert to radians
    let lat1_rad = lat1 * PI / 180.0;
    let lat2_rad = lat2 * PI / 180.0;
    let delta_lat = (lat2 - lat1) * PI / 180.0;
    let delta_lon = (lon2 - lon1) * PI / 180.0;

    // Haversine formula
    let a = (delta_lat / 2.0).sin().powi(2)
        + lat1_rad.cos() * lat2_rad.cos() * (delta_lon / 2.0).sin().powi(2);
    let c = 2.0 * a.sqrt().atan2((1.0 - a).sqrt());

    EARTH_RADIUS_KM * c
}

/// Geographic coordinate point
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GeoPoint {
    pub latitude: f64,
    pub longitude: f64,
}

impl GeoPoint {
    /// Create a new geographic coordinate point
    pub fn new(latitude: f64, longitude: f64) -> Self {
        Self {
            latitude,
            longitude,
        }
    }

    /// Compute distance to another point (kilometers)
    pub fn distance_to(&self, other: &GeoPoint) -> f64 {
        haversine_distance(
            self.latitude,
            self.longitude,
            other.latitude,
            other.longitude,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_haversine_same_point() {
        let distance = haversine_distance(39.9042, 116.4074, 39.9042, 116.4074);
        assert!(distance.abs() < 0.001);
    }

    #[test]
    fn test_haversine_beijing_shanghai() {
        // Beijing: 39.9042°N, 116.4074°E
        // Shanghai: 31.2304°N, 121.4737°E
        // Actual distance approximately 1067 km
        let distance = haversine_distance(39.9042, 116.4074, 31.2304, 121.4737);
        assert!((distance - 1067.0).abs() < 20.0); // allow 20km error
    }

    #[test]
    fn test_haversine_new_york_london() {
        // New York: 40.7128°N, 74.0060°W
        // London: 51.5074°N, 0.1278°W
        // Actual distance approximately 5570 km
        let distance = haversine_distance(40.7128, -74.0060, 51.5074, -0.1278);
        assert!((distance - 5570.0).abs() < 50.0); // allow 50km error
    }

    #[test]
    fn test_geopoint_distance() {
        let beijing = GeoPoint::new(39.9042, 116.4074);
        let shanghai = GeoPoint::new(31.2304, 121.4737);
        let distance = beijing.distance_to(&shanghai);
        assert!((distance - 1067.0).abs() < 20.0);
    }

    #[test]
    fn test_geopoint_equality() {
        let p1 = GeoPoint::new(39.9042, 116.4074);
        let p2 = GeoPoint::new(39.9042, 116.4074);
        assert_eq!(p1, p2);
    }
}

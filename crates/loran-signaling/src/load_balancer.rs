//! Load balancing module
//!
//! Implements service instance ranking and selection algorithm based on multiple factors
//!
//! # Supported ranking factors
//! - `MAXIMUM_POWER_RESERVE`: descending by remaining capacity (prefer lighter load)
//! - `MINIMUM_MAILBOX_BACKLOG`: ascending by message backlog (prefer smaller backlog)
//! - `BEST_COMPATIBILITY`: exact fingerprint match first (ExactMatchFirst strategy)
//! - `NEAREST`: by geographic distance (based on Haversine formula)
//! - `CLIENT_AFFINITY`: by client affinity (session stickiness)
//!
//! # Usage example
//! ```ignore
//! use signaling::load_balancer::LoadBalancer;
//! use signaling::actor_registry::ServiceInfo;
//! use actr_protocol::route_candidates_request::node_selection_criteria::NodeRankingFactor;
//!
//! let mut candidates: Vec<ServiceInfo> = vec![]; // get candidate list
//! let criteria = Some(&NodeSelectionCriteria {
//!     candidate_count: 3,
//!     ranking_factors: vec![
//!         NodeRankingFactor::MaximumPowerReserve as i32,
//!         NodeRankingFactor::MinimumMailboxBacklog as i32,
//!     ],
//!     minimal_health_requirement: None,
//!     minimal_dependency_requirement: None,
//! });
//!
//! let ranked = LoadBalancer::rank_candidates(candidates, criteria, "", None, None);
//! // Returns a ranked list of candidate ActrIds
//! ```

use crate::actor_registry::ServiceInfo;
use actr_protocol::{
    ActrId, ServiceAvailabilityState, ServiceDependencyState,
    route_candidates_request::{NodeSelectionCriteria, node_selection_criteria::NodeRankingFactor},
};

/// Load balancer
pub struct LoadBalancer;

impl LoadBalancer {
    /// Ranks candidate services according to selection criteria
    ///
    /// Precondition: candidates have already passed ACL filtering (realm access control)
    /// and all match the requester's ActrType.
    ///
    /// # Parameters
    /// - `candidates`: list of candidate services (already ACL-filtered)
    /// - `criteria`: node selection criteria (ranking factors, health/dependency requirements)
    /// - `client_fingerprint`: client fingerprint (used for EXACT_MATCH_FIRST factor marking)
    /// - `client_id`: client ID (used for CLIENT_AFFINITY)
    /// - `client_location`: client geographic coordinates (used for NEAREST)
    ///
    /// # Returns
    /// Ranked list of ActrIds (at most candidate_count returned)
    ///
    /// # Implementation logic
    /// 1. Apply health and dependency filtering
    /// 2. Sort by ranking factors in order
    /// 3. Return the top N candidates
    pub fn rank_candidates(
        candidates: &[ServiceInfo],
        criteria: Option<&NodeSelectionCriteria>,
        client_fingerprint: &str,
        client_id: Option<&str>,
        client_location: Option<(f64, f64)>,
    ) -> Vec<ActrId> {
        if candidates.is_empty() {
            return Vec::new();
        }

        // If no criteria specified, return all candidates
        let criteria = match criteria {
            Some(c) => c,
            None => {
                platform::recording::info!(
                    "No selection criteria specified, returning all candidates"
                );
                return candidates.iter().map(|s| s.actor_id.clone()).collect();
            }
        };

        let mut candidates: Vec<ServiceInfo> = candidates.to_vec();

        // Mark is_exact_match: fingerprint comparison done inside the LB
        if !client_fingerprint.is_empty() {
            for c in &mut candidates {
                let fp = c
                    .service_spec
                    .as_ref()
                    .map(|s| s.fingerprint.as_str())
                    .unwrap_or("");
                c.is_exact_match = fp == client_fingerprint;
            }
        }

        platform::recording::info!(
            "Load balancing rank: candidate_count={}, ranking_factor_count={}",
            candidates.len(),
            criteria.ranking_factors.len()
        );

        // 1. Apply health requirement filtering
        if let Some(min_health) = criteria.minimal_health_requirement {
            candidates = Self::filter_by_health(&candidates, min_health);
            platform::recording::debug!("Remaining after health filter: {}", candidates.len());
        }

        // 2. Apply dependency requirement filtering
        if let Some(min_dependency) = criteria.minimal_dependency_requirement {
            candidates = Self::filter_by_dependency(&candidates, min_dependency);
            platform::recording::debug!("Remaining after dependency filter: {}", candidates.len());
        }

        if candidates.is_empty() {
            platform::recording::warn!("No available candidates after filtering");
            return Vec::new();
        }

        // 3. If ExactMatchFirst is enabled and non-exact-match candidates exist, log a warning
        if !client_fingerprint.is_empty() && candidates.iter().any(|c| !c.is_exact_match) {
            platform::recording::warn!(
                "Routing result includes non-exact-match candidates (different fingerprint but compatible ActrType)"
            );
        }

        // 4. Multi-key sort: compare by factor list order; the first unequal factor decides the result
        //    Subsequent factors only take effect when all preceding factors are equal (true priority ordering)
        let factors: Vec<NodeRankingFactor> = criteria
            .ranking_factors
            .iter()
            .filter_map(|&f| NodeRankingFactor::try_from(f).ok())
            .collect();

        if !factors.is_empty() {
            candidates.sort_by(|a, b| {
                for factor in &factors {
                    let ord = Self::cmp_by_factor(a, b, factor, client_id, client_location);
                    if ord != std::cmp::Ordering::Equal {
                        return ord;
                    }
                }
                std::cmp::Ordering::Equal
            });
        }

        // 5. Return the top N candidates
        let limit = criteria.candidate_count as usize;
        candidates
            .into_iter()
            .take(limit)
            .map(|s| s.actor_id)
            .collect()
    }

    /// Filter by health requirement
    ///
    /// Health state priority order: FULL > DEGRADED > None(unknown) > OVERLOADED > UNAVAILABLE
    /// Filters out all candidates below the min_health requirement
    fn filter_by_health(candidates: &[ServiceInfo], min_health: i32) -> Vec<ServiceInfo> {
        platform::recording::debug!(
            "Applying health filter: min_health={}",
            ServiceAvailabilityState::try_from(min_health)
                .map(|s| format!("{s:?}"))
                .unwrap_or_else(|_| "Invalid".to_string())
        );

        let mut filtered: Vec<ServiceInfo> = candidates
            .iter()
            .filter(|s| {
                match s.service_availability_state {
                    Some(service_availability_state) => {
                        // Lower value means healthier (FULL=0, DEGRADED=1, OVERLOADED=2, UNAVAILABLE=3)
                        service_availability_state <= min_health
                    }
                    None => {
                        // None is treated as sub-healthy (between DEGRADED and OVERLOADED)
                        // If FULL or DEGRADED is required, None passes
                        // If OVERLOADED or UNAVAILABLE is required, None also passes
                        min_health >= ServiceAvailabilityState::Degraded as i32
                    }
                }
            })
            .cloned()
            .collect();

        platform::recording::debug!(
            "After health filter: {} -> {} candidates",
            candidates.len(),
            filtered.len()
        );

        // Sort by health state: FULL(0) > DEGRADED(1) > None(treated as 1.5) > OVERLOADED(2) > UNAVAILABLE(3)
        filtered.sort_by(|a, b| {
            let a_health = a.service_availability_state.unwrap_or(2); // None treated as between DEGRADED and OVERLOADED
            let b_health = b.service_availability_state.unwrap_or(2);
            a_health.cmp(&b_health)
        });

        filtered
    }

    /// Filter by dependency requirement
    ///
    /// Dependency state priority order: HEALTHY > WARNING > None(unknown) > BROKEN
    /// Filters out all candidates below the min_dependency requirement
    fn filter_by_dependency(candidates: &[ServiceInfo], min_dependency: i32) -> Vec<ServiceInfo> {
        platform::recording::debug!(
            "Applying dependency filter: min_dependency={}",
            ServiceDependencyState::try_from(min_dependency)
                .map(|s| format!("{s:?}"))
                .unwrap_or_else(|_| "Invalid".to_string())
        );

        let mut filtered: Vec<ServiceInfo> = candidates
            .iter()
            .filter(|s| {
                match s.worst_dependency_health_state {
                    Some(worst_dependency_health_state) => {
                        // Lower value means healthier dependency (HEALTHY=0, WARNING=1, BROKEN=2)
                        worst_dependency_health_state <= min_dependency
                    }
                    None => {
                        // None is treated as warning state (between WARNING and BROKEN)
                        min_dependency >= ServiceDependencyState::Warning as i32
                    }
                }
            })
            .cloned()
            .collect();

        platform::recording::debug!(
            "After dependency filter: {} -> {} candidates",
            candidates.len(),
            filtered.len()
        );

        // Sort by dependency health state: HEALTHY(0) > WARNING(1) > None(treated as 1.5) > BROKEN(2)
        filtered.sort_by(|a, b| {
            let a_dep = a.worst_dependency_health_state.unwrap_or(2); // None treated as between WARNING and BROKEN
            let b_dep = b.worst_dependency_health_state.unwrap_or(2);
            a_dep.cmp(&b_dep)
        });

        filtered
    }

    /// Compare two candidates by a single factor (for multi-key sorting)
    fn cmp_by_factor(
        a: &ServiceInfo,
        b: &ServiceInfo,
        factor: &NodeRankingFactor,
        client_id: Option<&str>,
        client_location: Option<(f64, f64)>,
    ) -> std::cmp::Ordering {
        match factor {
            NodeRankingFactor::MaximumPowerReserve => Self::cmp_power_reserve(a, b),
            NodeRankingFactor::MinimumMailboxBacklog => Self::cmp_mailbox_backlog(a, b),
            NodeRankingFactor::ExactMatchFirst => Self::cmp_exact_match(a, b),
            NodeRankingFactor::Nearest => Self::cmp_distance(a, b, client_location),
            NodeRankingFactor::ClientAffinity => Self::cmp_affinity(a, b, client_id),
        }
    }

    fn cmp_power_reserve(a: &ServiceInfo, b: &ServiceInfo) -> std::cmp::Ordering {
        b.power_reserve
            .unwrap_or(0.0)
            .partial_cmp(&a.power_reserve.unwrap_or(0.0))
            .unwrap_or(std::cmp::Ordering::Equal)
    }

    fn cmp_mailbox_backlog(a: &ServiceInfo, b: &ServiceInfo) -> std::cmp::Ordering {
        a.mailbox_backlog
            .unwrap_or(1.0)
            .partial_cmp(&b.mailbox_backlog.unwrap_or(1.0))
            .unwrap_or(std::cmp::Ordering::Equal)
    }

    fn cmp_exact_match(a: &ServiceInfo, b: &ServiceInfo) -> std::cmp::Ordering {
        // Exact matches rank first (true > false)
        match (a.is_exact_match, b.is_exact_match) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => std::cmp::Ordering::Equal,
        }
    }

    fn cmp_distance(
        a: &ServiceInfo,
        b: &ServiceInfo,
        client_location: Option<(f64, f64)>,
    ) -> std::cmp::Ordering {
        use crate::geo::haversine_distance;
        if let Some((lat, lon)) = client_location {
            let da = a.geo_location.as_ref().and_then(|l| {
                l.latitude
                    .zip(l.longitude)
                    .map(|(la, lo)| haversine_distance(lat, lon, la, lo))
            });
            let db = b.geo_location.as_ref().and_then(|l| {
                l.latitude
                    .zip(l.longitude)
                    .map(|(la, lo)| haversine_distance(lat, lon, la, lo))
            });
            match (da, db) {
                (Some(da), Some(db)) => da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => std::cmp::Ordering::Equal,
            }
        } else {
            match (&a.geo_location, &b.geo_location) {
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                _ => std::cmp::Ordering::Equal,
            }
        }
    }

    fn cmp_affinity(
        a: &ServiceInfo,
        b: &ServiceInfo,
        client_id: Option<&str>,
    ) -> std::cmp::Ordering {
        if let Some(cid) = client_id {
            let a_sticky = a.sticky_client_ids.contains(&cid.to_string());
            let b_sticky = b.sticky_client_ids.contains(&cid.to_string());
            match (a_sticky, b_sticky) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                _ => std::cmp::Ordering::Equal,
            }
        } else {
            std::cmp::Ordering::Equal
        }
    }

    /// Sort by remaining capacity (for direct test invocation)
    #[cfg(test)]
    fn sort_by_power_reserve(candidates: &mut [ServiceInfo]) {
        candidates.sort_by(Self::cmp_power_reserve);
    }

    /// Sort by message backlog (for direct test invocation)
    #[cfg(test)]
    fn sort_by_mailbox_backlog(candidates: &mut [ServiceInfo]) {
        candidates.sort_by(Self::cmp_mailbox_backlog);
    }

    /// Sort by exact match priority (for direct test invocation)
    #[cfg(test)]
    #[allow(dead_code)]
    fn sort_by_exact_match(candidates: &mut [ServiceInfo]) {
        candidates.sort_by(Self::cmp_exact_match);
    }

    /// Sort by geographic distance (for direct test invocation)
    #[cfg(test)]
    #[allow(dead_code)]
    fn sort_by_distance(candidates: &mut [ServiceInfo], client_location: Option<(f64, f64)>) {
        candidates.sort_by(|a, b| Self::cmp_distance(a, b, client_location));
    }

    /// Sort by client session stickiness (for direct test invocation)
    #[cfg(test)]
    fn sort_by_affinity(candidates: &mut [ServiceInfo], client_id: Option<&str>) {
        candidates.sort_by(|a, b| Self::cmp_affinity(a, b, client_id));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use actr_protocol::{ActrType, Realm};

    fn create_test_service(serial: u64, name: &str) -> ServiceInfo {
        ServiceInfo {
            actor_id: ActrId {
                serial_number: serial,
                r#type: ActrType {
                    publisher: "test".to_string(),
                    name: name.to_string(),
                    version: "1.0.0".to_string(),
                },
                realm: Realm { realm_id: 0 },
            },
            service_name: name.to_string(),
            message_types: vec![],
            capabilities: None,
            status: crate::actor_registry::ServiceStatus::Available,
            last_heartbeat_time_secs: 0,
            service_spec: None,
            acl: None,
            service_availability_state: None,
            power_reserve: None,
            mailbox_backlog: None,
            worst_dependency_health_state: None,
            geo_location: None,
            is_exact_match: false,
            sticky_client_ids: Vec::new(),
            ws_address: None,
            is_restored_from_storage: false,
        }
    }

    #[test]
    fn test_rank_candidates_without_criteria() {
        let candidates = vec![
            create_test_service(1, "service-1"),
            create_test_service(2, "service-2"),
        ];

        let ranked = LoadBalancer::rank_candidates(&candidates, None, "", None, None);
        assert_eq!(ranked.len(), 2);
    }

    #[test]
    fn test_rank_candidates_with_limit() {
        let candidates = vec![
            create_test_service(1, "service-1"),
            create_test_service(2, "service-2"),
            create_test_service(3, "service-3"),
        ];

        let criteria = NodeSelectionCriteria {
            candidate_count: 2,
            ranking_factors: vec![],
            minimal_dependency_requirement: None,
            minimal_health_requirement: None,
        };

        let ranked = LoadBalancer::rank_candidates(&candidates, Some(&criteria), "", None, None);
        assert_eq!(ranked.len(), 2);
    }

    #[test]
    fn test_empty_candidates() {
        let candidates = vec![];
        let ranked = LoadBalancer::rank_candidates(&candidates, None, "", None, None);
        assert_eq!(ranked.len(), 0);
    }

    // ========================================================================
    // Health and dependency filter tests
    // ========================================================================

    #[test]
    fn test_health_filter_full_only() {
        let mut s1 = create_test_service(1, "s1");
        s1.service_availability_state = Some(ServiceAvailabilityState::Full as i32);
        let mut s2 = create_test_service(2, "s2");
        s2.service_availability_state = Some(ServiceAvailabilityState::Degraded as i32);
        let mut s3 = create_test_service(3, "s3");
        s3.service_availability_state = Some(ServiceAvailabilityState::Overloaded as i32);

        let candidates = vec![s1.clone(), s2, s3];
        let filtered =
            LoadBalancer::filter_by_health(&candidates, ServiceAvailabilityState::Full as i32);

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].actor_id.serial_number, 1);
    }

    #[test]
    fn test_health_filter_with_none() {
        let mut s1 = create_test_service(1, "s1");
        s1.service_availability_state = Some(ServiceAvailabilityState::Full as i32);
        let s2 = create_test_service(2, "s2"); // None
        let mut s3 = create_test_service(3, "s3");
        s3.service_availability_state = Some(ServiceAvailabilityState::Unavailable as i32);

        let candidates = vec![s1.clone(), s2.clone(), s3];

        // Requires DEGRADED or better; None should pass
        let filtered =
            LoadBalancer::filter_by_health(&candidates, ServiceAvailabilityState::Degraded as i32);
        assert_eq!(filtered.len(), 2); // s1(FULL) and s2(None)

        // Sort order should be FULL < None
        assert_eq!(filtered[0].actor_id.serial_number, 1); // FULL ranks first
        assert_eq!(filtered[1].actor_id.serial_number, 2); // None ranks second
    }

    #[test]
    fn test_dependency_filter_healthy_only() {
        let mut s1 = create_test_service(1, "s1");
        s1.worst_dependency_health_state = Some(ServiceDependencyState::Healthy as i32);
        let mut s2 = create_test_service(2, "s2");
        s2.worst_dependency_health_state = Some(ServiceDependencyState::Warning as i32);
        let mut s3 = create_test_service(3, "s3");
        s3.worst_dependency_health_state = Some(ServiceDependencyState::Broken as i32);

        let candidates = vec![s1.clone(), s2, s3];
        let filtered =
            LoadBalancer::filter_by_dependency(&candidates, ServiceDependencyState::Healthy as i32);

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].actor_id.serial_number, 1);
    }

    // ========================================================================
    // Single-factor sorting tests
    // ========================================================================

    #[test]
    fn test_sort_by_power_reserve() {
        let mut s1 = create_test_service(1, "s1");
        s1.power_reserve = Some(0.3);
        let mut s2 = create_test_service(2, "s2");
        s2.power_reserve = Some(0.9);
        let mut s3 = create_test_service(3, "s3");
        s3.power_reserve = Some(0.5);
        let s4 = create_test_service(4, "s4"); // None

        let mut candidates = vec![s1, s2, s3, s4];
        LoadBalancer::sort_by_power_reserve(&mut candidates);

        // Should be descending: 0.9 > 0.5 > 0.3, None last
        assert_eq!(candidates[0].actor_id.serial_number, 2); // 0.9
        assert_eq!(candidates[1].actor_id.serial_number, 3); // 0.5
        assert_eq!(candidates[2].actor_id.serial_number, 1); // 0.3
        assert_eq!(candidates[3].actor_id.serial_number, 4); // None
    }

    #[test]
    fn test_sort_by_mailbox_backlog() {
        let mut s1 = create_test_service(1, "s1");
        s1.mailbox_backlog = Some(0.7);
        let mut s2 = create_test_service(2, "s2");
        s2.mailbox_backlog = Some(0.2);
        let mut s3 = create_test_service(3, "s3");
        s3.mailbox_backlog = Some(0.5);
        let s4 = create_test_service(4, "s4"); // None

        let mut candidates = vec![s1, s2, s3, s4];
        LoadBalancer::sort_by_mailbox_backlog(&mut candidates);

        // Should be ascending: 0.2 < 0.5 < 0.7, None last
        assert_eq!(candidates[0].actor_id.serial_number, 2); // 0.2
        assert_eq!(candidates[1].actor_id.serial_number, 3); // 0.5
        assert_eq!(candidates[2].actor_id.serial_number, 1); // 0.7
        assert_eq!(candidates[3].actor_id.serial_number, 4); // None
    }

    #[test]
    fn test_sort_by_affinity_sticky_clients() {
        let mut s1 = create_test_service(1, "s1");
        s1.sticky_client_ids = vec!["client-A".to_string(), "client-B".to_string()];
        let mut s2 = create_test_service(2, "s2");
        s2.sticky_client_ids = vec!["client-C".to_string()];
        let s3 = create_test_service(3, "s3"); // empty list

        let mut candidates = vec![s1.clone(), s2.clone(), s3.clone()];

        // Test: client-C should route to s2
        LoadBalancer::sort_by_affinity(&mut candidates, Some("client-C"));
        assert_eq!(candidates[0].actor_id.serial_number, 2); // s2 sticky match

        // Test: client-A should route to s1
        let mut candidates = vec![s1.clone(), s2.clone(), s3.clone()];
        LoadBalancer::sort_by_affinity(&mut candidates, Some("client-A"));
        assert_eq!(candidates[0].actor_id.serial_number, 1); // s1 sticky match

        // Test: client-X (not in any sticky list) — all candidates equally prioritized
        let mut candidates = vec![s1, s2, s3];
        LoadBalancer::sort_by_affinity(&mut candidates, Some("client-X"));
        // No sticky match, original order preserved
        assert_eq!(candidates[0].actor_id.serial_number, 1);
    }

    // ========================================================================
    // Multi-factor combined sorting tests
    // ========================================================================

    #[test]
    fn test_multi_factor_ranking() {
        let mut s1 = create_test_service(1, "s1");
        s1.power_reserve = Some(0.8);
        s1.mailbox_backlog = Some(0.3);

        let mut s2 = create_test_service(2, "s2");
        s2.power_reserve = Some(0.8); // same power
        s2.mailbox_backlog = Some(0.1); // but smaller backlog

        let mut s3 = create_test_service(3, "s3");
        s3.power_reserve = Some(0.5);
        s3.mailbox_backlog = Some(0.05); // smallest backlog, but lower power

        let candidates = vec![s1, s2, s3];
        let criteria = NodeSelectionCriteria {
            candidate_count: 10,
            ranking_factors: vec![
                NodeRankingFactor::MaximumPowerReserve as i32,
                NodeRankingFactor::MinimumMailboxBacklog as i32,
            ],
            minimal_health_requirement: None,
            minimal_dependency_requirement: None,
        };

        let ranked = LoadBalancer::rank_candidates(&candidates, Some(&criteria), "", None, None);

        // Multi-key sort: power_reserve is primary key, mailbox_backlog is secondary key
        // s1 and s2 have the same power (0.8); here backlog decides order: s2(0.1) < s1(0.3)
        // s3 power (0.5) < s1/s2 (0.8), ranks last
        assert_eq!(ranked[0].serial_number, 2); // power=0.8, backlog=0.1
        assert_eq!(ranked[1].serial_number, 1); // power=0.8, backlog=0.3
        assert_eq!(ranked[2].serial_number, 3); // power=0.5
    }

    // ========================================================================
    // Edge case tests
    // ========================================================================

    #[test]
    fn test_all_none_values() {
        let candidates = vec![
            create_test_service(1, "s1"),
            create_test_service(2, "s2"),
            create_test_service(3, "s3"),
        ];

        let criteria = NodeSelectionCriteria {
            candidate_count: 10,
            ranking_factors: vec![
                NodeRankingFactor::MaximumPowerReserve as i32,
                NodeRankingFactor::MinimumMailboxBacklog as i32,
            ],
            minimal_health_requirement: None,
            minimal_dependency_requirement: None,
        };

        let ranked = LoadBalancer::rank_candidates(&candidates, Some(&criteria), "", None, None);
        assert_eq!(ranked.len(), 3); // all retained, order unchanged
    }

    #[test]
    fn test_all_same_values() {
        let mut s1 = create_test_service(1, "s1");
        s1.power_reserve = Some(0.5);
        let mut s2 = create_test_service(2, "s2");
        s2.power_reserve = Some(0.5);
        let mut s3 = create_test_service(3, "s3");
        s3.power_reserve = Some(0.5);

        let mut candidates = vec![s1, s2, s3];
        LoadBalancer::sort_by_power_reserve(&mut candidates);

        // All values equal, should maintain stable sort order
        assert_eq!(candidates[0].actor_id.serial_number, 1);
        assert_eq!(candidates[1].actor_id.serial_number, 2);
        assert_eq!(candidates[2].actor_id.serial_number, 3);
    }

    #[test]
    fn test_filter_removes_all_candidates() {
        let mut s1 = create_test_service(1, "s1");
        s1.service_availability_state = Some(ServiceAvailabilityState::Unavailable as i32);
        let mut s2 = create_test_service(2, "s2");
        s2.service_availability_state = Some(ServiceAvailabilityState::Overloaded as i32);

        let candidates = vec![s1, s2];
        let criteria = NodeSelectionCriteria {
            candidate_count: 10,
            ranking_factors: vec![],
            minimal_health_requirement: Some(ServiceAvailabilityState::Full as i32),
            minimal_dependency_requirement: None,
        };

        let ranked = LoadBalancer::rank_candidates(&candidates, Some(&criteria), "", None, None);
        assert_eq!(ranked.len(), 0); // all filtered out
    }

    #[test]
    fn test_sort_by_distance_with_client_location() {
        use crate::actor_registry::ServiceLocation;

        // Client location: Beijing (39.9042, 116.4074)
        let client_location = Some((39.9042, 116.4074));

        // Candidate services: Shanghai, Shenzhen, Beijing
        let mut s1 = create_test_service(1, "shanghai");
        s1.geo_location = Some(ServiceLocation {
            region: "cn-east".to_string(),
            latitude: Some(31.2304),
            longitude: Some(121.4737),
        });

        let mut s2 = create_test_service(2, "shenzhen");
        s2.geo_location = Some(ServiceLocation {
            region: "cn-south".to_string(),
            latitude: Some(22.5431),
            longitude: Some(114.0579),
        });

        let mut s3 = create_test_service(3, "beijing");
        s3.geo_location = Some(ServiceLocation {
            region: "cn-north".to_string(),
            latitude: Some(39.9042),
            longitude: Some(116.4074),
        });

        let s4 = create_test_service(4, "unknown"); // no coordinates

        let candidates = vec![s1, s2, s3, s4];
        let criteria = NodeSelectionCriteria {
            candidate_count: 10,
            ranking_factors: vec![NodeRankingFactor::Nearest as i32],
            minimal_health_requirement: None,
            minimal_dependency_requirement: None,
        };

        let ranked =
            LoadBalancer::rank_candidates(&candidates, Some(&criteria), "", None, client_location);

        // Sort result should be: Beijing(0km) < Shanghai(~1067km) < Shenzhen(~1943km), no coordinates last
        assert_eq!(ranked.len(), 4);
        assert_eq!(ranked[0].serial_number, 3); // Beijing (nearest)
        assert_eq!(ranked[1].serial_number, 1); // Shanghai
        assert_eq!(ranked[2].serial_number, 2); // Shenzhen
        assert_eq!(ranked[3].serial_number, 4); // no coordinates
    }

    // ========================================================================
    // EXACT_MATCH_FIRST factor sorting tests
    // ========================================================================

    fn with_fingerprint(mut s: ServiceInfo, fp: &str) -> ServiceInfo {
        use actr_protocol::ServiceSpec;
        s.service_spec = Some(ServiceSpec {
            name: s.service_name.clone(),
            fingerprint: fp.to_string(),
            description: None,
            protobufs: vec![],
            published_at: None,
            tags: vec![],
        });
        s
    }

    #[test]
    fn test_exact_match_first_factor() {
        // s2 fingerprint matches client_fingerprint → exact match
        let s1 = create_test_service(1, "api");
        let s2 = with_fingerprint(create_test_service(2, "api"), "fp-v1");
        let s3 = create_test_service(3, "api");

        let candidates = vec![s1, s2, s3];
        let criteria = NodeSelectionCriteria {
            candidate_count: 10,
            ranking_factors: vec![NodeRankingFactor::ExactMatchFirst as i32],
            minimal_health_requirement: None,
            minimal_dependency_requirement: None,
        };

        let ranked =
            LoadBalancer::rank_candidates(&candidates, Some(&criteria), "fp-v1", None, None);

        // Exact-matched s2 should rank first
        assert_eq!(ranked[0].serial_number, 2);
        assert_eq!(ranked[1].serial_number, 1);
        assert_eq!(ranked[2].serial_number, 3);
    }

    #[test]
    fn test_exact_match_with_power_reserve() {
        // s1, s3 fingerprints match client_fingerprint → exact match
        let mut s1 = with_fingerprint(create_test_service(1, "api"), "fp-v1");
        s1.power_reserve = Some(0.3);

        let mut s2 = create_test_service(2, "api"); // no fingerprint → non-exact
        s2.power_reserve = Some(0.9);

        let mut s3 = with_fingerprint(create_test_service(3, "api"), "fp-v1");
        s3.power_reserve = Some(0.8);

        let candidates = vec![s1, s2, s3];
        let criteria = NodeSelectionCriteria {
            candidate_count: 10,
            ranking_factors: vec![
                NodeRankingFactor::ExactMatchFirst as i32,
                NodeRankingFactor::MaximumPowerReserve as i32,
            ],
            minimal_health_requirement: None,
            minimal_dependency_requirement: None,
        };

        let ranked =
            LoadBalancer::rank_candidates(&candidates, Some(&criteria), "fp-v1", None, None);

        // ExactMatchFirst priority: exact group (s1, s3) ranks before compat group (s2)
        // Within exact group, sort by power descending: s3(0.8) > s1(0.3)
        assert_eq!(ranked[0].serial_number, 3); // exact, power=0.8
        assert_eq!(ranked[1].serial_number, 1); // exact, power=0.3
        assert_eq!(ranked[2].serial_number, 2); // compat, power=0.9 (exact priority overrides power)
    }
}

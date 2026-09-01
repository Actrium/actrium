//! `get_stats` → `NetworkPath` resolution (RFC §"Stats APIs", Stage 6 item 3).
//!
//! The controller samples `get_stats(Instant::now(), StatsSelector::None)`
//! once when a peer enters `Sendable` (initial connect, recovery, or the
//! pending-session switch). The report's transport entry names the selected
//! candidate pair; the pair names its local/remote candidates; their
//! `candidate_type` classifies the path:
//!
//! * either side `Relay` → [`NetworkPath::Relayed`];
//! * a clear pair with no relay → [`NetworkPath::Direct`];
//! * missing/inconsistent report → keep the previous value (RFC: "maintained
//!   when data is missing or reports are inconsistent").
//!
//! `network_path` never gates sendability — it is observation only (RFC:
//! "`network_path` is updated asynchronously via stats and does not
//! participate in sendability").

use webrtc::peer_connection::{RTCIceCandidateType, RTCStatsReport, RTCStatsReportEntry};

use crate::wire::webrtc::v020::status::NetworkPath;

/// Classify a selected candidate pair (RFC §"Stats APIs").
pub(crate) fn network_path_from_candidate_types(
    local: RTCIceCandidateType,
    remote: RTCIceCandidateType,
) -> NetworkPath {
    if local == RTCIceCandidateType::Relay || remote == RTCIceCandidateType::Relay {
        NetworkPath::Relayed
    } else {
        NetworkPath::Direct
    }
}

/// Extract the selected pair's `(local, remote)` candidate types from a stats
/// report. `None` when the transport entry, the selected pair, or either
/// candidate entry is missing or has an unexpected shape — the caller keeps
/// the previous `NetworkPath`.
pub(crate) fn selected_pair_candidate_types(
    report: &RTCStatsReport,
) -> Option<(RTCIceCandidateType, RTCIceCandidateType)> {
    let pair_id = &report.transport()?.selected_candidate_pair_id;
    if pair_id.is_empty() {
        return None;
    }
    let pair = match report.get(pair_id)? {
        RTCStatsReportEntry::IceCandidatePair(pair) => pair,
        _ => return None,
    };
    let local = candidate_type_of(report, &pair.local_candidate_id, Side::Local)?;
    let remote =
        candidate_type_of(report, &pair.remote_candidate_id, Side::Remote).or_else(|| {
            // rtc 0.20 can select a peer-reflexive remote candidate whose ICE-agent
            // id is referenced by the pair while the stats accumulator only keeps
            // the signaled candidate entry. In that precise shape the missing
            // selected entry is, by definition, peer-reflexive. Do not substitute
            // an arbitrary reported remote candidate: that could misclassify a
            // relay path.
            pair.nominated.then_some(RTCIceCandidateType::Prflx)
        })?;
    Some((local, remote))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Side {
    Local,
    Remote,
}

/// Find a candidate entry's type by its candidate id. rtc 0.20 keys stats
/// entries as `<Type>_<candidate-id>` (e.g.
/// `RTCLocalIceCandidate_candidate:abc`), while the selected pair references
/// the *bare* candidate id (`candidate:abc`) — so match the entry whose key
/// ends with the referenced id, on the expected side.
fn candidate_type_of(
    report: &RTCStatsReport,
    candidate_id: &str,
    side: Side,
) -> Option<RTCIceCandidateType> {
    report.ids().find_map(|id| {
        if !id.ends_with(candidate_id) {
            return None;
        }
        match (report.get(id)?, side) {
            (RTCStatsReportEntry::LocalCandidate(candidate), Side::Local) => {
                Some(candidate.candidate_type)
            }
            (RTCStatsReportEntry::RemoteCandidate(candidate), Side::Remote) => {
                Some(candidate.candidate_type)
            }
            _ => None,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relay_on_either_side_is_relayed() {
        assert_eq!(
            network_path_from_candidate_types(
                RTCIceCandidateType::Relay,
                RTCIceCandidateType::Host
            ),
            NetworkPath::Relayed
        );
        assert_eq!(
            network_path_from_candidate_types(
                RTCIceCandidateType::Host,
                RTCIceCandidateType::Relay
            ),
            NetworkPath::Relayed
        );
    }

    #[test]
    fn non_relay_pair_is_direct() {
        assert_eq!(
            network_path_from_candidate_types(RTCIceCandidateType::Host, RTCIceCandidateType::Host),
            NetworkPath::Direct
        );
        assert_eq!(
            network_path_from_candidate_types(
                RTCIceCandidateType::Srflx,
                RTCIceCandidateType::Prflx
            ),
            NetworkPath::Direct
        );
    }

    #[test]
    fn empty_report_resolves_nothing() {
        // The default (empty) report has no transport entry.
        let report = RTCStatsReport::default();
        assert!(selected_pair_candidate_types(&report).is_none());
    }
}

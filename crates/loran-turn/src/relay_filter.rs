//! Denied-peer filtering for TURN relays.
//!
//! Upstream `turn` 0.11 does not filter the peer address a client can direct the relay
//! to send to, so a client holding a valid credential can use the relay as an
//! SSRF primitive against loopback, private, link-local (cloud metadata) ranges
//! and the host's own services. This module wraps the relay `Conn` and silently
//! drops datagrams addressed to forbidden peers, and wraps the relay-address
//! generator so every allocated relay conn is filtered.

use std::any::Any;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

use async_trait::async_trait;
use turn_crate::relay::RelayAddressGenerator;
use webrtc_util::Conn;
use webrtc_util::Error as UtilError;
use webrtc_util::Result as UtilResult;

/// The upstream `turn` crate keeps its `Result` alias private but re-exports
/// `Error`, so mirror the alias here for the trait return types.
type TurnResult<T> = std::result::Result<T, turn_crate::Error>;

/// Returns true if a relay must never send to this peer address.
///
/// Covers loopback, unspecified, private (RFC1918), link-local (including the
/// 169.254.0.0/16 cloud metadata range), CGNAT (100.64.0.0/10), multicast,
/// broadcast, ULA, and IPv4-mapped/compatible IPv6 forms of all of the above.
pub fn is_forbidden_relay_peer(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let [a, b, _, _] = v4.octets();
            v4.is_unspecified()
                || v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()
                || v4.is_multicast()
                || v4.is_broadcast()
                || a == 0
                || a >= 224
                || (a == 100 && (64..=127).contains(&b)) // CGNAT 100.64.0.0/10
        }
        IpAddr::V6(v6) => {
            // IPv4-mapped/compatible addresses reach IPv4 peers at the socket
            // layer, so judge them by the IPv4 rules.
            if let Some(v4) = v6.to_ipv4_mapped().or_else(|| v6.to_ipv4()) {
                return is_forbidden_relay_peer(IpAddr::V4(v4));
            }
            let seg = v6.segments();
            v6.is_unspecified()
                || v6.is_loopback()
                || v6.is_multicast()
                || v6.is_unique_local()
                || v6.is_unicast_link_local()
                || seg[0] == 0x2002 // 6to4
                || (seg[0] == 0x0064 && seg[1] == 0xff9b) // NAT64 well-known
        }
    }
}

/// A `Conn` decorator that refuses to relay to forbidden peer addresses.
struct FilteringConn {
    inner: Arc<dyn Conn + Send + Sync>,
}

#[async_trait]
impl Conn for FilteringConn {
    async fn connect(&self, addr: SocketAddr) -> UtilResult<()> {
        if is_forbidden_relay_peer(addr.ip()) {
            platform::recording::warn!("TURN relay refused connect to forbidden peer {addr}");
            return Err(UtilError::Other(
                "relay peer address is forbidden".to_string(),
            ));
        }
        self.inner.connect(addr).await
    }

    async fn recv(&self, buf: &mut [u8]) -> UtilResult<usize> {
        self.inner.recv(buf).await
    }

    async fn recv_from(&self, buf: &mut [u8]) -> UtilResult<(usize, SocketAddr)> {
        self.inner.recv_from(buf).await
    }

    async fn send(&self, buf: &[u8]) -> UtilResult<usize> {
        self.inner.send(buf).await
    }

    async fn send_to(&self, buf: &[u8], target: SocketAddr) -> UtilResult<usize> {
        if is_forbidden_relay_peer(target.ip()) {
            platform::recording::warn!("TURN relay dropped datagram to forbidden peer {target}");
            // Report success so the server keeps the allocation; the datagram is
            // silently dropped, exactly as an egress firewall would.
            return Ok(buf.len());
        }
        self.inner.send_to(buf, target).await
    }

    fn local_addr(&self) -> UtilResult<SocketAddr> {
        self.inner.local_addr()
    }

    fn remote_addr(&self) -> Option<SocketAddr> {
        self.inner.remote_addr()
    }

    async fn close(&self) -> UtilResult<()> {
        self.inner.close().await
    }

    fn as_any(&self) -> &(dyn Any + Send + Sync) {
        self
    }
}

/// Wraps another `RelayAddressGenerator`, decorating every allocated relay conn
/// with peer-address filtering.
pub struct FilteringRelayGenerator<G> {
    pub inner: G,
}

#[async_trait]
impl<G> RelayAddressGenerator for FilteringRelayGenerator<G>
where
    G: RelayAddressGenerator + Send + Sync,
{
    fn validate(&self) -> TurnResult<()> {
        self.inner.validate()
    }

    async fn allocate_conn(
        &self,
        use_ipv4: bool,
        requested_port: u16,
    ) -> TurnResult<(Arc<dyn Conn + Send + Sync>, SocketAddr)> {
        let (conn, addr) = self.inner.allocate_conn(use_ipv4, requested_port).await?;
        let filtered: Arc<dyn Conn + Send + Sync> = Arc::new(FilteringConn { inner: conn });
        Ok((filtered, addr))
    }
}

#[cfg(test)]
mod tests {
    use super::is_forbidden_relay_peer;

    #[test]
    fn forbids_internal_and_metadata_peers() {
        for ip in [
            "127.0.0.1",
            "10.0.0.1",
            "172.16.0.1",
            "192.168.0.1",
            "169.254.169.254",
            "100.64.0.1",
            "::1",
            "fd00::1",
            "fe80::1",
            "::ffff:169.254.169.254",
            "::ffff:127.0.0.1",
        ] {
            assert!(
                is_forbidden_relay_peer(ip.parse().unwrap()),
                "{ip} should be forbidden"
            );
        }
        assert!(!is_forbidden_relay_peer("1.1.1.1".parse().unwrap()));
        assert!(!is_forbidden_relay_peer(
            "2606:4700:4700::1111".parse().unwrap()
        ));
    }
}

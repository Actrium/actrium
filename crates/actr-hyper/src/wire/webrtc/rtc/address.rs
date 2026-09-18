//! Local address provider abstraction (RFC §"Local addresses and sockets").
//!
//! webrtc-rs can expand IPv4/IPv6 wildcard bind addresses into the live
//! interface set at every bind, including ICE-restart rebind. Production uses
//! that upstream path; explicit addresses remain available for deterministic
//! tests.
//!
//! The RFC requires the provider to use platform interface information, support
//! change notification, avoid the "probe a port, close the socket, then let the
//! builder bind" race, and fail fast on unsatisfiable bind configuration. A
//! wildcard is a valid bind descriptor; whether an interface can reach a peer
//! is decided later by webrtc-rs and ICE, not by this provider. The explicit
//! provider implementation remains available for deterministic tests and
//! platform integrations. Production uses RTC's wildcard resolver directly:
//! the upstream driver expands the live interface set at each initial bind and
//! ICE-restart rebind, so a cached address list is not part of production
//! recovery.

use std::{
    io,
    net::{IpAddr, Ipv4Addr, SocketAddr, ToSocketAddrs},
    sync::Arc,
};

use async_trait::async_trait;
use tokio::sync::watch;

/// Error type for local-address observation or an invalid bind descriptor.
///
/// These errors describe the provider's local OS query. They do not represent
/// end-to-end network reachability or ICE readiness.
#[derive(Debug, thiserror::Error)]
pub(crate) enum LocalAddressError {
    #[error("failed to enumerate local interfaces: {0}")]
    InterfaceQuery(String),
    #[error("no usable local interface address found")]
    NoUsableAddress,
}

/// A point-in-time view of the local interface addresses, re-published (with a
/// bumped `revision`) whenever the set changes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LocalAddressSnapshot {
    pub(crate) addresses: Vec<IpAddr>,
    pub(crate) revision: u64,
}

impl LocalAddressSnapshot {
    /// A snapshot at revision 0 (tests construct snapshots directly).
    pub(crate) fn new(addresses: Vec<IpAddr>) -> Self {
        Self {
            addresses,
            revision: 0,
        }
    }
}

/// Supplies bind inputs to a `PeerConnectionFactory` and publishes local
/// address snapshots for recovery (RFC §"LocalAddressProvider").
#[async_trait]
pub(crate) trait LocalAddressProvider: Send + Sync {
    /// The current concrete interface snapshot used for address-change
    /// detection. A successful result does not imply network or ICE readiness.
    async fn current_addresses(&self) -> Result<Vec<IpAddr>, LocalAddressError>;

    /// Bind inputs re-resolved by webrtc-rs at startup and on ICE restart.
    /// Production returns dual-stack wildcards so upstream enumerates live
    /// interfaces at bind time. Explicit/test providers return concrete IPs.
    /// This is intentionally independent of the concrete snapshots used for
    /// change detection; a wildcard does not assert that the network is ready.
    fn current_addresses_sync(&self) -> Result<Vec<IpAddr>, LocalAddressError>;

    /// Subscribe to address changes. The receiver immediately exposes the latest
    /// snapshot; platform providers publish a newer revision whenever their
    /// usable interface-address set changes.
    fn subscribe_changes(&self) -> watch::Receiver<LocalAddressSnapshot>;
}

/// A `ToSocketAddrs` value used by the RTC driver on every bind.
///
/// Test/platform providers resolve their current address set on each call. The
/// production factory uses [`Self::wildcard`], which leaves interface
/// expansion to webrtc-rs at the actual bind/rebind point.
#[derive(Clone)]
pub(crate) struct LocalAddressResolver {
    source: LocalAddressResolverSource,
}

#[derive(Clone)]
enum LocalAddressResolverSource {
    Provider(Arc<dyn LocalAddressProvider>),
    Wildcard,
}

impl LocalAddressResolver {
    pub(crate) fn new(provider: Arc<dyn LocalAddressProvider>) -> Self {
        Self {
            source: LocalAddressResolverSource::Provider(provider),
        }
    }

    pub(crate) fn wildcard() -> Self {
        Self {
            source: LocalAddressResolverSource::Wildcard,
        }
    }
}

impl ToSocketAddrs for LocalAddressResolver {
    type Iter = std::vec::IntoIter<SocketAddr>;

    fn to_socket_addrs(&self) -> io::Result<Self::Iter> {
        let addresses = match &self.source {
            LocalAddressResolverSource::Provider(provider) => provider
                .current_addresses_sync()
                .map_err(|error| io::Error::other(error.to_string()))?,
            LocalAddressResolverSource::Wildcard => vec![
                IpAddr::V4(Ipv4Addr::UNSPECIFIED),
                IpAddr::V6(std::net::Ipv6Addr::UNSPECIFIED),
            ],
        };
        if addresses.is_empty() {
            // An empty provider resolve must fail loudly: at startup it
            // surfaces as a build error; at an ICE-restart rebind it fails the
            // driver's rebind, and the recovery path escalates to a checked
            // Hard rebuild instead of idling on a silently dead transport.
            return Err(io::Error::other(
                "local address provider returned no bind descriptors",
            ));
        }
        Ok(addresses
            .into_iter()
            .map(|address| SocketAddr::new(address, 0))
            .collect::<Vec<_>>()
            .into_iter())
    }
}

/// Default desktop [`LocalAddressProvider`].
///
/// Desktop policy:
///
/// Enumerates every currently assigned unicast interface address through the
/// platform's native interface API (`getifaddrs` on Unix/Apple platforms and
/// the corresponding adapter API on Windows). Loopback is retained for local
/// development, while unspecified and multicast addresses are rejected.
/// The production node does not instantiate this polling provider. It remains
/// an explicit provider for tests and platform adapters that need a concrete
/// address snapshot; production desktop binding uses dual-stack wildcards and
/// desktop peer-state events as the recovery fallback.
pub(crate) struct DesktopLocalAddressProvider {
    /// Optional explicit override (used by tests to pin deterministic addresses).
    /// When empty, addresses are auto-detected as described above.
    override_addresses: Vec<IpAddr>,
    change_tx: watch::Sender<LocalAddressSnapshot>,
}

impl DesktopLocalAddressProvider {
    /// Auto-detecting provider (loopback + best-effort outbound IPv4).
    pub(crate) fn new() -> Self {
        Self::with_override(Vec::new())
    }

    /// Provider pinned to explicit addresses (tests use this to bind loopback
    /// deterministically).
    pub(crate) fn with_override(override_addresses: Vec<IpAddr>) -> Self {
        let initial = LocalAddressSnapshot {
            addresses: if override_addresses.is_empty() {
                // `new()` cannot return a startup error. This seed is only for
                // immediate subscription replay; the production bind path uses
                // wildcards and does not make startup depend on this seed.
                Self::detect().unwrap_or_else(|_| vec![IpAddr::V4(Ipv4Addr::LOCALHOST)])
            } else {
                override_addresses.clone()
            },
            revision: 0,
        };
        let (change_tx, _) = watch::channel(initial.clone());
        Self {
            override_addresses,
            change_tx,
        }
    }

    /// Active unicast interface addresses, sorted and de-duplicated.
    fn detect() -> Result<Vec<IpAddr>, LocalAddressError> {
        let interfaces = if_addrs::get_if_addrs()
            .map_err(|error| LocalAddressError::InterfaceQuery(error.to_string()))?;
        let mut addrs = interfaces
            .into_iter()
            .map(|interface| interface.ip())
            .filter(|ip| match ip {
                IpAddr::V4(ip) => !ip.is_unspecified() && !ip.is_multicast() && !ip.is_link_local(),
                IpAddr::V6(ip) => {
                    !ip.is_unspecified() && !ip.is_multicast() && !ip.is_unicast_link_local()
                }
            })
            .collect::<Vec<_>>();
        addrs.sort();
        addrs.dedup();
        if addrs.is_empty() {
            return Err(LocalAddressError::NoUsableAddress);
        }
        Ok(addrs)
    }
}

#[async_trait]
impl LocalAddressProvider for DesktopLocalAddressProvider {
    async fn current_addresses(&self) -> Result<Vec<IpAddr>, LocalAddressError> {
        let addresses = if !self.override_addresses.is_empty() {
            self.override_addresses.clone()
        } else {
            Self::detect()?
        };
        Ok(addresses)
    }

    fn current_addresses_sync(&self) -> Result<Vec<IpAddr>, LocalAddressError> {
        if self.override_addresses.is_empty() {
            Ok(vec![
                IpAddr::V4(Ipv4Addr::UNSPECIFIED),
                IpAddr::V6(std::net::Ipv6Addr::UNSPECIFIED),
            ])
        } else {
            Ok(self.override_addresses.clone())
        }
    }

    fn subscribe_changes(&self) -> watch::Receiver<LocalAddressSnapshot> {
        self.change_tx.subscribe()
    }
}

impl Default for DesktopLocalAddressProvider {
    fn default() -> Self {
        Self::new()
    }
}

/// Mutable address provider for production-factory integration tests. Each
/// update changes the exact address set consumed by the next RTC PC build or
/// same-generation ICE-restart rebind; publishing a lifecycle fact alone is
/// intentionally insufficient.
#[cfg(feature = "test-utils")]
pub struct TestLocalAddressProvider {
    current: std::sync::RwLock<LocalAddressSnapshot>,
    initial_addresses: Vec<IpAddr>,
    wildcard_bind_after_change: bool,
    change_tx: watch::Sender<LocalAddressSnapshot>,
}

#[cfg(feature = "test-utils")]
impl TestLocalAddressProvider {
    pub fn new(addresses: Vec<IpAddr>) -> Self {
        Self::with_bind_mode(addresses, false)
    }

    /// Keep the initial test bind deterministic, then expose dual-stack
    /// wildcards after the first address change. This exercises the production
    /// rebind path: the upstream driver expands the wildcard against the live
    /// interfaces when ICE restart rebinds the existing PeerConnection.
    pub fn new_with_wildcard_rebind(addresses: Vec<IpAddr>) -> Self {
        Self::with_bind_mode(addresses, true)
    }

    fn with_bind_mode(addresses: Vec<IpAddr>, wildcard_bind_after_change: bool) -> Self {
        let initial = LocalAddressSnapshot::new(addresses);
        let initial_addresses = initial.addresses.clone();
        let (change_tx, _) = watch::channel(initial.clone());
        Self {
            current: std::sync::RwLock::new(initial),
            initial_addresses,
            wildcard_bind_after_change,
            change_tx,
        }
    }

    pub fn set_addresses(&self, addresses: Vec<IpAddr>) -> u64 {
        let mut current = self.current.write().expect("address test lock poisoned");
        current.revision = current.revision.saturating_add(1);
        current.addresses = addresses;
        let revision = current.revision;
        self.change_tx.send_replace(current.clone());
        revision
    }

    pub fn snapshot(&self) -> (Vec<IpAddr>, u64) {
        let current = self.current.read().expect("address test lock poisoned");
        (current.addresses.clone(), current.revision)
    }
}

#[cfg(feature = "test-utils")]
#[async_trait]
impl LocalAddressProvider for TestLocalAddressProvider {
    async fn current_addresses(&self) -> Result<Vec<IpAddr>, LocalAddressError> {
        let addresses = self
            .current
            .read()
            .expect("address test lock poisoned")
            .addresses
            .clone();
        if addresses.is_empty() {
            Err(LocalAddressError::NoUsableAddress)
        } else {
            Ok(addresses)
        }
    }

    fn current_addresses_sync(&self) -> Result<Vec<IpAddr>, LocalAddressError> {
        let addresses = self
            .current
            .read()
            .expect("address test lock poisoned")
            .addresses
            .clone();
        if addresses.is_empty() {
            Err(LocalAddressError::NoUsableAddress)
        } else if self.wildcard_bind_after_change && addresses != self.initial_addresses {
            Ok(vec![
                IpAddr::V4(Ipv4Addr::UNSPECIFIED),
                IpAddr::V6(std::net::Ipv6Addr::UNSPECIFIED),
            ])
        } else {
            Ok(addresses)
        }
    }

    fn subscribe_changes(&self) -> watch::Receiver<LocalAddressSnapshot> {
        self.change_tx.subscribe()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sync_snapshot_serves_the_seed_before_any_refresh() {
        let provider =
            DesktopLocalAddressProvider::with_override(vec![IpAddr::V4(Ipv4Addr::LOCALHOST)]);
        assert_eq!(
            provider
                .current_addresses_sync()
                .expect("seeded sync snapshot"),
            vec![IpAddr::V4(Ipv4Addr::LOCALHOST)]
        );
    }

    #[tokio::test]
    async fn explicit_bind_inputs_remain_independent_of_async_snapshots() {
        let provider = DesktopLocalAddressProvider::with_override(vec![IpAddr::V4(Ipv4Addr::new(
            127, 0, 0, 2,
        ))]);
        let refreshed = provider.current_addresses().await.expect("async refresh");
        // Explicit providers pin their bind inputs. Production providers use
        // wildcards instead; neither path treats the snapshot as readiness.
        assert_eq!(
            provider
                .current_addresses_sync()
                .expect("explicit bind inputs"),
            refreshed
        );
    }

    #[tokio::test]
    async fn production_bind_plan_is_wildcard_independent_of_detection() {
        let provider = DesktopLocalAddressProvider::new();
        let refreshed = provider
            .current_addresses()
            .await
            .expect("detect must succeed on a host with a usable interface");
        assert!(!refreshed.is_empty());
        assert_eq!(
            provider
                .current_addresses_sync()
                .expect("detected sync snapshot"),
            vec![
                IpAddr::V4(Ipv4Addr::UNSPECIFIED),
                IpAddr::V6(std::net::Ipv6Addr::UNSPECIFIED),
            ]
        );
    }
}

#[cfg(test)]
mod resolver_tests {
    use super::*;
    use std::sync::RwLock;

    #[test]
    fn wildcard_resolver_keeps_interface_expansion_in_webrtc_rs() {
        let resolved = LocalAddressResolver::wildcard()
            .to_socket_addrs()
            .expect("wildcard bind plan");
        assert_eq!(
            resolved.collect::<Vec<_>>(),
            vec![
                SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0),
                SocketAddr::new(IpAddr::V6(std::net::Ipv6Addr::UNSPECIFIED), 0),
            ]
        );
    }

    /// Provider with a mutable set, optionally erroring on empty like the
    /// production providers do.
    struct MutableStubProvider {
        addresses: RwLock<Vec<IpAddr>>,
        error_when_empty: bool,
        change_tx: watch::Sender<LocalAddressSnapshot>,
    }

    struct WildcardOnlyProvider {
        change_tx: watch::Sender<LocalAddressSnapshot>,
    }

    impl WildcardOnlyProvider {
        fn new() -> Self {
            let (change_tx, _) = watch::channel(LocalAddressSnapshot::new(Vec::new()));
            Self { change_tx }
        }
    }

    impl MutableStubProvider {
        fn new(addresses: Vec<IpAddr>, error_when_empty: bool) -> Self {
            let (change_tx, _) = watch::channel(LocalAddressSnapshot::new(addresses.clone()));
            Self {
                addresses: RwLock::new(addresses),
                error_when_empty,
                change_tx,
            }
        }

        fn set(&self, addresses: Vec<IpAddr>) {
            *self.addresses.write().expect("stub lock poisoned") = addresses;
        }
    }

    #[async_trait]
    impl LocalAddressProvider for MutableStubProvider {
        async fn current_addresses(&self) -> Result<Vec<IpAddr>, LocalAddressError> {
            self.current_addresses_sync()
        }

        fn current_addresses_sync(&self) -> Result<Vec<IpAddr>, LocalAddressError> {
            let addresses = self.addresses.read().expect("stub lock poisoned").clone();
            if addresses.is_empty() && self.error_when_empty {
                Err(LocalAddressError::NoUsableAddress)
            } else {
                Ok(addresses)
            }
        }

        fn subscribe_changes(&self) -> watch::Receiver<LocalAddressSnapshot> {
            self.change_tx.subscribe()
        }
    }

    #[async_trait]
    impl LocalAddressProvider for WildcardOnlyProvider {
        async fn current_addresses(&self) -> Result<Vec<IpAddr>, LocalAddressError> {
            Err(LocalAddressError::NoUsableAddress)
        }

        fn current_addresses_sync(&self) -> Result<Vec<IpAddr>, LocalAddressError> {
            Ok(vec![
                IpAddr::V4(Ipv4Addr::UNSPECIFIED),
                IpAddr::V6(std::net::Ipv6Addr::UNSPECIFIED),
            ])
        }

        fn subscribe_changes(&self) -> watch::Receiver<LocalAddressSnapshot> {
            self.change_tx.subscribe()
        }
    }

    #[test]
    fn resolver_keeps_wildcard_bind_plan_separate_from_readiness_snapshot() {
        let resolver = LocalAddressResolver::new(Arc::new(WildcardOnlyProvider::new()));
        let resolved = resolver
            .to_socket_addrs()
            .expect("wildcard bind descriptors are valid")
            .collect::<Vec<_>>();
        assert_eq!(
            resolved,
            vec![
                SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0),
                SocketAddr::new(IpAddr::V6(std::net::Ipv6Addr::UNSPECIFIED), 0),
            ]
        );
    }

    #[test]
    fn resolver_propagates_sync_errors() {
        let resolver =
            LocalAddressResolver::new(Arc::new(MutableStubProvider::new(Vec::new(), true)));
        let error = resolver
            .to_socket_addrs()
            .expect_err("a sync snapshot error must fail the resolve");
        assert!(error.to_string().contains("no usable local interface"));
    }

    #[test]
    fn resolver_fails_loudly_on_empty_ok_snapshot() {
        // A provider violating the contract with Ok(empty) must still not
        // hand the driver an empty bind plan.
        let resolver =
            LocalAddressResolver::new(Arc::new(MutableStubProvider::new(Vec::new(), false)));
        let error = resolver
            .to_socket_addrs()
            .expect_err("an empty provider set must fail the resolve loudly");
        assert!(error.to_string().contains("no bind descriptors"));
    }

    #[test]
    fn resolver_fails_loudly_when_provider_empties_after_build() {
        let provider = Arc::new(MutableStubProvider::new(
            vec![IpAddr::V4(Ipv4Addr::LOCALHOST)],
            true,
        ));
        let resolver = LocalAddressResolver::new(provider.clone());
        assert_eq!(
            resolver.to_socket_addrs().expect("initial resolve").count(),
            1
        );

        // The runtime case that matters: the provider goes empty after the
        // initial build; the restart-rebind resolve must fail loudly rather
        // than let the driver idle on zero sockets.
        provider.set(Vec::new());
        resolver
            .to_socket_addrs()
            .expect_err("a provider that went empty must fail the rebind resolve");
    }
}

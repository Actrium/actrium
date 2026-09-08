//! Local address provider abstraction (RFC §"Local addresses and sockets").
//!
//! webrtc-rs v0.20 gathers **one host candidate per explicitly bound socket**.
//! Its ICE-restart rebind resolves the configured `ToSocketAddrs` values again,
//! but a concrete address captured at the first build remains concrete. Actr
//! therefore supplies a provider-backed resolver so a same-generation rebind
//! sees the current interface-address set rather than a stale build snapshot.
//!
//! The RFC requires the provider to use platform interface information, support
//! change notification, avoid the "probe a port, close the socket, then let the
//! builder bind" race, and fail fast on unsatisfiable bind configuration. The
//! provider's watch feed is bridged into RTC recovery by the node-owned address
//! monitor. Desktop native enumeration is polled for changes; mobile/platform
//! providers may additionally publish live interface snapshots. The controller
//! first attempts an in-place ICE restart; a failed restart can still escalate
//! to Hard Recovery.

use std::{
    io,
    net::{IpAddr, Ipv4Addr, SocketAddr, ToSocketAddrs},
    sync::{Arc, RwLock},
};

use async_trait::async_trait;
use tokio::sync::watch;

/// Error type for [`LocalAddressProvider::current_addresses`].
///
/// The default desktop provider is infallible (it always yields at least the
/// loopback address); the error variant exists so platform providers that query
/// a fallible OS API can fail fast instead of silently degrading (RFC §"Local
/// addresses and sockets": "fail-fast on unsatisfiable bind configuration").
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

/// Supplies the local IP addresses a `PeerConnectionFactory` should bind, and
/// notifies subscribers when that set changes (RFC §"LocalAddressProvider").
#[async_trait]
pub(crate) trait LocalAddressProvider: Send + Sync {
    /// The current set of local interface addresses to bind (one UDP socket per
    /// address, ephemeral port).
    async fn current_addresses(&self) -> Result<Vec<IpAddr>, LocalAddressError>;

    /// Synchronous snapshot used by webrtc-rs when it re-resolves
    /// `ToSocketAddrs` during an ICE-restart rebind. The rebind resolves
    /// inside the driver's sync bind step, which must never wait on OS
    /// interface APIs: implementations serve the latest snapshot produced by
    /// [`Self::current_addresses`] (or a platform push) from local state
    /// instead of re-querying the interface table here.
    fn current_addresses_sync(&self) -> Result<Vec<IpAddr>, LocalAddressError>;

    /// Subscribe to address changes. The receiver immediately exposes the latest
    /// snapshot; platform providers publish a newer revision whenever their
    /// usable interface-address set changes.
    fn subscribe_changes(&self) -> watch::Receiver<LocalAddressSnapshot>;
}

/// A `ToSocketAddrs` value that resolves the provider's current address set on
/// every bind. The v0.20 driver retains this value inside the PeerConnection
/// and invokes it again when an ICE restart replaces local sockets.
#[derive(Clone)]
pub(crate) struct LocalAddressResolver {
    provider: Arc<dyn LocalAddressProvider>,
}

impl LocalAddressResolver {
    pub(crate) fn new(provider: Arc<dyn LocalAddressProvider>) -> Self {
        Self { provider }
    }
}

impl ToSocketAddrs for LocalAddressResolver {
    type Iter = std::vec::IntoIter<SocketAddr>;

    fn to_socket_addrs(&self) -> io::Result<Self::Iter> {
        let addresses = self
            .provider
            .current_addresses_sync()
            .map_err(|error| io::Error::other(error.to_string()))?;
        if addresses.is_empty() {
            // An empty resolve must fail loudly: at startup it surfaces as a
            // build error; at an ICE-restart rebind it fails the driver's
            // rebind, and the recovery path escalates to a checked Hard
            // rebuild instead of idling on a silently dead transport.
            return Err(io::Error::other(
                "local address provider returned no bindable addresses",
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
/// Change detection is performed by the node-owned address monitor, which
/// combines provider notifications with a polling fallback so desktop and
/// mobile interface changes both trigger RTC recovery.
pub(crate) struct DesktopLocalAddressProvider {
    /// Optional explicit override (used by tests to pin deterministic addresses).
    /// When empty, addresses are auto-detected as described above.
    override_addresses: Vec<IpAddr>,
    /// Latest set produced by a successful `current_addresses()` call.
    /// `current_addresses_sync` serves this cache so the driver's sync rebind
    /// path never queries OS interface APIs; the address monitor's poll
    /// refreshes the cache before it publishes `LocalAddressChanged`, so a
    /// monitor-triggered restart rebind resolves the same fresh set.
    current: RwLock<Vec<IpAddr>>,
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
                // immediate subscription replay; `current_addresses()` runs
                // the fallible query again and Factory build fails fast.
                Self::detect().unwrap_or_else(|_| vec![IpAddr::V4(Ipv4Addr::LOCALHOST)])
            } else {
                override_addresses.clone()
            },
            revision: 0,
        };
        let (change_tx, _) = watch::channel(initial.clone());
        Self {
            override_addresses,
            current: RwLock::new(initial.addresses),
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
        // Refresh only on success: a failed enumeration keeps the last good
        // set available to the sync rebind path.
        *self.current.write().expect("address cache lock poisoned") = addresses.clone();
        Ok(addresses)
    }

    fn current_addresses_sync(&self) -> Result<Vec<IpAddr>, LocalAddressError> {
        Ok(self
            .current
            .read()
            .expect("address cache lock poisoned")
            .clone())
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
/// update changes the exact address set consumed by the next v0.20 PC build or
/// same-generation ICE-restart rebind; publishing a lifecycle fact alone is
/// intentionally insufficient.
#[cfg(feature = "test-utils")]
pub struct TestLocalAddressProvider {
    current: std::sync::RwLock<LocalAddressSnapshot>,
    change_tx: watch::Sender<LocalAddressSnapshot>,
}

#[cfg(feature = "test-utils")]
impl TestLocalAddressProvider {
    pub fn new(addresses: Vec<IpAddr>) -> Self {
        let initial = LocalAddressSnapshot::new(addresses);
        let (change_tx, _) = watch::channel(initial.clone());
        Self {
            current: std::sync::RwLock::new(initial),
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
    async fn sync_snapshot_reflects_the_latest_async_refresh() {
        let provider = DesktopLocalAddressProvider::with_override(vec![IpAddr::V4(Ipv4Addr::new(
            127, 0, 0, 2,
        ))]);
        let refreshed = provider.current_addresses().await.expect("async refresh");
        // The rebind path resolves the sync snapshot; it must observe the
        // set the monitor's poll just refreshed (refresh-before-publish).
        assert_eq!(
            provider
                .current_addresses_sync()
                .expect("refreshed sync snapshot"),
            refreshed
        );
    }

    #[tokio::test]
    async fn sync_snapshot_matches_detected_set_after_async_refresh() {
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
            refreshed
        );
    }
}

#[cfg(test)]
mod resolver_tests {
    use super::*;

    /// Provider with a mutable set, optionally erroring on empty like the
    /// production providers do.
    struct MutableStubProvider {
        addresses: RwLock<Vec<IpAddr>>,
        error_when_empty: bool,
        change_tx: watch::Sender<LocalAddressSnapshot>,
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
        assert!(error.to_string().contains("no bindable addresses"));
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

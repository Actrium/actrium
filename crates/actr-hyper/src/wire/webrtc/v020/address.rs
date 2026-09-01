//! Local address provider abstraction (RFC §"Local addresses and sockets").
//!
//! webrtc-rs v0.20 gathers **one host candidate per explicitly bound socket**
//! and uses the bound address literally — it never enumerates interfaces and
//! never expands an unspecified `0.0.0.0` into real addresses (verified against
//! `stun_gatherer.rs::gather_host_candidates`; see
//! `docs/webrtc-v020-capability-matrix.md`). This is a hard behavioural
//! difference from 0.14, which auto-gathered host candidates on every local
//! interface. Actr must therefore supply real interface addresses to bind, which
//! is exactly the responsibility of [`LocalAddressProvider`].
//!
//! The RFC requires the provider to use platform interface information, support
//! change notification, avoid the "probe a port, close the socket, then let the
//! builder bind" race, and fail fast on unsatisfiable bind configuration. The
//! provider's watch feed is bridged into Hard Recovery by the node-owned address
//! monitor. Desktop native enumeration is polled for changes; mobile/platform
//! providers may additionally publish live interface snapshots.

use std::net::{IpAddr, Ipv4Addr};

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

    /// Subscribe to address changes. The receiver immediately exposes the latest
    /// snapshot; platform providers publish a newer revision whenever their
    /// usable interface-address set changes.
    fn subscribe_changes(&self) -> watch::Receiver<LocalAddressSnapshot>;
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
/// mobile interface changes both trigger Hard Recovery.
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
                // immediate subscription replay; `current_addresses()` runs
                // the fallible query again and Factory build fails fast.
                Self::detect().unwrap_or_else(|_| vec![IpAddr::V4(Ipv4Addr::LOCALHOST)])
            } else {
                override_addresses.clone()
            },
            revision: 0,
        };
        let (change_tx, _) = watch::channel(initial);
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
        if !self.override_addresses.is_empty() {
            return Ok(self.override_addresses.clone());
        }
        Self::detect()
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
/// update changes the exact address set consumed by the next v0.20 PC build;
/// publishing a lifecycle fact alone is intentionally insufficient.
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

    fn subscribe_changes(&self) -> watch::Receiver<LocalAddressSnapshot> {
        self.change_tx.subscribe()
    }
}

use std::{
    collections::HashMap,
    future::Future,
    io::{self, IoSliceMut},
    net::SocketAddr,
    pin::Pin,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
    },
    task::{Context, Poll},
    time::Duration,
};

use actr_protocol::ActrId;
use tokio::sync::Notify;
use webrtc::runtime::{
    AsyncInterval, AsyncTcpListener, AsyncTcpStream, AsyncUdpSocket, JoinHandle, RecvMeta, Runtime,
    TokioRuntime, Transmit,
};

/// Direction of one logical pair fault, expressed from the pair's A/B
/// perspective. The registry maps it to the outbound and inbound socket faces
/// of the two independent PeerConnections.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkDirection {
    AToB,
    BToA,
    Both,
}

/// Deterministic packet disposition supported at the RTC Runtime/socket
/// seam. Every rule is scoped to one `(local, remote, pc_generation)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PacketRule {
    Pass,
    Blackhole,
    DropEvery(u32),
    Delay(Duration),
    DeterministicLoss { percent: u8, seed: u64 },
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct FaultKey {
    local: ActrId,
    remote: ActrId,
    generation: u64,
}

#[derive(Debug)]
struct ScopedFaultState {
    outgoing: Mutex<PacketRule>,
    incoming: Mutex<PacketRule>,
    /// Number of live Runtime/socket leases using this generation. A fault
    /// stops counting only after the builder and every wrapped socket release
    /// their shared lease.
    active_leases: AtomicUsize,
    outgoing_sequence: AtomicU64,
    incoming_sequence: AtomicU64,
    outgoing_hits: AtomicUsize,
    incoming_hits: AtomicUsize,
    hit_notify: Notify,
    bind_addresses: Mutex<Vec<SocketAddr>>,
    bound_socket_addresses: Mutex<Vec<SocketAddr>>,
    bind_notify: Notify,
    send_delay_until: Mutex<Option<std::time::Instant>>,
    recv_delay_until: Mutex<Option<std::time::Instant>>,
}

impl Default for ScopedFaultState {
    fn default() -> Self {
        Self {
            outgoing: Mutex::new(PacketRule::Pass),
            incoming: Mutex::new(PacketRule::Pass),
            active_leases: AtomicUsize::new(0),
            outgoing_sequence: AtomicU64::new(0),
            incoming_sequence: AtomicU64::new(0),
            outgoing_hits: AtomicUsize::new(0),
            incoming_hits: AtomicUsize::new(0),
            hit_notify: Notify::new(),
            bind_addresses: Mutex::new(Vec::new()),
            bound_socket_addresses: Mutex::new(Vec::new()),
            bind_notify: Notify::new(),
            send_delay_until: Mutex::new(None),
            recv_delay_until: Mutex::new(None),
        }
    }
}

/// Hit counters for one generation-scoped fault handle.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DirectedRtcFaultMetrics {
    pub outgoing_hits: usize,
    pub incoming_hits: usize,
}

/// Handle for exactly one local controller generation. A replacement PC gets
/// a different handle and therefore never inherits an old rule implicitly.
#[derive(Debug, Clone)]
pub struct DirectedRtcFaultHandle {
    key: FaultKey,
    state: Arc<ScopedFaultState>,
}

impl DirectedRtcFaultHandle {
    pub fn local(&self) -> &ActrId {
        &self.key.local
    }

    pub fn remote(&self) -> &ActrId {
        &self.key.remote
    }

    pub fn generation(&self) -> u64 {
        self.key.generation
    }

    pub fn set_outgoing(&self, rule: PacketRule) {
        *self.state.outgoing.lock().expect("RTC fault lock poisoned") = rule;
    }

    pub fn set_incoming(&self, rule: PacketRule) {
        *self.state.incoming.lock().expect("RTC fault lock poisoned") = rule;
    }

    pub fn heal(&self) {
        self.set_outgoing(PacketRule::Pass);
        self.set_incoming(PacketRule::Pass);
    }

    pub fn metrics(&self) -> DirectedRtcFaultMetrics {
        DirectedRtcFaultMetrics {
            outgoing_hits: self.state.outgoing_hits.load(Ordering::Acquire),
            incoming_hits: self.state.incoming_hits.load(Ordering::Acquire),
        }
    }

    /// Addresses handed to RTC's `with_udp_addrs` for this exact build.
    pub fn bind_addresses(&self) -> Vec<SocketAddr> {
        self.state
            .bind_addresses
            .lock()
            .expect("RTC bind-address lock poisoned")
            .clone()
    }

    /// Actual local socket addresses observed at the Runtime boundary. Unlike
    /// [`Self::bind_addresses`], this includes sockets created by a
    /// same-generation ICE-restart rebind.
    pub fn bound_socket_addresses(&self) -> Vec<SocketAddr> {
        self.state
            .bound_socket_addresses
            .lock()
            .expect("RTC bound-address lock poisoned")
            .clone()
    }

    /// Wait for a socket created after `baseline` to bind on `expected_ip`.
    /// This observes the actual `UdpSocket::local_addr()` passed through the
    /// Runtime wrapper, not a controller counter or an SDP-only projection.
    pub async fn wait_for_bound_address(
        &self,
        baseline: usize,
        expected_ip: std::net::IpAddr,
        timeout: Duration,
    ) -> Option<SocketAddr> {
        crate::timer::timeout(crate::timer::ids::WEBRTC_PEER_SENDABLE, timeout, async {
            loop {
                if let Some(address) = self
                    .bound_socket_addresses()
                    .into_iter()
                    .skip(baseline)
                    .find(|address| address.ip() == expected_ip)
                {
                    return address;
                }
                self.state.bind_notify.notified().await;
            }
        })
        .await
        .ok()
    }

    /// Wait for this generation's Runtime wrapper to apply at least one fault
    /// after `baseline`. The atomic counter plus stored `Notify` permit makes
    /// this a milestone wait rather than a polling/sleep assertion.
    pub async fn wait_for_hit_after(
        &self,
        baseline: usize,
        timeout: Duration,
    ) -> Option<DirectedRtcFaultMetrics> {
        crate::timer::timeout(crate::timer::ids::WEBRTC_PEER_SENDABLE, timeout, async {
            loop {
                let metrics = self.metrics();
                if metrics.outgoing_hits + metrics.incoming_hits > baseline {
                    return metrics;
                }
                self.state.hit_notify.notified().await;
            }
        })
        .await
        .ok()
    }
}

/// Registry for link-, direction-, and generation-scoped RTC socket faults.
#[derive(Debug, Clone, Default)]
pub struct DirectedRtcFaultRegistry {
    scopes: Arc<Mutex<HashMap<FaultKey, Arc<ScopedFaultState>>>>,
}

/// Pair of test-only fault registries injectable into a real `Node` through
/// [`crate::Node::with_test_faults`]. Both registries share state across
/// clones, so the host can keep its own copy for rule installation and hit
/// observation while the node uses its copy inside the factory and signaling
/// transport.
#[derive(Debug, Clone, Default)]
pub struct TestFaultRegistries {
    pub rtc: DirectedRtcFaultRegistry,
    pub signaling: crate::wire::webrtc::rtc::fault::DirectedSignalingFaultRegistry,
}

impl TestFaultRegistries {
    pub(crate) fn wrap_factory(
        &self,
        factory: super::rtc::factory::PeerConnectionFactory,
        local_id: super::rtc::signaling::SharedLocalId,
    ) -> super::rtc::factory::PeerConnectionFactory {
        factory.with_directed_faults(local_id, self.rtc.clone())
    }

    pub(crate) fn wrap_signaling<T>(
        &self,
        transport: T,
        local_id: super::rtc::signaling::SharedLocalId,
    ) -> super::rtc::fault::DirectedSignalingTransport<T> {
        super::rtc::fault::DirectedSignalingTransport::new(
            Arc::new(transport),
            local_id,
            self.signaling.clone(),
        )
    }
}

impl DirectedRtcFaultRegistry {
    /// Resolve a live scope for fault installation. Retain an already acquired
    /// handle to inspect metrics after teardown; drained scopes cannot be armed.
    pub fn handle(
        &self,
        local: &ActrId,
        remote: &ActrId,
        generation: u64,
    ) -> Option<DirectedRtcFaultHandle> {
        let key = FaultKey {
            local: local.clone(),
            remote: remote.clone(),
            generation,
        };
        self.scopes
            .lock()
            .expect("RTC fault registry lock poisoned")
            .get(&key)
            .filter(|state| state.active_leases.load(Ordering::Acquire) > 0)
            .cloned()
            .map(|state| DirectedRtcFaultHandle { key, state })
    }

    /// Apply one logical directional rule to both socket faces that carry it.
    /// Returns `false` until both endpoint generations have been built and
    /// registered by the RTC factory.
    pub fn set_link_rule(
        &self,
        a: (&ActrId, u64),
        b: (&ActrId, u64),
        direction: LinkDirection,
        rule: PacketRule,
    ) -> bool {
        let Some(a_handle) = self.handle(a.0, b.0, a.1) else {
            return false;
        };
        let Some(b_handle) = self.handle(b.0, a.0, b.1) else {
            return false;
        };
        match direction {
            LinkDirection::AToB => {
                a_handle.set_outgoing(rule);
                b_handle.set_incoming(rule);
            }
            LinkDirection::BToA => {
                b_handle.set_outgoing(rule);
                a_handle.set_incoming(rule);
            }
            LinkDirection::Both => {
                a_handle.set_outgoing(rule);
                a_handle.set_incoming(rule);
                b_handle.set_outgoing(rule);
                b_handle.set_incoming(rule);
            }
        }
        true
    }

    pub fn heal_link(&self, a: (&ActrId, u64), b: (&ActrId, u64)) -> bool {
        let Some(a_handle) = self.handle(a.0, b.0, a.1) else {
            return false;
        };
        let Some(b_handle) = self.handle(b.0, a.0, b.1) else {
            return false;
        };
        a_handle.heal();
        b_handle.heal();
        true
    }

    /// Prune inactive generations for a logical local→remote controller.
    /// Older scopes whose Runtime or wrapped sockets are still live remain
    /// observable and counted until their final lease is released.
    pub fn retain_generation(&self, local: &ActrId, remote: &ActrId, generation: u64) {
        self.scopes
            .lock()
            .expect("RTC fault registry lock poisoned")
            .retain(|key, state| {
                key.local != *local
                    || key.remote != *remote
                    || key.generation == generation
                    || state.active_leases.load(Ordering::Acquire) > 0
            });
    }

    /// Number of retained generation scopes, including inactive scopes kept
    /// for post-teardown metric inspection.
    pub fn scope_count(&self) -> usize {
        self.scopes
            .lock()
            .expect("RTC fault registry lock poisoned")
            .len()
    }

    /// Number of live generation scopes with a currently active non-Pass rule.
    /// A scope remains live while its builder or any wrapped socket holds a
    /// lease, regardless of whether a newer generation has started building.
    pub fn active_rule_count(&self) -> usize {
        self.scopes
            .lock()
            .expect("RTC fault registry lock poisoned")
            .values()
            .filter(|state| {
                state.active_leases.load(Ordering::Acquire) > 0
                    && (!matches!(
                        *state.outgoing.lock().expect("RTC fault lock poisoned"),
                        PacketRule::Pass
                    ) || !matches!(
                        *state.incoming.lock().expect("RTC fault lock poisoned"),
                        PacketRule::Pass
                    ))
            })
            .count()
    }

    pub fn clear(&self) {
        self.scopes
            .lock()
            .expect("RTC fault registry lock poisoned")
            .clear();
    }

    pub(crate) fn scoped_runtime(
        &self,
        inner: Arc<dyn Runtime>,
        local: ActrId,
        remote: ActrId,
        generation: u64,
        bind_addresses: Vec<SocketAddr>,
    ) -> Arc<dyn Runtime> {
        let key = FaultKey {
            local,
            remote,
            generation,
        };
        let (state, lease) = {
            let mut scopes = self
                .scopes
                .lock()
                .expect("RTC fault registry lock poisoned");
            let state = match scopes.get(&key).cloned() {
                Some(state) => {
                    let previous = state.active_leases.fetch_add(1, Ordering::AcqRel);
                    if previous == 0 {
                        // The generation key has been reused after its prior
                        // sockets were fully released. Give this lifetime a
                        // fresh state so an old rule cannot silently reappear.
                        state.active_leases.fetch_sub(1, Ordering::AcqRel);
                        let replacement = Arc::new(ScopedFaultState::default());
                        replacement.active_leases.store(1, Ordering::Release);
                        scopes.insert(key.clone(), replacement.clone());
                        replacement
                    } else {
                        state
                    }
                }
                None => {
                    let state = Arc::new(ScopedFaultState::default());
                    state.active_leases.store(1, Ordering::Release);
                    scopes.insert(key.clone(), state.clone());
                    state
                }
            };
            let lease = Arc::new(ScopedFaultLease {
                state: state.clone(),
            });
            (state, lease)
        };
        *state
            .bind_addresses
            .lock()
            .expect("RTC bind-address lock poisoned") = bind_addresses;
        Arc::new(ScopedFaultRuntime {
            inner,
            state,
            _lease: lease,
        })
    }
}

#[derive(Debug)]
struct ScopedFaultLease {
    state: Arc<ScopedFaultState>,
}

impl Drop for ScopedFaultLease {
    fn drop(&mut self) {
        let previous = self.state.active_leases.fetch_sub(1, Ordering::AcqRel);
        debug_assert!(previous > 0, "RTC fault lease count underflowed");
    }
}

#[derive(Debug)]
struct ScopedFaultRuntime {
    inner: Arc<dyn Runtime>,
    state: Arc<ScopedFaultState>,
    _lease: Arc<ScopedFaultLease>,
}

impl Runtime for ScopedFaultRuntime {
    fn spawn(&self, future: Pin<Box<dyn Future<Output = ()> + Send>>) -> Box<dyn JoinHandle> {
        self.inner.spawn(future)
    }

    fn spawn_reactor(
        &self,
        reactor_pool_size: usize,
        future: Pin<Box<dyn Future<Output = ()> + Send>>,
    ) -> Box<dyn JoinHandle> {
        self.inner.spawn_reactor(reactor_pool_size, future)
    }

    fn wrap_udp_socket(&self, socket: std::net::UdpSocket) -> io::Result<Arc<dyn AsyncUdpSocket>> {
        let local_addr = socket.local_addr()?;
        let inner = self.inner.wrap_udp_socket(socket)?;
        self.state.record_bound_socket(local_addr);
        Ok(Arc::new(ScopedFaultUdpSocket {
            inner,
            state: self.state.clone(),
            _lease: self._lease.clone(),
        }))
    }

    fn wrap_tcp_listener(
        &self,
        listener: std::net::TcpListener,
    ) -> io::Result<Arc<dyn AsyncTcpListener>> {
        Ok(Arc::new(ScopedFaultTcpListener {
            inner: self.inner.wrap_tcp_listener(listener)?,
            state: self.state.clone(),
            _lease: self._lease.clone(),
        }))
    }

    fn connect_tcp<'a>(
        &'a self,
        remote_addr: SocketAddr,
    ) -> Pin<Box<dyn Future<Output = io::Result<Arc<dyn AsyncTcpStream>>> + Send + 'a>> {
        Box::pin(async move {
            let inner = self.inner.connect_tcp(remote_addr).await?;
            Ok(Arc::new(ScopedFaultTcpStream {
                inner,
                state: self.state.clone(),
                _lease: self._lease.clone(),
            }) as Arc<dyn AsyncTcpStream>)
        })
    }

    fn resolve_host<'a>(
        &'a self,
        host: &'a str,
    ) -> Pin<Box<dyn Future<Output = io::Result<Vec<SocketAddr>>> + Send + 'a>> {
        self.inner.resolve_host(host)
    }

    fn sleep(&self, duration: Duration) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>> {
        self.inner.sleep(duration)
    }

    fn interval(&self, period: Duration) -> Box<dyn AsyncInterval> {
        self.inner.interval(period)
    }

    fn block_on(&self, future: Pin<Box<dyn Future<Output = ()> + '_>>) {
        self.inner.block_on(future);
    }

    fn yield_now(&self) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>> {
        self.inner.yield_now()
    }

    fn name(&self) -> &'static str {
        "actr-directed-rtc-fault"
    }
}

#[derive(Debug)]
struct ScopedFaultUdpSocket {
    inner: Arc<dyn AsyncUdpSocket>,
    state: Arc<ScopedFaultState>,
    _lease: Arc<ScopedFaultLease>,
}

impl ScopedFaultState {
    fn record_outgoing_hit(&self) {
        self.outgoing_hits.fetch_add(1, Ordering::Release);
        self.hit_notify.notify_one();
    }

    fn record_incoming_hit(&self) {
        self.incoming_hits.fetch_add(1, Ordering::Release);
        self.hit_notify.notify_one();
    }

    fn record_bound_socket(&self, address: SocketAddr) {
        self.bound_socket_addresses
            .lock()
            .expect("RTC bound-address lock poisoned")
            .push(address);
        self.bind_notify.notify_one();
    }
}

fn deterministic_drop(rule: PacketRule, sequence: u64) -> bool {
    match rule {
        PacketRule::Pass | PacketRule::Delay(_) => false,
        PacketRule::Blackhole => true,
        PacketRule::DropEvery(n) => n != 0 && sequence.is_multiple_of(u64::from(n)),
        PacketRule::DeterministicLoss { percent, seed } => {
            if percent == 0 {
                return false;
            }
            if percent >= 100 {
                return true;
            }
            let mut value = sequence
                .wrapping_add(seed)
                .wrapping_add(0x9e37_79b9_7f4a_7c15);
            value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
            value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
            ((value ^ (value >> 31)) % 100) < u64::from(percent)
        }
    }
}

fn delay_pending(
    delay: Duration,
    deadline: &Mutex<Option<std::time::Instant>>,
    cx: &Context<'_>,
) -> bool {
    let mut guard = deadline.lock().expect("RTC delay gate lock poisoned");
    if let Some(until) = *guard {
        if std::time::Instant::now() >= until {
            *guard = None;
            return false;
        }
        return true;
    }
    *guard = Some(std::time::Instant::now() + delay);
    let waker = cx.waker().clone();
    tokio::spawn(async move {
        crate::timer::sleep(crate::timer::ids::RTC_TEST_FAULT_DELAY, delay).await;
        waker.wake();
    });
    true
}

impl AsyncUdpSocket for ScopedFaultUdpSocket {
    fn local_addr(&self) -> io::Result<SocketAddr> {
        self.inner.local_addr()
    }

    fn poll_send(&self, cx: &mut Context<'_>, transmit: &Transmit<'_>) -> Poll<io::Result<usize>> {
        let rule = *self.state.outgoing.lock().expect("RTC fault lock poisoned");
        if let PacketRule::Delay(delay) = rule
            && delay_pending(delay, &self.state.send_delay_until, cx)
        {
            self.state.record_outgoing_hit();
            return Poll::Pending;
        }
        // A poll is not a datagram. Keep the next sequence stable while the
        // inner socket returns Pending, then advance it exactly once when this
        // transmit is either delivered or intentionally dropped.
        let sequence = self.state.outgoing_sequence.load(Ordering::Acquire) + 1;
        if deterministic_drop(rule, sequence) {
            self.state.outgoing_sequence.fetch_add(1, Ordering::AcqRel);
            self.state.record_outgoing_hit();
            return Poll::Ready(Ok(transmit.contents.len()));
        }
        let result = self.inner.poll_send(cx, transmit);
        if matches!(result, Poll::Ready(Ok(_))) {
            self.state.outgoing_sequence.fetch_add(1, Ordering::AcqRel);
        }
        result
    }

    fn poll_recv(
        &self,
        cx: &mut Context<'_>,
        bufs: &mut [IoSliceMut<'_>],
        meta: &mut [RecvMeta],
    ) -> Poll<io::Result<usize>> {
        let rule = *self.state.incoming.lock().expect("RTC fault lock poisoned");
        if let PacketRule::Delay(delay) = rule
            && delay_pending(delay, &self.state.recv_delay_until, cx)
        {
            self.state.record_incoming_hit();
            return Poll::Pending;
        }
        let result = self.inner.poll_recv(cx, bufs, meta);
        let Poll::Ready(Ok(received)) = result else {
            return result;
        };
        let sequence = self.state.incoming_sequence.fetch_add(1, Ordering::AcqRel) + 1;
        if deterministic_drop(rule, sequence) {
            self.state.record_incoming_hit();
            cx.waker().wake_by_ref();
            Poll::Pending
        } else {
            Poll::Ready(Ok(received))
        }
    }

    fn max_gso_segments(&self) -> usize {
        self.inner.max_gso_segments()
    }

    fn max_gro_segments(&self) -> usize {
        self.inner.max_gro_segments()
    }
}

#[derive(Debug)]
struct ScopedFaultTcpListener {
    inner: Arc<dyn AsyncTcpListener>,
    state: Arc<ScopedFaultState>,
    _lease: Arc<ScopedFaultLease>,
}

impl AsyncTcpListener for ScopedFaultTcpListener {
    fn accept<'a>(
        &'a self,
    ) -> Pin<Box<dyn Future<Output = io::Result<(Arc<dyn AsyncTcpStream>, SocketAddr)>> + Send + 'a>>
    {
        Box::pin(async move {
            let (inner, remote) = self.inner.accept().await?;
            Ok((
                Arc::new(ScopedFaultTcpStream {
                    inner,
                    state: self.state.clone(),
                    _lease: self._lease.clone(),
                }) as Arc<dyn AsyncTcpStream>,
                remote,
            ))
        })
    }

    fn local_addr(&self) -> io::Result<SocketAddr> {
        self.inner.local_addr()
    }
}

#[derive(Debug)]
struct ScopedFaultTcpStream {
    inner: Arc<dyn AsyncTcpStream>,
    state: Arc<ScopedFaultState>,
    _lease: Arc<ScopedFaultLease>,
}

impl AsyncTcpStream for ScopedFaultTcpStream {
    fn read<'a, 'b>(
        &'a self,
        buf: &'b mut [u8],
    ) -> Pin<Box<dyn Future<Output = io::Result<usize>> + Send + 'b>>
    where
        'a: 'b,
    {
        Box::pin(async move {
            loop {
                let rule = *self.state.incoming.lock().expect("RTC fault lock poisoned");
                if let PacketRule::Delay(delay) = rule {
                    self.state.record_incoming_hit();
                    crate::timer::sleep(crate::timer::ids::RTC_TEST_FAULT_DELAY, delay).await;
                }
                let received = self.inner.read(buf).await?;
                if received == 0 {
                    return Ok(0);
                }
                let sequence = self.state.incoming_sequence.fetch_add(1, Ordering::AcqRel) + 1;
                if deterministic_drop(rule, sequence) {
                    self.state.record_incoming_hit();
                    continue;
                }
                return Ok(received);
            }
        })
    }

    fn write_all<'a, 'b>(
        &'a self,
        buf: &'b [u8],
    ) -> Pin<Box<dyn Future<Output = io::Result<()>> + Send + 'b>>
    where
        'a: 'b,
    {
        Box::pin(async move {
            let rule = *self.state.outgoing.lock().expect("RTC fault lock poisoned");
            if let PacketRule::Delay(delay) = rule {
                self.state.record_outgoing_hit();
                crate::timer::sleep(crate::timer::ids::RTC_TEST_FAULT_DELAY, delay).await;
            }
            let sequence = self.state.outgoing_sequence.fetch_add(1, Ordering::AcqRel) + 1;
            if deterministic_drop(rule, sequence) {
                self.state.record_outgoing_hit();
                return Ok(());
            }
            self.inner.write_all(buf).await
        })
    }

    fn local_addr(&self) -> io::Result<SocketAddr> {
        self.inner.local_addr()
    }

    fn peer_addr(&self) -> io::Result<SocketAddr> {
        self.inner.peer_addr()
    }
}

#[cfg(feature = "test-utils")]
pub(crate) fn packet_filter_runtime(blocked: Arc<AtomicBool>) -> Arc<dyn Runtime> {
    Arc::new(PacketFilterRuntime {
        inner: TokioRuntime,
        blocked: Some(blocked),
    })
}

#[derive(Debug)]
struct PacketFilterRuntime {
    inner: TokioRuntime,
    blocked: Option<Arc<AtomicBool>>,
}

impl Runtime for PacketFilterRuntime {
    fn spawn(&self, future: Pin<Box<dyn Future<Output = ()> + Send>>) -> Box<dyn JoinHandle> {
        self.inner.spawn(future)
    }

    fn spawn_reactor(
        &self,
        reactor_pool_size: usize,
        future: Pin<Box<dyn Future<Output = ()> + Send>>,
    ) -> Box<dyn JoinHandle> {
        self.inner.spawn_reactor(reactor_pool_size, future)
    }

    fn wrap_udp_socket(&self, socket: std::net::UdpSocket) -> io::Result<Arc<dyn AsyncUdpSocket>> {
        let inner = self.inner.wrap_udp_socket(socket)?;
        Ok(match &self.blocked {
            Some(blocked) => Arc::new(PacketFilterUdpSocket {
                inner,
                blocked: Arc::clone(blocked),
            }),
            None => inner,
        })
    }

    fn wrap_tcp_listener(
        &self,
        listener: std::net::TcpListener,
    ) -> io::Result<Arc<dyn AsyncTcpListener>> {
        self.inner.wrap_tcp_listener(listener)
    }

    fn connect_tcp<'a>(
        &'a self,
        remote_addr: SocketAddr,
    ) -> Pin<Box<dyn Future<Output = io::Result<Arc<dyn AsyncTcpStream>>> + Send + 'a>> {
        self.inner.connect_tcp(remote_addr)
    }

    fn resolve_host<'a>(
        &'a self,
        host: &'a str,
    ) -> Pin<Box<dyn Future<Output = io::Result<Vec<SocketAddr>>> + Send + 'a>> {
        self.inner.resolve_host(host)
    }

    fn sleep(&self, duration: Duration) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>> {
        self.inner.sleep(duration)
    }

    fn interval(&self, period: Duration) -> Box<dyn AsyncInterval> {
        self.inner.interval(period)
    }

    fn block_on(&self, future: Pin<Box<dyn Future<Output = ()> + '_>>) {
        self.inner.block_on(future);
    }

    fn yield_now(&self) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>> {
        self.inner.yield_now()
    }

    fn name(&self) -> &'static str {
        "tokio-packet-filter"
    }
}

#[derive(Debug)]
struct PacketFilterUdpSocket {
    inner: Arc<dyn AsyncUdpSocket>,
    blocked: Arc<AtomicBool>,
}

impl AsyncUdpSocket for PacketFilterUdpSocket {
    fn local_addr(&self) -> io::Result<SocketAddr> {
        self.inner.local_addr()
    }

    fn poll_send(&self, cx: &mut Context<'_>, transmit: &Transmit<'_>) -> Poll<io::Result<usize>> {
        if self.blocked.load(Ordering::Acquire) {
            Poll::Ready(Ok(transmit.contents.len()))
        } else {
            self.inner.poll_send(cx, transmit)
        }
    }

    fn poll_recv(
        &self,
        cx: &mut Context<'_>,
        bufs: &mut [IoSliceMut<'_>],
        meta: &mut [RecvMeta],
    ) -> Poll<io::Result<usize>> {
        if !self.blocked.load(Ordering::Acquire) {
            return self.inner.poll_recv(cx, bufs, meta);
        }

        match self.inner.poll_recv(cx, bufs, meta) {
            Poll::Ready(Ok(_)) => {
                cx.waker().wake_by_ref();
                Poll::Pending
            }
            Poll::Ready(Err(err)) => Poll::Ready(Err(err)),
            Poll::Pending => Poll::Pending,
        }
    }

    fn max_gso_segments(&self) -> usize {
        self.inner.max_gso_segments()
    }

    fn max_gro_segments(&self) -> usize {
        self.inner.max_gro_segments()
    }
}

#[cfg(test)]
mod directed_tests {

    use super::*;

    fn actor(serial_number: u64) -> ActrId {
        ActrId {
            realm: "test@example.com".to_string(),
            serial_number,
        }
    }

    #[tokio::test]
    async fn active_fault_leases_follow_runtime_lifetime_across_generations() {
        let registry = DirectedRtcFaultRegistry::default();
        let a = actor(1);
        let b = actor(2);
        let a1 = registry.scoped_runtime(Arc::new(TokioRuntime), a.clone(), b.clone(), 1, vec![]);
        let b1 = registry.scoped_runtime(Arc::new(TokioRuntime), b.clone(), a.clone(), 1, vec![]);
        let a1_socket = a1
            .wrap_udp_socket(std::net::UdpSocket::bind("127.0.0.1:0").unwrap())
            .unwrap();
        assert!(registry.set_link_rule(
            (&a, 1),
            (&b, 1),
            LinkDirection::AToB,
            PacketRule::Blackhole
        ));
        assert!(
            registry.active_rule_count() > 0,
            "an installed generation-1 fault must count as active"
        );

        // Beginning a newer build does not retire the older scope while its
        // Runtime lease is still live.
        let _a2 = registry.scoped_runtime(Arc::new(TokioRuntime), a.clone(), b.clone(), 2, vec![]);
        let _b2 = registry.scoped_runtime(Arc::new(TokioRuntime), b.clone(), a.clone(), 2, vec![]);
        assert_eq!(
            registry.active_rule_count(),
            2,
            "a newer build must not hide a still-live older generation fault"
        );

        // Pruning generation 1 keeps the A→B scope because its wrapped UDP
        // socket still holds a lease, even after the runtime wrappers drop.
        drop((a1, b1));
        registry.retain_generation(&a, &b, 2);
        registry.retain_generation(&b, &a, 2);
        assert!(registry.handle(&a, &b, 1).is_some());
        assert!(registry.handle(&b, &a, 1).is_none());
        assert_eq!(
            registry.active_rule_count(),
            1,
            "the wrapped socket keeps the generation-1 A→B fault active"
        );

        // Once the last socket lease is released, the next prune can remove
        // generation 1 without touching the active generation 2 scopes.
        drop(a1_socket);
        registry.retain_generation(&a, &b, 2);
        registry.retain_generation(&b, &a, 2);
        assert!(registry.handle(&a, &b, 1).is_none());
        assert!(registry.handle(&a, &b, 2).is_some());
        assert_eq!(
            registry.active_rule_count(),
            0,
            "the fault lease ends after the old generation releases its final socket"
        );
    }

    #[test]
    fn drained_scope_cannot_accept_faults_before_generation_key_is_reused() {
        let registry = DirectedRtcFaultRegistry::default();
        let (a, b) = (actor(1), actor(2));
        let a1 = registry.scoped_runtime(Arc::new(TokioRuntime), a.clone(), b.clone(), 1, vec![]);
        let b1 = registry.scoped_runtime(Arc::new(TokioRuntime), b.clone(), a.clone(), 1, vec![]);
        let old = registry.handle(&a, &b, 1).unwrap();
        drop((a1, b1));
        assert_eq!(registry.scope_count(), 2, "retain post-teardown metrics");
        assert!(registry.handle(&a, &b, 1).is_none());
        assert!(!registry.set_link_rule(
            (&a, 1),
            (&b, 1),
            LinkDirection::Both,
            PacketRule::Blackhole
        ));
        assert_eq!(old.metrics(), DirectedRtcFaultMetrics::default());

        let _a1 = registry.scoped_runtime(Arc::new(TokioRuntime), a.clone(), b.clone(), 1, vec![]);
        let _b1 = registry.scoped_runtime(Arc::new(TokioRuntime), b.clone(), a.clone(), 1, vec![]);
        assert_eq!(
            registry.active_rule_count(),
            0,
            "new lifetime starts unfaulted"
        );
        assert!(registry.set_link_rule(
            (&a, 1),
            (&b, 1),
            LinkDirection::Both,
            PacketRule::Blackhole
        ));
        assert_eq!(registry.active_rule_count(), 2);
    }

    #[test]
    fn direction_and_generation_rules_are_independent() {
        let registry = DirectedRtcFaultRegistry::default();
        let a = actor(1);
        let b = actor(2);
        let _a1 = registry.scoped_runtime(Arc::new(TokioRuntime), a.clone(), b.clone(), 1, vec![]);
        let _b1 = registry.scoped_runtime(Arc::new(TokioRuntime), b.clone(), a.clone(), 1, vec![]);
        let _a2 = registry.scoped_runtime(Arc::new(TokioRuntime), a.clone(), b.clone(), 2, vec![]);
        let _b2 = registry.scoped_runtime(Arc::new(TokioRuntime), b.clone(), a.clone(), 2, vec![]);

        assert!(registry.set_link_rule(
            (&a, 1),
            (&b, 1),
            LinkDirection::AToB,
            PacketRule::Blackhole,
        ));
        let a1 = registry.handle(&a, &b, 1).expect("A generation 1");
        let b1 = registry.handle(&b, &a, 1).expect("B generation 1");
        let a2 = registry.handle(&a, &b, 2).expect("A generation 2");
        assert_eq!(
            *a1.state.outgoing.lock().expect("A outgoing rule"),
            PacketRule::Blackhole
        );
        assert_eq!(
            *b1.state.incoming.lock().expect("B incoming rule"),
            PacketRule::Blackhole
        );
        assert_eq!(
            *a1.state.incoming.lock().expect("A incoming rule"),
            PacketRule::Pass
        );
        assert_eq!(
            *a2.state.outgoing.lock().expect("replacement outgoing rule"),
            PacketRule::Pass,
            "a replacement generation must not inherit an old rule"
        );
    }

    #[test]
    fn deterministic_loss_is_replayable_for_the_same_seed() {
        let first = (1..=100)
            .map(|sequence| {
                deterministic_drop(
                    PacketRule::DeterministicLoss {
                        percent: 37,
                        seed: 9,
                    },
                    sequence,
                )
            })
            .collect::<Vec<_>>();
        let replay = (1..=100)
            .map(|sequence| {
                deterministic_drop(
                    PacketRule::DeterministicLoss {
                        percent: 37,
                        seed: 9,
                    },
                    sequence,
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(first, replay);
        assert!(first.iter().any(|drop| *drop));
        assert!(first.iter().any(|drop| !*drop));
    }
}

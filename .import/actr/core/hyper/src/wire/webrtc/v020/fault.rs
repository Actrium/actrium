//! Fault-injection seams for the v0.20 RTC architecture (RFC Stage 5 item 10).
//!
//! Two seams, both restricted to test / explicit debug builds (RFC §"Security
//! and privacy": "Runtime/socket fault injection MUST only be enabled in test
//! or explicit debug configuration"):
//!
//! * [`RuntimeFaultProbe`] — the RFC's `InstrumentedRuntime`: wraps the
//!   production `Runtime`, collects metrics at the task/socket/DNS/timer
//!   boundaries (`spawn`, `wrap_udp_socket`, `wrap_tcp_listener`,
//!   `connect_tcp`, `resolve_host`, `sleep`, `interval`), and injects a small,
//!   explicit fault set: failing TCP connects, blackholed UDP sockets (sends
//!   accepted then dropped, receives never delivered), and spawned-driver
//!   exit. Inject it through `RtcRuntimeConfig::with_runtime_override`.
//! * [`DeterministicNetwork`] — a `SignalingTransport` decorator applying a
//!   deterministic, test-defined rule (loss / delay / duplicate / partition)
//!   per signaling message. The specific loss/reorder/device matrix is
//!   deliberately NOT fixed here (RFC: "do not fix a loss/reorder or device
//!   matrix at this stage"); concrete scenarios land in the follow-up test
//!   plans (docs/webrtc-v020-recovery-test-plan.md).
//!
//! System/platform harness seams (identified, not built here): the platform
//! network monitor publishes `RtcNetworkEvent`s via
//! `RtcRecoverySupervisor::publish` (S5-1); address changes come through
//! `LocalAddressProvider` (`address.rs`, with a test override); the unified
//! observation facade for harness assertions is `RtcConnectionManager`'s
//! snapshot/budget surface (S5-11).
//!
//! Clock note (R3): the stock crates.io v0.20 runtime has no `Runtime::now()`
//! seam. `ManualPolicyClock` still drives Actr policy tests, but high-level
//! driver and protocol-core virtual-time replay are not claimed; see
//! docs/webrtc-v020-clock-patch-tracking.md.

use std::fmt::Debug;
use std::io::IoSliceMut;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::task::{Context, Poll};
use std::time::{Duration, Instant, SystemTime};

use actr_protocol::ActrId;
use webrtc::runtime::{
    AsyncInterval, AsyncTcpListener, AsyncTcpStream, AsyncUdpSocket, JoinHandle, RecvMeta, Runtime,
    Transmit,
};

use super::peer::reducer::SignalingMessage;
use super::signaling::router::SignalingTransport;

// ---- RuntimeFaultProbe -----------------------------------------------------

/// Metrics collected at the runtime boundary (RFC: "collect indicators at the
/// task and socket boundaries"). All counters are monotonic.
#[derive(Debug, Default)]
pub(crate) struct ProbeMetrics {
    pub spawned_tasks: AtomicUsize,
    pub udp_sockets_wrapped: AtomicUsize,
    pub tcp_listeners_wrapped: AtomicUsize,
    pub tcp_connects: AtomicUsize,
    pub host_resolutions: AtomicUsize,
    pub sleeps: AtomicUsize,
    pub intervals: AtomicUsize,
    // Injected faults actually applied.
    pub tcp_connect_failures_injected: AtomicUsize,
    pub udp_sockets_blackholed: AtomicUsize,
    pub spawned_tasks_killed: AtomicUsize,
    pub blackholed_datagrams: AtomicUsize,
}

/// Fault switches for the probe. Tests arm/disarm them at any time; the
/// switches are checked at each intercepted call.
#[derive(Debug, Default)]
pub(crate) struct FaultSwitches {
    /// `connect_tcp` fails with `ConnectionRefused`.
    fail_connect_tcp: AtomicBool,
    /// `wrap_udp_socket` returns a blackhole socket (sends dropped, receives
    /// never delivered).
    blackhole_udp: AtomicBool,
    /// `spawn` replaces the task with one that exits immediately (driver exit).
    kill_spawned_tasks: AtomicBool,
}

impl FaultSwitches {
    pub(crate) fn set_fail_connect_tcp(&self, on: bool) {
        self.fail_connect_tcp.store(on, Ordering::Relaxed);
    }
    pub(crate) fn set_blackhole_udp(&self, on: bool) {
        self.blackhole_udp.store(on, Ordering::Relaxed);
    }
    pub(crate) fn set_kill_spawned_tasks(&self, on: bool) {
        self.kill_spawned_tasks.store(on, Ordering::Relaxed);
    }
}

/// An instrumented `Runtime` wrapper: delegates to the production runtime,
/// counts every boundary crossing, and applies the armed faults. The only
/// injection point into a PeerConnection is `with_runtime(...)`
/// (RFC §"Runtime and network-test APIs"), which `RtcRuntimeConfig`'s
/// test-only override forwards.
#[derive(Debug)]
pub(crate) struct RuntimeFaultProbe {
    inner: Arc<dyn Runtime>,
    metrics: Arc<ProbeMetrics>,
    switches: Arc<FaultSwitches>,
}

impl RuntimeFaultProbe {
    pub(crate) fn new(inner: Arc<dyn Runtime>) -> Self {
        Self {
            inner,
            metrics: Arc::new(ProbeMetrics::default()),
            switches: Arc::new(FaultSwitches::default()),
        }
    }

    pub(crate) fn metrics(&self) -> &Arc<ProbeMetrics> {
        &self.metrics
    }

    pub(crate) fn switches(&self) -> &Arc<FaultSwitches> {
        &self.switches
    }
}

impl Runtime for RuntimeFaultProbe {
    fn spawn(
        &self,
        future: Pin<Box<dyn std::future::Future<Output = ()> + Send>>,
    ) -> Box<dyn JoinHandle> {
        self.metrics.spawned_tasks.fetch_add(1, Ordering::Relaxed);
        if self.switches.kill_spawned_tasks.load(Ordering::Relaxed) {
            self.metrics
                .spawned_tasks_killed
                .fetch_add(1, Ordering::Relaxed);
            return self.inner.spawn(Box::pin(async {}));
        }
        self.inner.spawn(future)
    }

    fn spawn_reactor(
        &self,
        reactor_pool_size: usize,
        future: Pin<Box<dyn std::future::Future<Output = ()> + Send>>,
    ) -> Box<dyn JoinHandle> {
        // Delegate to the inner pool so reactor confinement is preserved when
        // the probe wraps a dedicated-reactor configuration.
        self.metrics.spawned_tasks.fetch_add(1, Ordering::Relaxed);
        if self.switches.kill_spawned_tasks.load(Ordering::Relaxed) {
            self.metrics
                .spawned_tasks_killed
                .fetch_add(1, Ordering::Relaxed);
            return self
                .inner
                .spawn_reactor(reactor_pool_size, Box::pin(async {}));
        }
        self.inner.spawn_reactor(reactor_pool_size, future)
    }

    fn wrap_udp_socket(
        &self,
        socket: std::net::UdpSocket,
    ) -> std::io::Result<Arc<dyn AsyncUdpSocket>> {
        self.metrics
            .udp_sockets_wrapped
            .fetch_add(1, Ordering::Relaxed);
        let wrapped = self.inner.wrap_udp_socket(socket)?;
        let blackholed = self.switches.blackhole_udp.load(Ordering::Relaxed);
        if blackholed {
            self.metrics
                .udp_sockets_blackholed
                .fetch_add(1, Ordering::Relaxed);
        }
        // Always install the lightweight wrapper so a test can blackhole and
        // heal an already-established PC. Checking the switch only at socket
        // construction would model startup failure, not a mid-session link
        // loss/recovery.
        Ok(Arc::new(FaultInjectedUdpSocket {
            inner: wrapped,
            metrics: self.metrics.clone(),
            switches: self.switches.clone(),
            blackhole_counted: AtomicBool::new(blackholed),
        }))
    }

    fn wrap_tcp_listener(
        &self,
        listener: std::net::TcpListener,
    ) -> std::io::Result<Arc<dyn AsyncTcpListener>> {
        self.metrics
            .tcp_listeners_wrapped
            .fetch_add(1, Ordering::Relaxed);
        self.inner.wrap_tcp_listener(listener)
    }

    fn connect_tcp<'a>(
        &'a self,
        remote_addr: SocketAddr,
    ) -> Pin<
        Box<dyn std::future::Future<Output = std::io::Result<Arc<dyn AsyncTcpStream>>> + Send + 'a>,
    > {
        self.metrics.tcp_connects.fetch_add(1, Ordering::Relaxed);
        if self.switches.fail_connect_tcp.load(Ordering::Relaxed) {
            self.metrics
                .tcp_connect_failures_injected
                .fetch_add(1, Ordering::Relaxed);
            return Box::pin(async move {
                Err(std::io::Error::new(
                    std::io::ErrorKind::ConnectionRefused,
                    "RuntimeFaultProbe: connect_tcp fault injected",
                ))
            });
        }
        self.inner.connect_tcp(remote_addr)
    }

    fn resolve_host<'a>(
        &'a self,
        host: &'a str,
    ) -> Pin<Box<dyn std::future::Future<Output = std::io::Result<Vec<SocketAddr>>> + Send + 'a>>
    {
        self.metrics
            .host_resolutions
            .fetch_add(1, Ordering::Relaxed);
        self.inner.resolve_host(host)
    }

    fn sleep(
        &self,
        duration: Duration,
    ) -> Pin<Box<dyn std::future::Future<Output = ()> + Send + 'static>> {
        self.metrics.sleeps.fetch_add(1, Ordering::Relaxed);
        self.inner.sleep(duration)
    }

    fn interval(&self, period: Duration) -> Box<dyn AsyncInterval> {
        self.metrics.intervals.fetch_add(1, Ordering::Relaxed);
        self.inner.interval(period)
    }

    fn block_on(&self, future: Pin<Box<dyn std::future::Future<Output = ()> + '_>>) {
        self.inner.block_on(future)
    }

    fn name(&self) -> &'static str {
        "runtime-fault-probe"
    }
}

/// A UDP socket that accepts sends (counted, then dropped) and never delivers
/// a receive — the "blackhole" fault from RFC Stage 5 item 10.
#[derive(Debug)]
struct FaultInjectedUdpSocket {
    inner: Arc<dyn AsyncUdpSocket>,
    metrics: Arc<ProbeMetrics>,
    switches: Arc<FaultSwitches>,
    blackhole_counted: AtomicBool,
}

impl FaultInjectedUdpSocket {
    fn blackholed(&self) -> bool {
        let blackholed = self.switches.blackhole_udp.load(Ordering::Relaxed);
        if blackholed && !self.blackhole_counted.swap(true, Ordering::Relaxed) {
            self.metrics
                .udp_sockets_blackholed
                .fetch_add(1, Ordering::Relaxed);
        }
        blackholed
    }
}

impl AsyncUdpSocket for FaultInjectedUdpSocket {
    fn local_addr(&self) -> std::io::Result<SocketAddr> {
        self.inner.local_addr()
    }

    fn poll_send(
        &self,
        cx: &mut Context<'_>,
        transmit: &Transmit<'_>,
    ) -> Poll<std::io::Result<usize>> {
        if !self.blackholed() {
            return self.inner.poll_send(cx, transmit);
        }
        // The datagram is "sent" from the caller's perspective and lost on the
        // wire — a blackhole must not surface a send error.
        self.metrics
            .blackholed_datagrams
            .fetch_add(1, Ordering::Relaxed);
        Poll::Ready(Ok(transmit.contents.len()))
    }

    fn poll_recv(
        &self,
        cx: &mut Context<'_>,
        bufs: &mut [IoSliceMut<'_>],
        meta: &mut [RecvMeta],
    ) -> Poll<std::io::Result<usize>> {
        if !self.blackholed() {
            return self.inner.poll_recv(cx, bufs, meta);
        }
        // Never ready: nothing ever arrives from a blackholed socket.
        Poll::Pending
    }

    fn max_gso_segments(&self) -> usize {
        self.inner.max_gso_segments()
    }

    fn max_gro_segments(&self) -> usize {
        self.inner.max_gro_segments()
    }
}

// ---- DeterministicNetwork --------------------------------------------------

/// The deterministic disposition of one outbound signaling message. Reorder is
/// expressed as a long `Delay` (a later message overtakes); partition is a
/// dynamic toggle on the decorator, not a per-message rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NetworkFault {
    /// Forward immediately.
    Deliver,
    /// Lose the message.
    Drop,
    /// Forward after the given delay (timer-facade sleep; inv24).
    Delay(Duration),
    /// Forward twice (the receiver's dedup/idempotency handles the repeat).
    Duplicate,
}

/// Per-decorator fault counters (monotonic).
#[derive(Debug, Default)]
pub(crate) struct NetworkFaultMetrics {
    pub delivered: AtomicUsize,
    pub dropped: AtomicUsize,
    pub delayed: AtomicUsize,
    pub duplicated: AtomicUsize,
    pub partition_dropped: AtomicUsize,
}

/// A deterministic per-message fault rule: classifies `(from, to, message)`
/// into a [`NetworkFault`]. A pure function, so a fixed rule replays
/// identically across runs.
pub(crate) type NetworkFaultRule =
    Box<dyn Fn(&ActrId, &ActrId, &SignalingMessage) -> NetworkFault + Send + Sync>;

/// A deterministic fault-injection `SignalingTransport` decorator: every
/// outbound message is classified by a test-supplied rule into
/// [`NetworkFault`]. The rule is a pure function of `(from, to, message)`, so
/// a fixed rule replays identically across runs — no randomness, no wall-clock
/// dependence beyond the explicit `Delay` duration.
///
/// This is the Actr-signaling-level seam only: deterministic loss/reorder at
/// the *protocol packet* level belongs to the Sans-I/O lab (Clock level C);
/// real-link faults belong to the system netns/`tc` harness.
pub(crate) struct DeterministicNetwork<T> {
    inner: Arc<T>,
    local_id: ActrId,
    rule: NetworkFaultRule,
    partitioned: AtomicBool,
    metrics: Arc<NetworkFaultMetrics>,
}

impl<T> DeterministicNetwork<T> {
    pub(crate) fn new(
        inner: Arc<T>,
        local_id: ActrId,
        rule: impl Fn(&ActrId, &ActrId, &SignalingMessage) -> NetworkFault + Send + Sync + 'static,
    ) -> Self {
        Self {
            inner,
            local_id,
            rule: Box::new(rule),
            partitioned: AtomicBool::new(false),
            metrics: Arc::new(NetworkFaultMetrics::default()),
        }
    }

    /// Cut the link: every outbound message is dropped until [`Self::heal`].
    pub(crate) fn partition(&self) {
        self.partitioned.store(true, Ordering::Relaxed);
    }

    /// Restore the link after [`Self::partition`].
    pub(crate) fn heal(&self) {
        self.partitioned.store(false, Ordering::Relaxed);
    }

    pub(crate) fn metrics(&self) -> &Arc<NetworkFaultMetrics> {
        &self.metrics
    }
}

#[async_trait::async_trait]
impl<T: SignalingTransport + 'static> SignalingTransport for DeterministicNetwork<T> {
    async fn send(&self, to: ActrId, message: SignalingMessage) {
        if self.partitioned.load(Ordering::Relaxed) {
            self.metrics
                .partition_dropped
                .fetch_add(1, Ordering::Relaxed);
            return;
        }
        match (self.rule)(&self.local_id, &to, &message) {
            NetworkFault::Deliver => {
                self.metrics.delivered.fetch_add(1, Ordering::Relaxed);
                self.inner.send(to, message).await;
            }
            NetworkFault::Drop => {
                self.metrics.dropped.fetch_add(1, Ordering::Relaxed);
            }
            NetworkFault::Delay(delay) => {
                self.metrics.delayed.fetch_add(1, Ordering::Relaxed);
                let inner = self.inner.clone();
                tokio::spawn(async move {
                    crate::timer::sleep(crate::timer::ids::RTC_TEST_FAULT_DELAY, delay).await;
                    inner.send(to, message).await;
                });
            }
            NetworkFault::Duplicate => {
                self.metrics.duplicated.fetch_add(1, Ordering::Relaxed);
                self.inner.send(to.clone(), message.clone()).await;
                self.inner.send(to, message).await;
            }
        }
    }

    async fn recv(&self) -> Option<(ActrId, SignalingMessage)> {
        self.inner.recv().await
    }
}

// ---- DirectedSignalingFaultRegistry ---------------------------------------

/// Stable message classes exposed to pair/system tests. The classification is
/// performed before encoding, so tests do not depend on protobuf internals.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SignalingMessageClass {
    Any,
    Offer,
    Answer,
    Candidate,
    IceRestartOffer,
    IceRestartRequest,
    IceRestartRejected,
    RoleNegotiation,
    RoleAssignment,
}

impl SignalingMessageClass {
    fn of(message: &SignalingMessage) -> Self {
        match message {
            SignalingMessage::Offer { .. } => Self::Offer,
            SignalingMessage::Answer { .. } => Self::Answer,
            SignalingMessage::Candidate(_) => Self::Candidate,
            SignalingMessage::IceRestartOffer { .. } => Self::IceRestartOffer,
            SignalingMessage::IceRestartRequest { .. } => Self::IceRestartRequest,
            SignalingMessage::IceRestartRejected { .. } => Self::IceRestartRejected,
            SignalingMessage::RoleNegotiation => Self::RoleNegotiation,
            SignalingMessage::RoleAssignment { .. } => Self::RoleAssignment,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignalingFaultRule {
    Deliver,
    Drop,
    Delay(Duration),
    /// Delay this message so a later message can overtake it.
    Reorder(Duration),
    Duplicate,
}

/// Ordered signaling evidence recorded before the external fault rule is
/// applied. Actrix remains the actual signaling server and transport peer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignalingTraceEvent {
    pub from: ActrId,
    pub to: ActrId,
    pub class: SignalingMessageClass,
    pub rule: SignalingFaultRule,
    /// One-based ordinal within this exact `(from, to, class)` stream.
    pub ordinal: usize,
    /// Restart exchange/request identity when the protocol message carries it.
    pub exchange_id: Option<String>,
    pub attempt: Option<u32>,
    pub observed_at: SystemTime,
    pub observed_after: Duration,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct SignalingFaultKey {
    from: ActrId,
    to: ActrId,
    class: SignalingMessageClass,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct SignalingNthFaultKey {
    key: SignalingFaultKey,
    ordinal: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct SignalingExchangeFaultKey {
    key: SignalingFaultKey,
    exchange_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct SignalingRouteKey {
    from: ActrId,
    to: ActrId,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DirectedSignalingFaultMetrics {
    pub delivered: usize,
    pub dropped: usize,
    pub delayed: usize,
    pub reordered: usize,
    pub duplicated: usize,
    pub deferred_inflight: usize,
}

/// Hit counters and the recorded message trace. Both live behind one lock so
/// an observer that sees a hit count also sees the matching trace entry (the
/// waiting helpers rely on this: `wait_for_hits_after` returns only after the
/// trace snapshot is guaranteed to include the hit's event).
#[derive(Debug, Default)]
struct SignalingObservation {
    hits: std::collections::HashMap<SignalingFaultKey, usize>,
    trace: Vec<SignalingTraceEvent>,
}

#[derive(Debug)]
struct DirectedSignalingFaultState {
    rules: Mutex<std::collections::HashMap<SignalingFaultKey, SignalingFaultRule>>,
    nth_rules: Mutex<std::collections::HashMap<SignalingNthFaultKey, SignalingFaultRule>>,
    exchange_rules: Mutex<std::collections::HashMap<SignalingExchangeFaultKey, SignalingFaultRule>>,
    /// Hit counters and the message trace share one lock so they become
    /// visible to waiting tests atomically: a test woken by `changed` after a
    /// hit count moves is guaranteed to also see the matching trace entry.
    observed: Mutex<SignalingObservation>,
    changed: tokio::sync::Notify,
    /// Cancellation is route-scoped: healing A-B must not cancel delayed
    /// signaling that belongs to A-C or any other independent peer link.
    deferred_cancels:
        Mutex<std::collections::HashMap<SignalingRouteKey, tokio_util::sync::CancellationToken>>,
    deferred_idle: tokio::sync::Notify,
    started_at: Instant,
    delivered: AtomicUsize,
    dropped: AtomicUsize,
    delayed: AtomicUsize,
    reordered: AtomicUsize,
    duplicated: AtomicUsize,
    deferred_inflight: AtomicUsize,
}

impl Default for DirectedSignalingFaultState {
    fn default() -> Self {
        Self {
            rules: Mutex::new(std::collections::HashMap::new()),
            nth_rules: Mutex::new(std::collections::HashMap::new()),
            exchange_rules: Mutex::new(std::collections::HashMap::new()),
            observed: Mutex::new(SignalingObservation::default()),
            changed: tokio::sync::Notify::new(),
            deferred_cancels: Mutex::new(std::collections::HashMap::new()),
            deferred_idle: tokio::sync::Notify::new(),
            started_at: Instant::now(),
            delivered: AtomicUsize::new(0),
            dropped: AtomicUsize::new(0),
            delayed: AtomicUsize::new(0),
            reordered: AtomicUsize::new(0),
            duplicated: AtomicUsize::new(0),
            deferred_inflight: AtomicUsize::new(0),
        }
    }
}

/// Shared dynamic fault policy around the production WebSocket signaling
/// adapter. Actrix remains the signaling server; this decorator is the RFC's
/// permitted external fault proxy at the Actr transport boundary.
#[derive(Debug, Clone, Default)]
pub struct DirectedSignalingFaultRegistry {
    state: Arc<DirectedSignalingFaultState>,
}

impl DirectedSignalingFaultRegistry {
    pub fn set_rule(
        &self,
        from: ActrId,
        to: ActrId,
        class: SignalingMessageClass,
        rule: SignalingFaultRule,
    ) {
        self.state
            .rules
            .lock()
            .expect("signaling fault registry lock poisoned")
            .insert(SignalingFaultKey { from, to, class }, rule);
    }

    /// Apply a rule only to the Nth message of one directed class stream.
    pub fn set_nth_rule(
        &self,
        from: ActrId,
        to: ActrId,
        class: SignalingMessageClass,
        ordinal: usize,
        rule: SignalingFaultRule,
    ) {
        assert!(ordinal > 0, "signaling ordinals are one-based");
        self.state
            .nth_rules
            .lock()
            .expect("signaling nth-rule lock poisoned")
            .insert(
                SignalingNthFaultKey {
                    key: SignalingFaultKey { from, to, class },
                    ordinal,
                },
                rule,
            );
    }

    /// Apply a rule only to a named restart exchange/request.
    pub fn set_exchange_rule(
        &self,
        from: ActrId,
        to: ActrId,
        class: SignalingMessageClass,
        exchange_id: impl Into<String>,
        rule: SignalingFaultRule,
    ) {
        self.state
            .exchange_rules
            .lock()
            .expect("signaling exchange-rule lock poisoned")
            .insert(
                SignalingExchangeFaultKey {
                    key: SignalingFaultKey { from, to, class },
                    exchange_id: exchange_id.into(),
                },
                rule,
            );
    }

    pub fn clear_rule(&self, from: &ActrId, to: &ActrId, class: SignalingMessageClass) {
        self.state
            .rules
            .lock()
            .expect("signaling fault registry lock poisoned")
            .remove(&SignalingFaultKey {
                from: from.clone(),
                to: to.clone(),
                class,
            });
    }

    pub fn heal_pair(&self, a: &ActrId, b: &ActrId) {
        self.state
            .rules
            .lock()
            .expect("signaling fault registry lock poisoned")
            .retain(|key, _| {
                !((&key.from == a && &key.to == b) || (&key.from == b && &key.to == a))
            });
        self.state
            .nth_rules
            .lock()
            .expect("signaling nth-rule lock poisoned")
            .retain(|selector, _| {
                !((&selector.key.from == a && &selector.key.to == b)
                    || (&selector.key.from == b && &selector.key.to == a))
            });
        self.state
            .exchange_rules
            .lock()
            .expect("signaling exchange-rule lock poisoned")
            .retain(|selector, _| {
                !((&selector.key.from == a && &selector.key.to == b)
                    || (&selector.key.from == b && &selector.key.to == a))
            });
        self.cancel_deferred_pair(a, b);
    }

    pub fn clear(&self) {
        self.state
            .rules
            .lock()
            .expect("signaling fault registry lock poisoned")
            .clear();
        self.state
            .nth_rules
            .lock()
            .expect("signaling nth-rule lock poisoned")
            .clear();
        self.state
            .exchange_rules
            .lock()
            .expect("signaling exchange-rule lock poisoned")
            .clear();
        self.cancel_all_deferred();
    }

    pub fn active_rule_count(&self) -> usize {
        self.state
            .rules
            .lock()
            .expect("signaling fault registry lock poisoned")
            .len()
            + self
                .state
                .nth_rules
                .lock()
                .expect("signaling nth-rule lock poisoned")
                .len()
            + self
                .state
                .exchange_rules
                .lock()
                .expect("signaling exchange-rule lock poisoned")
                .len()
    }

    fn deferred_token(&self, from: &ActrId, to: &ActrId) -> tokio_util::sync::CancellationToken {
        self.state
            .deferred_cancels
            .lock()
            .expect("signaling deferred-cancel lock poisoned")
            .entry(SignalingRouteKey {
                from: from.clone(),
                to: to.clone(),
            })
            .or_default()
            .clone()
    }

    fn cancel_deferred_pair(&self, a: &ActrId, b: &ActrId) {
        let mut cancels = self
            .state
            .deferred_cancels
            .lock()
            .expect("signaling deferred-cancel lock poisoned");
        for route in [
            SignalingRouteKey {
                from: a.clone(),
                to: b.clone(),
            },
            SignalingRouteKey {
                from: b.clone(),
                to: a.clone(),
            },
        ] {
            if let Some(cancel) = cancels.remove(&route) {
                cancel.cancel();
            }
        }
    }

    fn cancel_all_deferred(&self) {
        for (_, cancel) in self
            .state
            .deferred_cancels
            .lock()
            .expect("signaling deferred-cancel lock poisoned")
            .drain()
        {
            cancel.cancel();
        }
    }

    pub async fn wait_deferred_idle(&self, timeout: Duration) -> bool {
        crate::timer::timeout(crate::timer::ids::WEBRTC_PEER_SENDABLE, timeout, async {
            loop {
                if self.state.deferred_inflight.load(Ordering::Acquire) == 0 {
                    return;
                }
                self.state.deferred_idle.notified().await;
            }
        })
        .await
        .is_ok()
    }

    pub fn partition_pair(&self, a: &ActrId, b: &ActrId) {
        self.set_rule(
            a.clone(),
            b.clone(),
            SignalingMessageClass::Any,
            SignalingFaultRule::Drop,
        );
        self.set_rule(
            b.clone(),
            a.clone(),
            SignalingMessageClass::Any,
            SignalingFaultRule::Drop,
        );
    }

    pub fn metrics(&self) -> DirectedSignalingFaultMetrics {
        DirectedSignalingFaultMetrics {
            delivered: self.state.delivered.load(Ordering::Acquire),
            dropped: self.state.dropped.load(Ordering::Acquire),
            delayed: self.state.delayed.load(Ordering::Acquire),
            reordered: self.state.reordered.load(Ordering::Acquire),
            duplicated: self.state.duplicated.load(Ordering::Acquire),
            deferred_inflight: self.state.deferred_inflight.load(Ordering::Acquire),
        }
    }

    pub fn hits(&self, from: &ActrId, to: &ActrId, class: SignalingMessageClass) -> usize {
        self.state
            .observed
            .lock()
            .expect("signaling observation lock poisoned")
            .hits
            .get(&SignalingFaultKey {
                from: from.clone(),
                to: to.clone(),
                class,
            })
            .copied()
            .unwrap_or(0)
    }

    pub fn trace(&self) -> Vec<SignalingTraceEvent> {
        self.state
            .observed
            .lock()
            .expect("signaling observation lock poisoned")
            .trace
            .clone()
    }

    pub async fn wait_for_hits_after(
        &self,
        from: &ActrId,
        to: &ActrId,
        class: SignalingMessageClass,
        baseline: usize,
        timeout: Duration,
    ) -> Option<usize> {
        crate::timer::timeout(crate::timer::ids::WEBRTC_PEER_SENDABLE, timeout, async {
            loop {
                // Register the Notified future BEFORE reading the counter: a
                // notification that fires between the read and the first poll
                // of `notified()` can be consumed by a concurrent waiter's
                // future, leaving this waiter parked until an unrelated later
                // hit. `enable()` closes that window; the future is rebuilt on
                // every wakeup so each loop iteration re-registers first.
                let notified = self.state.changed.notified();
                let mut notified = std::pin::pin!(notified);
                notified.as_mut().enable();
                let hits = self.hits(from, to, class);
                if hits > baseline {
                    return hits;
                }
                notified.await;
            }
        })
        .await
        .ok()
    }

    /// Resolve the fault rule for one message and record its observation.
    /// The hit counter increment and the trace entry are committed under the
    /// same `observed` lock, and `changed` is notified only after both are
    /// visible — so a test woken by `wait_for_hits_after` is guaranteed that
    /// its next `trace()` snapshot includes the hit's event.
    ///
    /// Lock order: `observed` → `exchange_rules` → `nth_rules` → `rules`.
    /// Every other accessor takes at most one of these locks, so the nesting
    /// cannot deadlock.
    fn decision_and_record(
        &self,
        from: &ActrId,
        to: &ActrId,
        message: &SignalingMessage,
    ) -> SignalingDecision {
        let class = SignalingMessageClass::of(message);
        let exact = SignalingFaultKey {
            from: from.clone(),
            to: to.clone(),
            class,
        };
        let any = SignalingFaultKey {
            from: from.clone(),
            to: to.clone(),
            class: SignalingMessageClass::Any,
        };
        let (exchange_id, attempt) = signaling_identity(message);

        let mut observed = self
            .state
            .observed
            .lock()
            .expect("signaling observation lock poisoned");
        let ordinal = {
            let count = observed.hits.entry(exact.clone()).or_default();
            *count += 1;
            *count
        };
        let any_ordinal = if class == SignalingMessageClass::Any {
            ordinal
        } else {
            let count = observed.hits.entry(any.clone()).or_default();
            *count += 1;
            *count
        };

        let exchange_rules = self
            .state
            .exchange_rules
            .lock()
            .expect("signaling exchange-rule lock poisoned");
        let exchange_rule = exchange_id.as_ref().and_then(|exchange_id| {
            exchange_rules
                .get(&SignalingExchangeFaultKey {
                    key: exact.clone(),
                    exchange_id: exchange_id.clone(),
                })
                .or_else(|| {
                    exchange_rules.get(&SignalingExchangeFaultKey {
                        key: any.clone(),
                        exchange_id: exchange_id.clone(),
                    })
                })
                .copied()
        });
        drop(exchange_rules);

        let nth_rules = self
            .state
            .nth_rules
            .lock()
            .expect("signaling nth-rule lock poisoned");
        let nth_rule = nth_rules
            .get(&SignalingNthFaultKey {
                key: exact.clone(),
                ordinal,
            })
            .or_else(|| {
                nth_rules.get(&SignalingNthFaultKey {
                    key: any.clone(),
                    ordinal: any_ordinal,
                })
            })
            .copied();
        drop(nth_rules);

        let rules = self
            .state
            .rules
            .lock()
            .expect("signaling fault registry lock poisoned");
        let rule = exchange_rule
            .or(nth_rule)
            .or_else(|| rules.get(&exact).copied())
            .or_else(|| rules.get(&any).copied())
            .unwrap_or(SignalingFaultRule::Deliver);
        drop(rules);

        observed.trace.push(SignalingTraceEvent {
            from: from.clone(),
            to: to.clone(),
            class,
            rule,
            ordinal,
            exchange_id: exchange_id.clone(),
            attempt,
            observed_at: SystemTime::now(),
            observed_after: self.state.started_at.elapsed(),
        });
        drop(observed);
        self.state.changed.notify_one();
        SignalingDecision {
            class,
            rule,
            ordinal,
            exchange_id,
            attempt,
        }
    }
}

struct SignalingDecision {
    class: SignalingMessageClass,
    rule: SignalingFaultRule,
    ordinal: usize,
    exchange_id: Option<String>,
    attempt: Option<u32>,
}

fn signaling_identity(message: &SignalingMessage) -> (Option<String>, Option<u32>) {
    match message {
        SignalingMessage::IceRestartOffer { exchange_id, .. }
        | SignalingMessage::IceRestartRejected { exchange_id, .. } => {
            (Some(exchange_id.clone()), None)
        }
        SignalingMessage::IceRestartRequest {
            request_id,
            attempt,
        } => (Some(request_id.clone()), Some(*attempt)),
        _ => (None, None),
    }
}

pub(crate) struct DirectedSignalingTransport<T> {
    inner: Arc<T>,
    local_id: ActrId,
    faults: DirectedSignalingFaultRegistry,
}

impl<T> DirectedSignalingTransport<T> {
    pub(crate) fn new(
        inner: Arc<T>,
        local_id: ActrId,
        faults: DirectedSignalingFaultRegistry,
    ) -> Self {
        Self {
            inner,
            local_id,
            faults,
        }
    }
}

#[async_trait::async_trait]
impl<T: SignalingTransport + 'static> SignalingTransport for DirectedSignalingTransport<T> {
    async fn send(&self, to: ActrId, message: SignalingMessage) {
        let decision = self
            .faults
            .decision_and_record(&self.local_id, &to, &message);
        match decision.rule {
            SignalingFaultRule::Deliver => {
                self.faults.state.delivered.fetch_add(1, Ordering::Relaxed);
                self.inner.send(to, message).await;
            }
            SignalingFaultRule::Drop => {
                self.faults.state.dropped.fetch_add(1, Ordering::Relaxed);
            }
            SignalingFaultRule::Delay(delay) => {
                self.faults.state.delayed.fetch_add(1, Ordering::Relaxed);
                let inner = self.inner.clone();
                let state = self.faults.state.clone();
                let cancel = self.faults.deferred_token(&self.local_id, &to);
                state.deferred_inflight.fetch_add(1, Ordering::AcqRel);
                tokio::spawn(async move {
                    tokio::select! {
                        _ = cancel.cancelled() => {}
                        _ = crate::timer::sleep(crate::timer::ids::RTC_TEST_FAULT_DELAY, delay) => {
                            inner.send(to, message).await;
                        }
                    }
                    state.deferred_inflight.fetch_sub(1, Ordering::AcqRel);
                    state.deferred_idle.notify_one();
                });
            }
            SignalingFaultRule::Reorder(delay) => {
                self.faults.state.reordered.fetch_add(1, Ordering::Relaxed);
                let inner = self.inner.clone();
                let state = self.faults.state.clone();
                let cancel = self.faults.deferred_token(&self.local_id, &to);
                state.deferred_inflight.fetch_add(1, Ordering::AcqRel);
                tokio::spawn(async move {
                    tokio::select! {
                        _ = cancel.cancelled() => {}
                        _ = crate::timer::sleep(crate::timer::ids::RTC_TEST_FAULT_DELAY, delay) => {
                            inner.send(to, message).await;
                        }
                    }
                    state.deferred_inflight.fetch_sub(1, Ordering::AcqRel);
                    state.deferred_idle.notify_one();
                });
            }
            SignalingFaultRule::Duplicate => {
                self.faults.state.duplicated.fetch_add(1, Ordering::Relaxed);
                self.inner.send(to.clone(), message.clone()).await;
                self.inner.send(to, message).await;
            }
        }
    }

    async fn recv(&self) -> Option<(ActrId, SignalingMessage)> {
        self.inner.recv().await
    }
}

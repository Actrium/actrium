//! Tests for the Stage 5 item 10 fault-injection seams: `RuntimeFaultProbe`
//! boundary metrics + fault switches, and `DeterministicNetwork` deterministic
//! loss/delay/duplicate/partition rules.

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use actr_protocol::ActrId;
use webrtc::runtime::{Runtime, default_runtime};

use crate::wire::webrtc::v020::fault::{
    DeterministicNetwork, DirectedSignalingFaultRegistry, DirectedSignalingTransport, NetworkFault,
    RuntimeFaultProbe, SignalingFaultRule, SignalingMessageClass,
};
use crate::wire::webrtc::v020::peer::reducer::SignalingMessage;
use crate::wire::webrtc::v020::signaling::in_memory::InMemorySignalingHub;
use crate::wire::webrtc::v020::signaling::router::SignalingTransport;

const WAIT: Duration = Duration::from_secs(5);

fn test_actor_id(serial_number: u64) -> ActrId {
    ActrId {
        realm: "test@example.com".to_string(),
        serial_number,
    }
}

// ---- RuntimeFaultProbe -----------------------------------------------------

#[tokio::test]
async fn probe_counts_boundary_crossings() {
    let probe = RuntimeFaultProbe::new(default_runtime().expect("tokio runtime"));

    probe.spawn(Box::pin(async {})).detach();
    probe.sleep(Duration::from_millis(1)).await;
    let socket = std::net::UdpSocket::bind("127.0.0.1:0").expect("bind udp");
    probe.wrap_udp_socket(socket).expect("wrap udp");
    let _ = probe.resolve_host("localhost:3478").await;

    let metrics = probe.metrics();
    assert_eq!(metrics.spawned_tasks.load(Ordering::Relaxed), 1);
    assert_eq!(metrics.sleeps.load(Ordering::Relaxed), 1);
    assert_eq!(metrics.udp_sockets_wrapped.load(Ordering::Relaxed), 1);
    assert_eq!(metrics.host_resolutions.load(Ordering::Relaxed), 1);
    // No faults armed: nothing injected.
    assert_eq!(metrics.spawned_tasks_killed.load(Ordering::Relaxed), 0);
    assert_eq!(metrics.udp_sockets_blackholed.load(Ordering::Relaxed), 0);
}

#[tokio::test]
async fn connect_tcp_fault_fails_fast_and_counts() {
    let probe = RuntimeFaultProbe::new(default_runtime().expect("tokio runtime"));
    probe.switches().set_fail_connect_tcp(true);

    let result = probe.connect_tcp("127.0.0.1:9".parse().unwrap()).await;
    let err = result.expect_err("injected fault must fail the connect");
    assert_eq!(err.kind(), std::io::ErrorKind::ConnectionRefused);
    assert_eq!(probe.metrics().tcp_connects.load(Ordering::Relaxed), 1);
    assert_eq!(
        probe
            .metrics()
            .tcp_connect_failures_injected
            .load(Ordering::Relaxed),
        1
    );
}

#[tokio::test]
async fn kill_spawned_tasks_exits_the_driver_immediately() {
    let probe = RuntimeFaultProbe::new(default_runtime().expect("tokio runtime"));

    // Kill switch OFF: a never-completing task stays unfinished.
    let live = probe.spawn(Box::pin(std::future::pending::<()>()));
    assert!(!live.is_finished());

    // Kill switch ON: the spawned future is replaced by an immediately
    // exiting one, so the handle reports finished as soon as it is polled.
    probe.switches().set_kill_spawned_tasks(true);
    let killed = probe.spawn(Box::pin(std::future::pending::<()>()));
    tokio::time::timeout(WAIT, async {
        while !killed.is_finished() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("killed task should finish immediately");
    assert_eq!(
        probe.metrics().spawned_tasks_killed.load(Ordering::Relaxed),
        1
    );

    live.abort();
}

#[tokio::test]
async fn blackholed_udp_socket_drops_sends_and_never_receives() {
    let probe = RuntimeFaultProbe::new(default_runtime().expect("tokio runtime"));
    probe.switches().set_blackhole_udp(true);

    let socket = std::net::UdpSocket::bind("127.0.0.1:0").expect("bind udp");
    let socket = probe.wrap_udp_socket(socket).expect("wrap udp");
    assert_eq!(
        probe
            .metrics()
            .udp_sockets_blackholed
            .load(Ordering::Relaxed),
        1
    );

    // Sends succeed (accepted then dropped) and are counted.
    let sent = socket
        .send_to(b"lost-datagram", "127.0.0.1:9".parse().unwrap())
        .await
        .expect("blackhole accepts sends");
    assert_eq!(sent, 13);
    assert_eq!(
        probe.metrics().blackholed_datagrams.load(Ordering::Relaxed),
        1
    );

    // Receives never complete.
    let recv = async {
        let mut buf = [0u8; 16];
        socket.recv_from(&mut buf).await
    };
    tokio::time::timeout(Duration::from_millis(200), recv)
        .await
        .expect_err("a blackholed socket must never deliver a receive");
}

#[tokio::test]
async fn existing_udp_socket_can_be_blackholed_and_healed_mid_session() {
    let probe = RuntimeFaultProbe::new(default_runtime().expect("tokio runtime"));
    let sender = std::net::UdpSocket::bind("127.0.0.1:0").expect("bind sender");
    let sender = probe.wrap_udp_socket(sender).expect("wrap sender");
    let receiver = tokio::net::UdpSocket::bind("127.0.0.1:0")
        .await
        .expect("bind receiver");
    let target = receiver.local_addr().expect("receiver address");

    probe.switches().set_blackhole_udp(true);
    sender
        .send_to(b"lost", target)
        .await
        .expect("blackhole accepts send");
    let mut buf = [0u8; 16];
    tokio::time::timeout(Duration::from_millis(100), receiver.recv_from(&mut buf))
        .await
        .expect_err("mid-session blackhole must drop the datagram");
    assert_eq!(
        probe
            .metrics()
            .udp_sockets_blackholed
            .load(Ordering::Relaxed),
        1
    );

    probe.switches().set_blackhole_udp(false);
    sender
        .send_to(b"healed", target)
        .await
        .expect("healed socket sends");
    let (n, _) = tokio::time::timeout(WAIT, receiver.recv_from(&mut buf))
        .await
        .expect("healed socket did not deliver")
        .expect("receiver error");
    assert_eq!(&buf[..n], b"healed");
}

// ---- DeterministicNetwork --------------------------------------------------

fn network(
    hub: &Arc<InMemorySignalingHub>,
    local_id: ActrId,
    rule: impl Fn(&ActrId, &ActrId, &SignalingMessage) -> NetworkFault + Send + Sync + 'static,
) -> DeterministicNetwork<crate::wire::webrtc::v020::signaling::in_memory::InMemoryTransport> {
    DeterministicNetwork::new(Arc::new(hub.connect(local_id.clone())), local_id, rule)
}

#[tokio::test]
async fn drop_rule_loses_the_message_deterministically() {
    let hub = InMemorySignalingHub::new();
    let (a, b) = (test_actor_id(1), test_actor_id(2));
    let net_a = network(&hub, a.clone(), |_, _, _| NetworkFault::Drop);
    let transport_b = hub.connect(b.clone());

    let _ = net_a
        .send(b, SignalingMessage::RoleAssignment { is_offerer: true })
        .await;
    let recv = async { transport_b.recv().await };
    tokio::time::timeout(Duration::from_millis(200), recv)
        .await
        .expect_err("a dropped message must not arrive");
    assert_eq!(net_a.metrics().dropped.load(Ordering::Relaxed), 1);
}

#[tokio::test]
async fn duplicate_rule_delivers_twice() {
    let hub = InMemorySignalingHub::new();
    let (a, b) = (test_actor_id(1), test_actor_id(2));
    let net_a = network(&hub, a.clone(), |_, _, _| NetworkFault::Duplicate);
    let transport_b = hub.connect(b.clone());

    let _ = net_a
        .send(b, SignalingMessage::RoleAssignment { is_offerer: false })
        .await;
    for i in 0..2 {
        let got = tokio::time::timeout(WAIT, transport_b.recv())
            .await
            .unwrap_or_else(|_| panic!("duplicate copy {i} did not arrive"));
        assert!(matches!(
            got,
            Some((_, SignalingMessage::RoleAssignment { is_offerer: false }))
        ));
    }
    assert_eq!(net_a.metrics().duplicated.load(Ordering::Relaxed), 1);
}

#[tokio::test]
async fn delay_rule_delivers_late_via_the_timer_facade() {
    let hub = InMemorySignalingHub::new();
    let (a, b) = (test_actor_id(1), test_actor_id(2));
    let net_a = network(&hub, a.clone(), |_, _, _| {
        NetworkFault::Delay(Duration::from_millis(50))
    });
    let transport_b = hub.connect(b.clone());

    let _ = net_a
        .send(b, SignalingMessage::RoleAssignment { is_offerer: true })
        .await;
    let got = tokio::time::timeout(WAIT, transport_b.recv())
        .await
        .expect("delayed message did not arrive");
    assert!(got.is_some());
    assert_eq!(net_a.metrics().delayed.load(Ordering::Relaxed), 1);
}

#[tokio::test]
async fn partition_drops_until_healed() {
    let hub = InMemorySignalingHub::new();
    let (a, b) = (test_actor_id(1), test_actor_id(2));
    let net_a = network(&hub, a.clone(), |_, _, _| NetworkFault::Deliver);
    let transport_b = hub.connect(b.clone());

    net_a.partition();
    let _ = net_a
        .send(
            b.clone(),
            SignalingMessage::RoleAssignment { is_offerer: true },
        )
        .await;
    let recv = async { transport_b.recv().await };
    tokio::time::timeout(Duration::from_millis(200), recv)
        .await
        .expect_err("a partitioned link must not deliver");
    assert_eq!(net_a.metrics().partition_dropped.load(Ordering::Relaxed), 1);

    net_a.heal();
    let _ = net_a
        .send(b, SignalingMessage::RoleAssignment { is_offerer: false })
        .await;
    let got = tokio::time::timeout(WAIT, transport_b.recv())
        .await
        .expect("healed link did not deliver");
    assert!(got.is_some());
    assert_eq!(net_a.metrics().delivered.load(Ordering::Relaxed), 1);
}

#[tokio::test]
async fn directed_signaling_fault_records_ordered_typed_evidence() {
    let hub = InMemorySignalingHub::new();
    let (a, b) = (test_actor_id(31), test_actor_id(32));
    let faults = DirectedSignalingFaultRegistry::default();
    faults.set_rule(
        a.clone(),
        b.clone(),
        SignalingMessageClass::RoleAssignment,
        SignalingFaultRule::Drop,
    );
    let transport_a = DirectedSignalingTransport::new(
        Arc::new(hub.connect(a.clone())),
        a.clone(),
        faults.clone(),
    );
    let transport_b = hub.connect(b.clone());

    let _ = transport_a
        .send(
            b.clone(),
            SignalingMessage::RoleAssignment { is_offerer: true },
        )
        .await;
    tokio::time::timeout(Duration::from_millis(100), transport_b.recv())
        .await
        .expect_err("directed drop must not reach the remote transport");
    assert_eq!(faults.metrics().dropped, 1);
    assert_eq!(
        faults.hits(&a, &b, SignalingMessageClass::RoleAssignment),
        1
    );
    let trace = faults.trace();
    assert_eq!(trace.len(), 1);
    assert_eq!(trace[0].from, a);
    assert_eq!(trace[0].to, b);
    assert_eq!(trace[0].class, SignalingMessageClass::RoleAssignment);
    assert_eq!(trace[0].rule, SignalingFaultRule::Drop);
    assert_eq!(trace[0].ordinal, 1);
    assert_eq!(trace[0].exchange_id, None);
}

#[tokio::test]
async fn directed_signaling_nth_rule_targets_only_the_selected_message() {
    let hub = InMemorySignalingHub::new();
    let (a, b) = (test_actor_id(33), test_actor_id(34));
    let faults = DirectedSignalingFaultRegistry::default();
    faults.set_nth_rule(
        a.clone(),
        b.clone(),
        SignalingMessageClass::RoleAssignment,
        2,
        SignalingFaultRule::Drop,
    );
    let transport_a = DirectedSignalingTransport::new(
        Arc::new(hub.connect(a.clone())),
        a.clone(),
        faults.clone(),
    );
    let transport_b = hub.connect(b.clone());

    let _ = transport_a
        .send(
            b.clone(),
            SignalingMessage::RoleAssignment { is_offerer: true },
        )
        .await;
    let _ = transport_a
        .send(
            b.clone(),
            SignalingMessage::RoleAssignment { is_offerer: false },
        )
        .await;

    assert!(
        tokio::time::timeout(WAIT, transport_b.recv())
            .await
            .unwrap()
            .is_some()
    );
    tokio::time::timeout(Duration::from_millis(100), transport_b.recv())
        .await
        .expect_err("only the second directed message must be dropped");
    let trace = faults.trace();
    assert_eq!(
        trace.iter().map(|event| event.ordinal).collect::<Vec<_>>(),
        vec![1, 2]
    );
    assert_eq!(trace[0].rule, SignalingFaultRule::Deliver);
    assert_eq!(trace[1].rule, SignalingFaultRule::Drop);
}

#[tokio::test]
async fn directed_signaling_exchange_rule_records_request_identity_and_attempt() {
    let hub = InMemorySignalingHub::new();
    let (a, b) = (test_actor_id(35), test_actor_id(36));
    let faults = DirectedSignalingFaultRegistry::default();
    faults.set_exchange_rule(
        a.clone(),
        b.clone(),
        SignalingMessageClass::IceRestartRequest,
        "restart-7",
        SignalingFaultRule::Drop,
    );
    let transport_a = DirectedSignalingTransport::new(
        Arc::new(hub.connect(a.clone())),
        a.clone(),
        faults.clone(),
    );
    let transport_b = hub.connect(b.clone());

    let _ = transport_a
        .send(
            b.clone(),
            SignalingMessage::IceRestartRequest {
                request_id: "restart-7".to_string(),
                attempt: 3,
            },
        )
        .await;
    tokio::time::timeout(Duration::from_millis(100), transport_b.recv())
        .await
        .expect_err("the selected restart exchange must be dropped");

    let trace = faults.trace();
    assert_eq!(trace.len(), 1);
    assert_eq!(trace[0].exchange_id.as_deref(), Some("restart-7"));
    assert_eq!(trace[0].attempt, Some(3));
    assert_eq!(trace[0].rule, SignalingFaultRule::Drop);
}

#[tokio::test]
async fn directed_signaling_fault_can_drop_restart_request_ack() {
    let hub = InMemorySignalingHub::new();
    let (a, b) = (test_actor_id(39), test_actor_id(40));
    let faults = DirectedSignalingFaultRegistry::default();
    faults.set_rule(
        a.clone(),
        b.clone(),
        SignalingMessageClass::IceRestartRequestAck,
        SignalingFaultRule::Drop,
    );
    let transport_a = DirectedSignalingTransport::new(
        Arc::new(hub.connect(a.clone())),
        a.clone(),
        faults.clone(),
    );
    let transport_b = hub.connect(b.clone());

    transport_a
        .send(
            b,
            SignalingMessage::IceRestartRequestAck {
                request_id: "ack-drop".to_string(),
                attempt: 1,
                outcome: actr_protocol::ice_restart_request_ack::Outcome::NoActiveSession as i32,
            },
        )
        .await
        .expect("fault proxy accepts dropped Ack");
    tokio::time::timeout(Duration::from_millis(100), transport_b.recv())
        .await
        .expect_err("dropped Ack must not reach the receiver");
    assert_eq!(
        faults.trace()[0].class,
        SignalingMessageClass::IceRestartRequestAck
    );
}

#[tokio::test]
async fn directed_signaling_fault_can_duplicate_restart_request_ack() {
    let hub = InMemorySignalingHub::new();
    let (a, b) = (test_actor_id(41), test_actor_id(42));
    let faults = DirectedSignalingFaultRegistry::default();
    faults.set_rule(
        a.clone(),
        b.clone(),
        SignalingMessageClass::IceRestartRequestAck,
        SignalingFaultRule::Duplicate,
    );
    let transport_a =
        DirectedSignalingTransport::new(Arc::new(hub.connect(a.clone())), a, faults.clone());
    let transport_b = hub.connect(b.clone());

    transport_a
        .send(
            b,
            SignalingMessage::IceRestartRequestAck {
                request_id: "ack-duplicate".to_string(),
                attempt: 2,
                outcome: actr_protocol::ice_restart_request_ack::Outcome::Accepted as i32,
            },
        )
        .await
        .expect("fault proxy accepts duplicated Ack");
    for _ in 0..2 {
        assert!(matches!(
            tokio::time::timeout(WAIT, transport_b.recv())
                .await
                .expect("duplicated Ack timeout"),
            Some((_, SignalingMessage::IceRestartRequestAck { .. }))
        ));
    }
    tokio::time::timeout(Duration::from_millis(100), transport_b.recv())
        .await
        .expect_err("duplicate rule must emit exactly two Ack messages");
    assert_eq!(
        faults.trace()[0].class,
        SignalingMessageClass::IceRestartRequestAck
    );
}

#[tokio::test]
async fn directed_signaling_reorder_allows_the_next_message_to_overtake() {
    let hub = InMemorySignalingHub::new();
    let (a, b) = (test_actor_id(37), test_actor_id(38));
    let faults = DirectedSignalingFaultRegistry::default();
    faults.set_nth_rule(
        a.clone(),
        b.clone(),
        SignalingMessageClass::RoleAssignment,
        1,
        SignalingFaultRule::Reorder(Duration::from_millis(50)),
    );
    let transport_a =
        DirectedSignalingTransport::new(Arc::new(hub.connect(a.clone())), a, faults.clone());
    let transport_b = hub.connect(b.clone());

    let _ = transport_a
        .send(
            b.clone(),
            SignalingMessage::RoleAssignment { is_offerer: true },
        )
        .await;
    let _ = transport_a
        .send(b, SignalingMessage::RoleAssignment { is_offerer: false })
        .await;

    let (_, first) = tokio::time::timeout(WAIT, transport_b.recv())
        .await
        .expect("overtaking message deadline")
        .expect("overtaking message missing");
    let (_, second) = tokio::time::timeout(WAIT, transport_b.recv())
        .await
        .expect("reordered message deadline")
        .expect("reordered message missing");
    assert!(matches!(
        first,
        SignalingMessage::RoleAssignment { is_offerer: false }
    ));
    assert!(matches!(
        second,
        SignalingMessage::RoleAssignment { is_offerer: true }
    ));
    assert_eq!(faults.metrics().reordered, 1);
}

#[tokio::test]
async fn healing_one_signaling_pair_does_not_cancel_another_pairs_deferred_message() {
    let hub = InMemorySignalingHub::new();
    let (a, b, c) = (test_actor_id(39), test_actor_id(40), test_actor_id(41));
    let faults = DirectedSignalingFaultRegistry::default();
    for target in [&b, &c] {
        faults.set_rule(
            a.clone(),
            target.clone(),
            SignalingMessageClass::RoleAssignment,
            SignalingFaultRule::Delay(Duration::from_millis(50)),
        );
    }
    let transport_a = DirectedSignalingTransport::new(
        Arc::new(hub.connect(a.clone())),
        a.clone(),
        faults.clone(),
    );
    let transport_b = hub.connect(b.clone());
    let transport_c = hub.connect(c.clone());

    let _ = transport_a
        .send(
            b.clone(),
            SignalingMessage::RoleAssignment { is_offerer: true },
        )
        .await;
    let _ = transport_a
        .send(c, SignalingMessage::RoleAssignment { is_offerer: false })
        .await;
    faults.heal_pair(&a, &b);

    let delivered_to_c = tokio::time::timeout(WAIT, transport_c.recv())
        .await
        .expect("healing A-B must not cancel A-C's deferred message")
        .expect("A-C deferred message missing");
    assert!(matches!(
        delivered_to_c.1,
        SignalingMessage::RoleAssignment { is_offerer: false }
    ));
    tokio::time::timeout(Duration::from_millis(100), transport_b.recv())
        .await
        .expect_err("A-B's deferred message must be cancelled by pair healing");
    assert!(faults.wait_deferred_idle(WAIT).await);
}

/// The probe composes with the factory: the runtime override flows into PC
/// construction (this is the injection point contract tests use).
#[tokio::test]
async fn runtime_override_flows_into_peer_connection_build() {
    use crate::wire::webrtc::v020::address::DesktopLocalAddressProvider;
    use crate::wire::webrtc::v020::event_handler::WebRtcEventHandler;
    use crate::wire::webrtc::v020::events::peer_event_mailbox;
    use crate::wire::webrtc::v020::factory::PeerConnectionFactory;
    use crate::wire::webrtc::v020::runtime::RtcRuntimeConfig;

    let probe = Arc::new(RuntimeFaultProbe::new(
        default_runtime().expect("tokio runtime"),
    ));
    let config = RtcRuntimeConfig::desktop_default().with_runtime_override(probe.clone());
    let factory = PeerConnectionFactory::new(
        config,
        Arc::new(DesktopLocalAddressProvider::with_override(vec![
            std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
        ])),
        Vec::new(),
    );

    let (mailbox, _rx) = peer_event_mailbox();
    let handler = Arc::new(WebRtcEventHandler::new(test_actor_id(1), 1, mailbox));
    let pc = factory
        .build(handler)
        .await
        .expect("build through the probe");
    pc.close().await.expect("close");

    // Building a real PC wrapped at least one UDP socket and spawned driver
    // tasks through the probe.
    assert!(
        probe.metrics().udp_sockets_wrapped.load(Ordering::Relaxed) >= 1,
        "PC build should wrap a UDP socket through the probe"
    );
    assert!(
        probe.metrics().spawned_tasks.load(Ordering::Relaxed) >= 1,
        "PC build should spawn driver tasks through the probe"
    );
}

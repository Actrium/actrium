//! Production Node/Context/Mailbox/Workload/response path, real Loran signaling.
use crate::connection_governance_support as support;

use actr_framework::Bytes;
use actr_hyper::test_support::TestRtcRole;
use actr_protocol::PayloadType;
use std::time::Duration;
use support::{Nodes, WAIT};

#[tokio::test(flavor = "multi_thread", worker_threads = 6)]
async fn production_recovery_waiter_peer_limit_releases_capacity_after_foreground() {
    let pair = Nodes::new(TestRtcRole::Answerer).await;
    pair.background(0).await;
    let mut calls = tokio::task::JoinSet::new();
    for index in 0..65 {
        let node = pair.nodes[0].clone();
        let dest = actr_framework::Dest::Peer(pair.nodes[1].actor_id());
        calls.spawn(async move {
            node.app_context()
                .await
                .call_raw(
                    &dest,
                    "capacity".into(),
                    PayloadType::RpcReliable,
                    Bytes::from(format!("operation-{index}")),
                    10_000,
                )
                .await
        });
    }
    let refused = tokio::time::timeout(Duration::from_secs(3), calls.join_next())
        .await
        .expect("65th waiter must fail without waiting for the 10s business deadline")
        .unwrap()
        .unwrap();
    assert!(
        matches!(refused, Err(actr_protocol::ActrError::Unavailable(ref cause)) if cause.contains("waiter capacity")),
        "{refused:?}"
    );
    assert_eq!(pair.journals[1].count("capacity"), 0);
    pair.foreground(0, 5_000).await;
    tokio::time::timeout(WAIT, async {
        while let Some(result) = calls.join_next().await {
            result
                .unwrap()
                .expect("admitted waiter must send after recovery");
        }
    })
    .await
    .expect("all admitted operations complete");
    assert_eq!(pair.journals[1].count("capacity"), 64);
    pair.echo(0, "capacity-was-released").await;
    pair.shutdown().await;
}

async fn payloads_after_replacement(role: TestRtcRole) {
    let pair = Nodes::new(role).await;
    let before = pair.ready().await;
    pair.rtc[0]
        .close_lane(&pair.nodes[1].actor_id(), PayloadType::RpcReliable)
        .await
        .unwrap();
    for (i, prior) in before.iter().enumerate() {
        pair.rtc[i]
            .wait_state(&pair.nodes[1 - i].actor_id(), WAIT, |s| {
                s.sendable && s.pc_generation > prior.pc_generation
            })
            .await
            .expect("replacement before new payloads");
    }
    for from in 0..2 {
        let ctx = pair.nodes[from].app_context().await;
        let dest = actr_framework::Dest::Peer(pair.nodes[1 - from].actor_id());
        for lane in [PayloadType::RpcReliable, PayloadType::RpcSignal] {
            let key = format!("tell-after-replace-{from}-{lane:?}");
            ctx.tell_raw(&dest, key.clone(), lane, Bytes::from_static(b"tell-body"))
                .await
                .unwrap();
            pair.journals[1 - from].wait(&key, 1).await;
            assert_eq!(pair.journals[1 - from].count(&key), 1);
            let response = ctx
                .call_raw(
                    &dest,
                    "signal-call".into(),
                    lane,
                    Bytes::from_static(b"signal"),
                    5_000,
                )
                .await
                .unwrap();
            assert_eq!(response, Bytes::from_static(b"signal"));
        }
        for lane in [PayloadType::StreamReliable, PayloadType::StreamLatencyFirst] {
            let stream = format!("new-lane-{from}-{lane:?}");
            pair.call(from, "open-stream", Bytes::from(stream.clone()), 5_000)
                .await
                .unwrap();
            // Includes the first message on the newly discovered remote lane.
            for seq in 1..=8 {
                pair.stream(from, &stream, seq, lane).await;
            }
            pair.call(from, "finish", Bytes::from(stream), 5_000)
                .await
                .unwrap();
        }
    }
    pair.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn production_offerer_replacement_delivers_tell_signal_and_both_stream_lanes() {
    payloads_after_replacement(TestRtcRole::Offerer).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn production_answerer_replacement_delivers_tell_signal_and_both_stream_lanes() {
    payloads_after_replacement(TestRtcRole::Answerer).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn production_finish_drains_already_entered_callback_without_blocking_rpc() {
    let pair = Nodes::new(TestRtcRole::Answerer).await;
    let stream = "held-finish";
    pair.call(
        1,
        "open-stream",
        Bytes::from_static(stream.as_bytes()),
        5_000,
    )
    .await
    .unwrap();
    let dest = actr_framework::Dest::Peer(pair.nodes[0].actor_id());
    pair.nodes[1]
        .app_context()
        .await
        .send_data_chunk_with_type(
            &dest,
            PayloadType::StreamReliable,
            actr_protocol::DataChunk {
                stream_id: stream.into(),
                sequence: 1,
                payload: Bytes::from_static(b"accepted"),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    pair.journals[0].wait("chunk-enter:held-finish:1", 1).await;
    assert_eq!(pair.journals[0].count("chunk:held-finish:1"), 0);
    pair.call(1, "finish", Bytes::from_static(stream.as_bytes()), 3_000)
        .await
        .expect("Finish must not block behind callback");
    pair.echo(1, "unrelated-rpc-during-drain").await;
    pair.journals[0].chunk_release.add_permits(1);
    pair.journals[0].wait("chunk:held-finish:1", 1).await;
    assert_eq!(pair.journals[0].count("chunk:held-finish:1"), 1);
    pair.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn production_timed_out_waiter_never_executes_and_fresh_context_still_works() {
    let pair = Nodes::new(TestRtcRole::Answerer).await;
    pair.background(0).await;
    let result = pair.call(0, "expired-before-send", Bytes::new(), 100).await;
    assert!(
        matches!(result, Err(actr_protocol::ActrError::TimedOut)),
        "{result:?}"
    );
    assert_eq!(pair.journals[1].count("expired-before-send"), 0);
    pair.foreground(0, 5_000).await;
    pair.ready().await;
    pair.echo(0, "fresh-deadline-after-timeout").await;
    assert_eq!(pair.journals[1].count("expired-before-send"), 0);
    pair.shutdown().await;
}

async fn offline_wait(role: TestRtcRole) {
    use actr_hyper::lifecycle::{NetworkAvailability, NetworkSnapshot, NetworkTransportFlags};
    let pair = Nodes::new(role).await;
    pair.events[0]
        .handle_network_path_changed(NetworkSnapshot {
            sequence: 1,
            availability: NetworkAvailability::Unavailable,
            transport: NetworkTransportFlags::default(),
            is_expensive: false,
            is_constrained: false,
        })
        .await
        .unwrap();
    pair.rtc[0]
        .wait_state(&pair.nodes[1].actor_id(), WAIT, |s| !s.sendable)
        .await
        .unwrap();
    let result = pair
        .call(0, "offline-must-not-arrive", Bytes::new(), 600)
        .await;
    assert!(
        matches!(result, Err(actr_protocol::ActrError::TimedOut)),
        "{result:?}"
    );
    assert_eq!(pair.journals[1].count("offline-must-not-arrive"), 0);
    pair.events[0]
        .handle_network_path_changed(NetworkSnapshot {
            sequence: 2,
            availability: NetworkAvailability::Available,
            transport: NetworkTransportFlags {
                wifi: true,
                ..Default::default()
            },
            is_expensive: false,
            is_constrained: false,
        })
        .await
        .unwrap();
    // First ordinary business request after the platform observation; no manual reconnect.
    pair.echo(0, "first-after-offline").await;
    pair.echo(1, "reverse-after-offline").await;
    assert_eq!(pair.journals[1].count("offline-must-not-arrive"), 0);
    pair.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn production_offerer_offline_projection_bounds_send_and_restores() {
    offline_wait(TestRtcRole::Offerer).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn production_answerer_offline_projection_bounds_send_and_restores() {
    offline_wait(TestRtcRole::Answerer).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn production_cancelled_background_call_is_not_replayed_after_foreground() {
    let pair = Nodes::new(TestRtcRole::Offerer).await;
    pair.background(0).await;
    let sender = pair.nodes[0].clone();
    let dest = actr_framework::Dest::Peer(pair.nodes[1].actor_id());
    let call = tokio::spawn(async move {
        sender
            .app_context()
            .await
            .call_raw(
                &dest,
                "cancelled".into(),
                PayloadType::RpcReliable,
                Bytes::new(),
                30_000,
            )
            .await
    });
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(!call.is_finished());
    call.abort();
    assert!(call.await.unwrap_err().is_cancelled());
    pair.foreground(0, 5_000).await;
    pair.ready().await;
    pair.echo(0, "after-cancel").await;
    assert_eq!(pair.journals[1].count("cancelled"), 0);
    pair.shutdown().await;
}

async fn grace_restores_response(role: TestRtcRole, fail_mobile: bool) {
    let pair = Nodes::new(role).await;
    let target = pair.nodes[0].actor_id();
    let server = pair.nodes[1].clone();
    let call = tokio::spawn(async move {
        server
            .app_context()
            .await
            .call_raw(
                &actr_framework::Dest::Peer(target),
                "hold".into(),
                PayloadType::RpcReliable,
                Bytes::from_static(b"grace"),
                20_000,
            )
            .await
    });
    pair.journals[0].wait("hold", 1).await;
    let failed = if fail_mobile { 0 } else { 1 };
    pair.rtc[failed]
        .fail_peer(&pair.nodes[1 - failed].actor_id())
        .await
        .unwrap();
    pair.rtc[failed]
        .wait_state(&pair.nodes[1 - failed].actor_id(), WAIT, |s| !s.sendable)
        .await
        .unwrap();
    // A caller whose own connection is failed has a local grace obligation.
    // A still-healthy caller cannot see the remote-only failure immediately;
    // its business deadline remains the backstop.
    let bound = if fail_mobile { 23 } else { 12 };
    let result = tokio::time::timeout(Duration::from_secs(bound), call).await;
    pair.journals[0].reply_release.add_permits(1);
    let count = pair.journals[0].count("hold");
    pair.shutdown().await;
    let result = result.expect("submitted request grace deadline").unwrap();
    if fail_mobile {
        assert!(
            matches!(
                result,
                Err(actr_protocol::ActrError::DeliveryUncertain { .. }
                    | actr_protocol::ActrError::TimedOut)
            ),
            "{result:?}"
        );
    } else {
        assert!(
            matches!(
                result,
                Err(actr_protocol::ActrError::DeliveryUncertain { .. })
            ),
            "{result:?}"
        );
    }
    assert_eq!(count, 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn production_failed_mobile_answerer_bounds_remote_call_at_business_deadline() {
    grace_restores_response(TestRtcRole::Answerer, true).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn production_failed_server_answerer_releases_its_pending_before_rpc_deadline() {
    grace_restores_response(TestRtcRole::Offerer, false).await;
}

async fn repeated_cycles(role: TestRtcRole) {
    let pair = Nodes::new(role).await;
    let mut previous = pair.ready().await;
    for cycle in 0..6 {
        pair.background(0).await;
        if cycle % 2 == 0 {
            pair.foreground(0, 5_000).await;
            let current = pair.ready().await;
            assert_eq!(current[0].pc_generation, previous[0].pc_generation);
            assert_eq!(current[1].pc_generation, previous[1].pc_generation);
            previous = current;
        } else {
            pair.foreground(0, 5_000).await;
            pair.ready().await;
            pair.rtc[0]
                .close_lane(&pair.nodes[1].actor_id(), PayloadType::RpcReliable)
                .await
                .unwrap();
            for (i, prior) in previous.iter().enumerate() {
                pair.rtc[i]
                    .wait_state(&pair.nodes[1 - i].actor_id(), WAIT, |s| {
                        s.sendable && s.pc_generation > prior.pc_generation
                    })
                    .await
                    .unwrap();
            }
            previous = pair.ready().await;
        }
        for from in 0..2 {
            pair.echo(from, &format!("cycle-{cycle}")).await;
            let stream = format!("cycle-{cycle}-{from}");
            pair.call(from, "open-stream", Bytes::from(stream.clone()), 5_000)
                .await
                .unwrap();
            pair.stream(from, &stream, 1, PayloadType::StreamReliable)
                .await;
            pair.call(from, "finish", Bytes::from(stream), 5_000)
                .await
                .unwrap();
            let resources = pair.rtc[from].observation().await;
            assert_eq!(resources.peer_count, 1);
            assert!(
                resources.peers.iter().all(|p| p.live_lanes <= 2),
                "{resources:?}"
            );
            assert_eq!(resources.inbound_dropped_messages, 0);
        }
    }
    pair.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn production_offerer_repeated_soft_hard_cycles_do_not_accumulate_lanes() {
    repeated_cycles(TestRtcRole::Offerer).await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn production_answerer_repeated_soft_hard_cycles_do_not_accumulate_lanes() {
    repeated_cycles(TestRtcRole::Answerer).await;
}

async fn short_background(role: TestRtcRole) {
    let pair = Nodes::new(role).await;
    let before = pair.ready().await;
    let probe = pair.rtc[0]
        .lane_probe(&pair.nodes[1].actor_id(), PayloadType::RpcReliable)
        .await;
    assert!(probe.is_open().await);
    pair.background(0).await;
    pair.foreground(0, 5_000).await;
    let after = pair.ready().await;
    for i in 0..2 {
        assert_eq!(after[i].pc_generation, before[i].pc_generation);
    }
    let current = pair.rtc[0]
        .lane_probe(&pair.nodes[1].actor_id(), PayloadType::RpcReliable)
        .await;
    assert!(
        probe.same_channel(&current),
        "same physical anchor must survive soft recovery"
    );
    assert!(current.is_open().await);
    for from in 0..2 {
        pair.echo(from, "first-production-response-after-foreground")
            .await;
        let stream = format!("after-bg-{from}");
        pair.call(from, "open-stream", Bytes::from(stream.clone()), 10_000)
            .await
            .unwrap();
        pair.stream(from, &stream, 1, PayloadType::StreamReliable)
            .await;
        pair.call(from, "finish", Bytes::from(stream), 10_000)
            .await
            .unwrap();
    }
    pair.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn production_offerer_short_background_reuses_anchor_and_replies_bidirectionally() {
    short_background(TestRtcRole::Offerer).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn production_answerer_short_background_reuses_anchor_and_replies_bidirectionally() {
    short_background(TestRtcRole::Answerer).await;
}

async fn submitted_reply(role: TestRtcRole) {
    let pair = Nodes::new(role).await;
    // Server -> mobile request enters a real Handler, which retains the reply.
    let sender = pair.nodes[1].clone();
    let target = pair.nodes[0].actor_id();
    let request = tokio::spawn(async move {
        sender
            .app_context()
            .await
            .call_raw(
                &actr_framework::Dest::Peer(target),
                "hold".into(),
                PayloadType::RpcReliable,
                Bytes::from_static(b"held-response"),
                15_000,
            )
            .await
    });
    pair.journals[0].wait("hold", 1).await;
    pair.background(0).await;
    pair.journals[0].reply_release.add_permits(1);
    pair.foreground(0, 5_000).await;
    let response = tokio::time::timeout(WAIT, request)
        .await
        .unwrap()
        .unwrap()
        .expect("real held reply across soft recovery");
    assert_eq!(response, Bytes::from_static(b"held-response"));
    assert_eq!(pair.journals[0].count("hold"), 1);
    pair.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn production_mobile_offerer_replies_to_submitted_server_call_during_recovery() {
    submitted_reply(TestRtcRole::Offerer).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn production_mobile_answerer_replies_to_submitted_server_call_during_recovery() {
    submitted_reply(TestRtcRole::Answerer).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn production_offerer_long_background_first_business_dials_after_cleanup() {
    let pair = Nodes::new(TestRtcRole::Offerer).await;
    let original = pair.ready().await;
    pair.background(0).await;
    pair.foreground(0, 65_000).await;
    assert!(
        pair.rtc[0].wait_peer_count(0, WAIT).await,
        "observe actual cleanup before business dial"
    );
    pair.echo(0, "first-call-after-global-cleanup").await;
    pair.echo(1, "reverse-after-global-cleanup").await;
    let current = pair.ready().await;
    assert!(current[0].pc_generation > original[0].pc_generation);
    assert!(current[1].pc_generation > original[1].pc_generation);
    pair.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn production_reliable_call_waits_in_background_then_sends_once() {
    let pair = Nodes::new(TestRtcRole::Answerer).await;
    pair.background(0).await;
    let sender = pair.nodes[0].clone();
    let target = pair.nodes[1].actor_id();
    let (entered, at_send) = tokio::sync::oneshot::channel();
    let call = tokio::spawn(async move {
        let ctx = sender.app_context().await;
        entered.send(()).unwrap();
        ctx.call_raw(
            &actr_framework::Dest::Peer(target),
            "waited".into(),
            PayloadType::RpcReliable,
            Bytes::from_static(b"new-during-background"),
            5_000,
        )
        .await
    });
    at_send.await.unwrap();
    // Give the blocked send an observation window, rather than assuming that
    // accepting a platform callback means the business route is ready.
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(
        !call.is_finished(),
        "reliable send must wait for the recovering route"
    );
    assert_eq!(pair.journals[1].count("waited"), 0);
    pair.foreground(0, 5_000).await;
    assert_eq!(
        tokio::time::timeout(WAIT, call)
            .await
            .unwrap()
            .unwrap()
            .unwrap(),
        Bytes::from_static(b"new-during-background")
    );
    assert_eq!(pair.journals[1].count("waited"), 1);
    pair.shutdown().await;
}

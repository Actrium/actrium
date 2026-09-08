//! Fault at a verified first-fragment boundary through real production Contexts.
#[path = "support/governance.rs"]
mod support;
use actr_framework::Bytes;
use actr_hyper::test_support::{
    TestRtcRole, WebRtcFragmentSendHook, install_webrtc_fragment_send_hook_for_test,
};
use actr_protocol::{ActrError, DataChunk, PayloadType};
use std::sync::{Arc, Mutex, OnceLock};
use support::{Nodes, WAIT};

fn hook_lock() -> &'static tokio::sync::Mutex<()> {
    static LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
}

async fn partial_submit(role: TestRtcRole, from: usize) {
    let _exclusive = hook_lock().lock().await;
    for kind in ["call", "tell", "chunk"] {
        let pair = Nodes::new(role).await;
        let old = pair.ready().await;
        let stream = format!("fragment-{from}");
        if kind == "chunk" {
            pair.call(from, "open-stream", Bytes::from(stream.clone()), 5_000)
                .await
                .unwrap();
        }
        let release = Arc::new(tokio::sync::Semaphore::new(0));
        let (tx, entered) = tokio::sync::oneshot::channel();
        let tx = Arc::new(Mutex::new(Some(tx)));
        let hook: WebRtcFragmentSendHook = {
            let release = release.clone();
            Arc::new(move |event| {
                let tx = tx.clone();
                let release = release.clone();
                Box::pin(async move {
                    if event.frag_index != 0 || event.total_frags < 2 {
                        return;
                    }
                    let first = tx.lock().unwrap().take();
                    if let Some(tx) = first {
                        let _ = tx.send(event);
                        release.acquire().await.unwrap().forget();
                    }
                })
            })
        };
        let guard = install_webrtc_fragment_send_hook_for_test(hook);
        let sender = pair.nodes[from].clone();
        let dest = actr_framework::Dest::Peer(pair.nodes[1 - from].actor_id());
        let operation_stream = stream.clone();
        let operation = tokio::spawn(async move {
            let ctx = sender.app_context().await;
            let bytes = Bytes::from(vec![0x61; 200 * 1024]);
            match kind {
                "call" => ctx
                    .call_raw(
                        &dest,
                        "partial-call".into(),
                        PayloadType::RpcReliable,
                        bytes,
                        15_000,
                    )
                    .await
                    .map(|_| ()),
                "tell" => {
                    ctx.tell_raw(
                        &dest,
                        "partial-tell".into(),
                        PayloadType::RpcReliable,
                        bytes,
                    )
                    .await
                }
                _ => {
                    ctx.send_data_chunk_with_type(
                        &dest,
                        PayloadType::StreamReliable,
                        DataChunk {
                            stream_id: operation_stream,
                            sequence: 7,
                            payload: bytes,
                            ..Default::default()
                        },
                    )
                    .await
                }
            }
        });
        let event = tokio::time::timeout(WAIT, entered).await.unwrap().unwrap();
        assert!(event.total_frags > 1);
        // Real physical anchor close: both endpoints may observe it and the
        // Offerer autonomously creates the replacement. No manual reconnect.
        pair.rtc[0]
            .close_lane(&pair.nodes[1].actor_id(), PayloadType::RpcReliable)
            .await
            .unwrap();
        let replacement = pair.rtc[from]
            .wait_state(&pair.nodes[1 - from].actor_id(), WAIT, |s| {
                s.sendable && s.pc_generation > old[from].pc_generation
            })
            .await;
        release.add_permits(1);
        drop(guard);
        let outcome = tokio::time::timeout(WAIT, operation)
            .await
            .unwrap()
            .unwrap();
        let executed = pair.journals[1 - from].count(match kind {
            "call" => "partial-call",
            "tell" => "partial-tell",
            _ => "unused",
        });
        let delivered = pair.journals[1 - from].count(&format!("chunk:{stream}:7"));
        pair.shutdown().await;
        assert_eq!(
            executed, 0,
            "incomplete old-generation RPC/Tell must not execute"
        );
        assert_eq!(
            delivered, 0,
            "incomplete old-generation chunk must not reach the callback"
        );
        assert!(
            replacement.is_some(),
            "autonomous replacement while {kind} is paused"
        );
        match outcome {
            Err(ActrError::DeliveryUncertain {
                operation_id,
                cause,
            }) => {
                assert!(!operation_id.is_empty() && !cause.is_empty());
                if kind == "chunk" {
                    assert_eq!(operation_id, format!("{stream}#7"));
                }
            }
            other => panic!(
                "{role:?} direction={from} {kind}: partial submission must be uncertain, got {other:?}"
            ),
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mobile_offerer_outbound_partial_call_tell_chunk_are_uncertain() {
    partial_submit(TestRtcRole::Offerer, 0).await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mobile_offerer_inbound_partial_call_tell_chunk_are_uncertain() {
    partial_submit(TestRtcRole::Offerer, 1).await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mobile_answerer_outbound_partial_call_tell_chunk_are_uncertain() {
    partial_submit(TestRtcRole::Answerer, 0).await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mobile_answerer_inbound_partial_call_tell_chunk_are_uncertain() {
    partial_submit(TestRtcRole::Answerer, 1).await;
}

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::time::Duration;

use actr_framework::{Bytes, Context, Dest};
use actr_hyper::actrium::{ActriumPeerSession, FrontSessionMetadata};
use actr_hyper::test_support::TestSignalingServer;
use actr_hyper::{ActrRef, FrontEventListener, FrontEventRouter, FrontServiceEvent};
use actr_protocol::{ActorResult, ActrId, DataChunk, PayloadType};
use actrium_runtime::{InMemorySessionService, ServicePolicies};
use tempfile::TempDir;
use tokio::sync::Notify;

use super::{
    AllowPolicies, EchoWorkload, READ, attach_params, front_type, make_service_config,
    register_node, service_type,
};

pub(super) const ROUTE: &str = "echo.stream";
pub(super) const UPLOAD_ROUTE: &str = "echo.upload";
const STREAM_ID: &str = "echo-reply";
const UPLOAD_ID: &str = "echo-input";
const ACK: &[u8] = b"accepted";
const TEXT: &[u8] = b"background reply";
const TERMINAL: &[u8] = b"completed";

struct IgnoreEvents;

impl FrontEventListener for IgnoreEvents {
    fn on_event(&self, _event: FrontServiceEvent) {}
}

pub(super) async fn start_front_node(
    server: &TestSignalingServer,
    dir: &TempDir,
) -> (ActrRef, FrontEventRouter) {
    let registered = register_node(server, dir, front_type(), EchoWorkload::default()).await;
    let policies = ServicePolicies {
        membership: Arc::new(AllowPolicies),
        authorize_attach: Arc::new(AllowPolicies),
        authorize_takeover: Arc::new(AllowPolicies),
        authorize_request: Arc::new(AllowPolicies),
        privacy: Arc::new(AllowPolicies),
        quota: Arc::new(AllowPolicies),
    };
    let service = InMemorySessionService::new(
        make_service_config(
            &registered.actor_id().to_string_repr(),
            registered.actrium_actor_signing_key().unwrap(),
        ),
        policies,
    )
    .unwrap();
    let router = FrontEventRouter::new([], Arc::new(IgnoreEvents));
    let front = registered
        .with_actrium_front(service, router.clone())
        .start()
        .await
        .unwrap();
    (front, router)
}

pub(super) fn dispatch<C: Context>(ctx: &C, reply_ready: Arc<Notify>) -> ActorResult<Bytes> {
    let caller = ctx.caller_id().expect("the reply RPC has a caller").clone();
    let ctx = ctx.clone();
    tokio::spawn(async move {
        // The caller releases this only after receiving the RPC acknowledgement.
        reply_ready.notified().await;
        for (sequence, payload) in [(1, TEXT), (2, TERMINAL)] {
            ctx.send_data_chunk(
                &Dest::Peer(caller.clone()),
                DataChunk {
                    stream_id: STREAM_ID.into(),
                    sequence,
                    payload: Bytes::from_static(payload),
                    ..Default::default()
                },
                PayloadType::StreamReliable,
            )
            .await
            .expect("send the background reply frame");
        }
    });
    Ok(Bytes::from_static(ACK))
}

pub(super) async fn prepare_upload<C: Context>(ctx: &C, ready: Arc<Notify>) -> ActorResult<Bytes> {
    ctx.register_stream(UPLOAD_ID.into(), move |chunk, _sender| {
        let ready = ready.clone();
        Box::pin(async move {
            assert_eq!(chunk.payload, Bytes::from_static(b"start reply"));
            ready.notify_one();
            Ok(())
        })
    })
    .await?;
    Ok(Bytes::new())
}

pub(super) async fn verify(
    front: &ActrRef,
    router: &FrontEventRouter,
    service: &ActrId,
    session: &Arc<ActriumPeerSession>,
    reply_ready: &Notify,
) {
    router
        .register_stream(STREAM_ID.into(), service.clone())
        .unwrap();
    let mut receive = register_reply(front, service).await;
    assert_eq!(
        session
            .call(ROUTE, READ, Vec::new(), Duration::from_secs(10))
            .await
            .unwrap(),
        ACK
    );
    reply_ready.notify_one();
    expect_reply(&mut receive, service).await;
    front
        .app_context()
        .await
        .unregister_stream(STREAM_ID)
        .await
        .unwrap();
    router.unregister_stream(STREAM_ID);
}

async fn register_reply(
    front: &ActrRef,
    service: &ActrId,
) -> tokio::sync::mpsc::Receiver<(DataChunk, ActrId)> {
    let (send, receive) = tokio::sync::mpsc::channel(2);
    let ctx = front.app_context().await;
    ctx.register_peer_stream(STREAM_ID.into(), service.clone(), move |chunk, sender| {
        let send = send.clone();
        Box::pin(async move {
            send.send((chunk, sender))
                .await
                .expect("send to the test receiver");
            Ok(())
        })
    })
    .unwrap();
    receive
}

async fn expect_reply(
    receive: &mut tokio::sync::mpsc::Receiver<(DataChunk, ActrId)>,
    service: &ActrId,
) {
    for (sequence, payload) in [(1, TEXT), (2, TERMINAL)] {
        let (chunk, sender) = tokio::time::timeout(Duration::from_secs(5), receive.recv())
            .await
            .expect("the Front receives the deferred reply frame")
            .unwrap();
        assert_eq!(sender, *service);
        assert_eq!(chunk.payload, Bytes::from_static(payload));
        assert_eq!(chunk.sequence, sequence);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn ordinary_relay_replies_after_ack() {
    let mut server = TestSignalingServer::start_with_relay().await.unwrap();
    server.pause_forwarding();
    let service_dir = TempDir::new().unwrap();
    let front_dir = TempDir::new().unwrap();
    let service = start_ordinary_service(&server, &service_dir).await;
    let front = register_node(&server, &front_dir, front_type(), EchoWorkload::default())
        .await
        .start()
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(30), async {
        while server.relay_connection_count() != 2 {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("both nodes connect to Relay");

    let target = service.actor_id();
    front.prepare_peer_rpc(&target, 15_000).await.unwrap();
    let mut receive = register_reply(&front, &target).await;
    call_and_send_upload(&front, &target).await;
    expect_reply(&mut receive, &target).await;
    for node in [front, service] {
        node.shutdown();
        node.wait_for_shutdown().await;
    }
    server.shutdown().await;
}

async fn start_ordinary_service(server: &TestSignalingServer, dir: &TempDir) -> ActrRef {
    let params = attach_params();
    register_node(server, dir, service_type(), EchoWorkload::default())
        .await
        .with_ordinary_service_authorization(FrontSessionMetadata {
            service_type: service_type(),
            front_actor_type: front_type(),
            package_digest: params.package_digest,
            guest_digest: params.guest_digest,
            protocol_fingerprint: params.protocol_fingerprint,
            runtime: params.runtime,
            program_capabilities: BTreeSet::from([READ.to_owned()]),
            routes: BTreeMap::from([
                (ROUTE.to_owned(), BTreeSet::from([READ.to_owned()])),
                (UPLOAD_ROUTE.to_owned(), BTreeSet::from([READ.to_owned()])),
            ]),
            stream_send_allowed: true,
        })
        .start()
        .await
        .unwrap()
}

async fn call_and_send_upload(front: &ActrRef, service: &ActrId) {
    let ctx = front.app_context().await;
    let target = Dest::Peer(service.clone());
    ctx.call_raw(
        &target,
        UPLOAD_ROUTE.into(),
        PayloadType::RpcReliable,
        Bytes::new(),
        10_000,
    )
    .await
    .unwrap();
    assert_eq!(
        ctx.call_raw(
            &target,
            ROUTE.into(),
            PayloadType::RpcReliable,
            Bytes::new(),
            10_000
        )
        .await
        .unwrap(),
        ACK
    );
    ctx.send_data_chunk(
        &target,
        DataChunk {
            stream_id: UPLOAD_ID.into(),
            sequence: 1,
            payload: Bytes::from_static(b"start reply"),
            ..Default::default()
        },
        PayloadType::StreamReliable,
    )
    .await
    .unwrap();
}

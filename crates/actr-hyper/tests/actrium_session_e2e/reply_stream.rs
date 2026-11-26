use std::sync::Arc;
use std::time::Duration;

use actr_framework::{Bytes, Context, Dest};
use actr_hyper::actrium::ActriumPeerSession;
use actr_hyper::test_support::TestSignalingServer;
use actr_hyper::{
    ActrRef, FrontEventListener, FrontEventRouter, FrontServiceEvent, Hyper, HyperConfig, Node,
    StaticTrust,
};
use actr_protocol::{ActorResult, ActrId, DataChunk, PayloadType};
use actrium_runtime::{InMemorySessionService, ServicePolicies};
use tempfile::TempDir;
use tokio::sync::Notify;

use super::{AllowPolicies, EchoWorkload, READ, front_type, make_service_config, runtime_config};

pub(super) const ROUTE: &str = "echo.stream";
const STREAM_ID: &str = "echo-reply";
const REPLY: &[u8] = b"background reply over the verified channel";

struct IgnoreEvents;

impl FrontEventListener for IgnoreEvents {
    fn on_event(&self, _event: FrontServiceEvent) {}
}

pub(super) async fn start_front_node(
    server: &TestSignalingServer,
    dir: &TempDir,
) -> (ActrRef, FrontEventRouter) {
    let hyper = Hyper::new(HyperConfig::new(
        dir.path(),
        Arc::new(StaticTrust::dev_only()),
    ))
    .await
    .unwrap();
    let config = runtime_config(dir, server, front_type());
    let registered = Node::from_hyper(hyper, config)
        .link(EchoWorkload {
            reply_ready: Arc::new(Notify::new()),
        })
        .await
        .unwrap()
        .register(&format!("http://127.0.0.1:{}/issuer", server.port()))
        .await
        .unwrap();
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
        ctx.send_data_chunk(
            &Dest::Peer(caller),
            DataChunk {
                stream_id: STREAM_ID.into(),
                sequence: 1,
                payload: Bytes::from_static(REPLY),
                ..Default::default()
            },
            PayloadType::StreamReliable,
        )
        .await
        .expect("send the background reply");
    });
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
    let (send, mut receive) = tokio::sync::mpsc::channel(1);
    let ctx = front.app_context().await;
    ctx.register_stream(STREAM_ID.into(), move |chunk, sender| {
        let send = send.clone();
        Box::pin(async move {
            send.send((chunk, sender))
                .await
                .expect("send to the test receiver");
            Ok(())
        })
    })
    .await
    .unwrap();
    session
        .call(ROUTE, READ, Vec::new(), Duration::from_secs(10))
        .await
        .unwrap();
    reply_ready.notify_one();
    let (chunk, sender) = tokio::time::timeout(Duration::from_secs(5), receive.recv())
        .await
        .expect("the verified Front receives the deferred reply stream")
        .unwrap();
    assert_eq!(sender, *service);
    assert_eq!(chunk.payload, Bytes::from_static(REPLY));
    assert_eq!(chunk.sequence, 1);
    ctx.unregister_stream(STREAM_ID).await.unwrap();
    router.unregister_stream(STREAM_ID);
}

use super::*;
use actr_protocol::{ActrId, DataChunk, PayloadType};
use std::sync::Arc;

#[tokio::test]
async fn ordinary_policy_checks_tell_routes_and_upload_grants() {
    let front_type = ActrType {
        publisher: "test".into(),
        name: "front".into(),
        version: "1.0.0".into(),
    };
    let front = ActrId {
        realm: "test@example.com".into(),
        serial_number: 1,
    };
    let unknown = ActrId {
        serial_number: 2,
        ..front.clone()
    };
    let peer_types = PeerTypes::default();
    peer_types.record(front.clone(), front_type.clone());
    let mut guard = OrdinaryServiceGuard {
        peer_types,
        front_type: front_type.clone(),
        routes: BTreeSet::from(["echo".into()]),
        streams_allowed: false,
    };
    let mut tell = RpcEnvelope {
        direction: Some(Direction::Tell as i32),
        route_key: "echo".into(),
        ..Default::default()
    };
    let caller = Some((&front, Some(&front_type)));
    assert!(
        guard
            .check(IncomingDispatchRequest::new(&tell, caller))
            .await
            .is_ok()
    );
    tell.route_key = "undeclared".into();
    assert!(
        guard
            .check(IncomingDispatchRequest::new(&tell, caller))
            .await
            .is_err()
    );

    let mut chunk = DataChunk::default();
    assert!(
        guard
            .check_stream(IncomingStreamChunk::new(&mut chunk, &front))
            .await
            .is_err()
    );
    guard.streams_allowed = true;
    assert!(
        guard
            .check_stream(IncomingStreamChunk::new(&mut chunk, &front))
            .await
            .is_ok()
    );
    assert!(
        guard
            .check_stream(IncomingStreamChunk::new(&mut chunk, &unknown))
            .await
            .is_err()
    );
}

#[tokio::test]
async fn ordinary_uploads_are_bound_to_the_registering_front() {
    let front_type = ActrType {
        publisher: "test".into(),
        name: "front".into(),
        version: "1.0.0".into(),
    };
    let first = ActrId {
        realm: "test@example.com".into(),
        serial_number: 1,
    };
    let second = ActrId {
        serial_number: 2,
        ..first.clone()
    };
    let peer_types = PeerTypes::default();
    peer_types.record(first.clone(), front_type.clone());
    peer_types.record(second.clone(), front_type.clone());
    let registry = crate::inbound::DataChunkRegistry::new();
    registry.set_incoming_guard(Some(Arc::new(OrdinaryServiceGuard {
        peer_types,
        front_type,
        routes: BTreeSet::new(),
        streams_allowed: true,
    })));
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let callback: crate::inbound::DataChunkCallback = Arc::new(move |chunk, sender| {
        tx.send((chunk.stream_id, sender)).unwrap();
        Box::pin(async { Ok(()) })
    });
    for (stream, owner) in [("one", &first), ("two", &second)] {
        registry
            .register_for_caller(stream.into(), callback.clone(), Some(owner))
            .unwrap();
    }
    assert!(
        registry
            .register_for_caller("one".into(), callback, Some(&second))
            .is_err()
    );
    assert!(
        registry
            .unregister_for_caller("one", Some(&second))
            .is_err()
    );

    for (stream, sender) in [("one", &second), ("one", &first), ("two", &second)] {
        registry
            .dispatch_with_context(
                DataChunk {
                    stream_id: stream.into(),
                    ..DataChunk::default()
                },
                sender.clone(),
                None,
                PayloadType::StreamReliable,
            )
            .await;
    }
    let mut delivered = tokio::time::timeout(std::time::Duration::from_secs(2), async {
        vec![rx.recv().await.unwrap(), rx.recv().await.unwrap()]
    })
    .await
    .unwrap();
    delivered.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(
        delivered,
        vec![("one".into(), first), ("two".into(), second)]
    );
    registry.shutdown().await;
}

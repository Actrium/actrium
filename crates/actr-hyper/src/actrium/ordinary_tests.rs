use super::*;
use actr_protocol::{ActrId, DataChunk};

#[tokio::test]
async fn ordinary_policy_checks_routes_and_denies_streams() {
    let front_type = ActrType {
        publisher: "test".into(),
        name: "front".into(),
        version: "1.0.0".into(),
    };
    let front = ActrId {
        realm: "test@example.com".into(),
        serial_number: 1,
    };
    let guard = OrdinaryServiceGuard {
        front_type: front_type.clone(),
        routes: BTreeSet::from(["echo".into()]),
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
    // Ordinary Services have no stream route; the guard must never advertise
    // caller-bound streams.
    assert!(!guard.binds_streams_to_caller());
}

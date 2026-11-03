use super::*;
use actr_protocol::ActrId;

#[tokio::test]
async fn cancelled_creation_does_not_materialize_candidates() {
    let factory = DefaultWireBuilder::new(DefaultWireBuilderConfig::default());
    let token = CancellationToken::new();
    token.cancel();
    let result = factory
        .create_connections_with_cancel(&Dest::peer(ActrId::default()), Some(token))
        .await;
    assert!(matches!(result, Err(NetworkError::ConnectionClosed(_))));
}

#[tokio::test]
async fn missing_relay_does_not_fall_back_to_an_application_endpoint() {
    let factory = DefaultWireBuilder::new(DefaultWireBuilderConfig::default());
    assert!(
        factory
            .create_connections(&Dest::peer(ActrId::default()))
            .await
            .unwrap()
            .is_empty()
    );
}

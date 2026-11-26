use std::collections::BTreeSet;

use actr_protocol::{ActrType, Direction, RpcEnvelope};

use crate::actrium::{ATTACHMENT_PREPARE_ROUTE, ATTACHMENT_UPLOAD_TRANSPORT, FrontSessionMetadata};
use crate::lifecycle::node::PeerTypes;
use crate::lifecycle::{
    IncomingDispatchRequest, IncomingGuard, IncomingGuardError, IncomingGuardErrorCode,
    IncomingStreamChunk,
};

/// Service policy for ordinary traffic from the issuer-authenticated Front type.
pub(crate) struct OrdinaryServiceGuard {
    peer_types: PeerTypes,
    front_type: ActrType,
    routes: BTreeSet<String>,
    uploads_allowed: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use actr_protocol::{ActrId, DataChunk};

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
            uploads_allowed: false,
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
        guard.uploads_allowed = true;
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
}

impl OrdinaryServiceGuard {
    pub(crate) fn new(metadata: FrontSessionMetadata, peer_types: PeerTypes) -> Self {
        let uploads_allowed = metadata.routes.contains_key(ATTACHMENT_UPLOAD_TRANSPORT);
        let mut routes: BTreeSet<_> = metadata.routes.into_keys().collect();
        routes.remove(ATTACHMENT_UPLOAD_TRANSPORT);
        routes.remove(ATTACHMENT_PREPARE_ROUTE);
        if uploads_allowed {
            routes.insert(ATTACHMENT_PREPARE_ROUTE.to_owned());
        }
        Self {
            peer_types,
            front_type: metadata.front_actor_type,
            routes,
            uploads_allowed,
        }
    }
}

#[async_trait::async_trait]
impl IncomingGuard for OrdinaryServiceGuard {
    async fn check(&self, request: IncomingDispatchRequest<'_>) -> Result<(), IncomingGuardError> {
        if request.envelope.direction == Some(Direction::Response as i32) {
            return Ok(());
        }
        let caller_type = request.caller.and_then(|(_, actor_type)| actor_type);
        if caller_type != Some(&self.front_type)
            || !self.routes.contains(&request.envelope.route_key)
        {
            return Err(IncomingGuardError::new(
                IncomingGuardErrorCode::PermissionDenied,
                "Service policy does not grant this caller the requested route",
            ));
        }
        Ok(())
    }

    async fn authenticate_response(
        &self,
        _response: &mut RpcEnvelope,
    ) -> Result<(), IncomingGuardError> {
        Ok(())
    }

    async fn check_stream(
        &self,
        request: IncomingStreamChunk<'_>,
    ) -> Result<(), IncomingGuardError> {
        if !self.uploads_allowed
            || self.peer_types.get(request.sender).as_ref() != Some(&self.front_type)
        {
            return Err(IncomingGuardError::new(
                IncomingGuardErrorCode::PermissionDenied,
                "Service policy does not grant this caller attachment uploads",
            ));
        }
        Ok(())
    }
}

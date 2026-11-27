use std::collections::BTreeSet;

use actr_protocol::{ActrType, Direction, RpcEnvelope};

use super::manifest::SERVICE_STREAM_SEND_CAPABILITY;
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
    streams_allowed: bool,
}

impl OrdinaryServiceGuard {
    pub(crate) fn new(metadata: FrontSessionMetadata, peer_types: PeerTypes) -> Self {
        let uploads_allowed = metadata.routes.contains_key(ATTACHMENT_UPLOAD_TRANSPORT);
        let streams_allowed = uploads_allowed
            || metadata
                .program_capabilities
                .contains(SERVICE_STREAM_SEND_CAPABILITY);
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
            streams_allowed,
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
        if !self.streams_allowed
            || self.peer_types.get(request.sender).as_ref() != Some(&self.front_type)
        {
            return Err(IncomingGuardError::new(
                IncomingGuardErrorCode::PermissionDenied,
                "Service policy does not grant this caller data streams",
            ));
        }
        Ok(())
    }

    fn binds_streams_to_caller(&self) -> bool {
        true
    }
}

#[cfg(test)]
#[path = "ordinary_tests.rs"]
mod tests;

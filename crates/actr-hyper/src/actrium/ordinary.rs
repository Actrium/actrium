use std::collections::BTreeSet;

use actr_protocol::{ActrType, Direction, RpcEnvelope};

use crate::actrium::FrontSessionMetadata;
use crate::lifecycle::{
    IncomingDispatchRequest, IncomingGuard, IncomingGuardError, IncomingGuardErrorCode,
    IncomingStreamChunk,
};

/// Service policy for ordinary traffic from the issuer-authenticated Front type.
pub(crate) struct OrdinaryServiceGuard {
    front_type: ActrType,
    routes: BTreeSet<String>,
}

impl OrdinaryServiceGuard {
    pub(crate) fn new(metadata: FrontSessionMetadata) -> Self {
        let routes: BTreeSet<_> = metadata.routes.into_keys().collect();
        Self {
            front_type: metadata.front_actor_type,
            routes,
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
        _request: IncomingStreamChunk<'_>,
    ) -> Result<(), IncomingGuardError> {
        Err(IncomingGuardError::new(
            IncomingGuardErrorCode::PermissionDenied,
            "Service policy does not grant this caller streams",
        ))
    }
}

#[cfg(test)]
#[path = "ordinary_tests.rs"]
mod tests;

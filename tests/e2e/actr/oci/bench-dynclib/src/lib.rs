//! DynClib echo used for runtime comparisons with the OCI Guest.

use actr_framework::{Context, MessageDispatcher, Workload, entry};
use actr_protocol::{ActorResult, RpcEnvelope};
use async_trait::async_trait;
use bytes::Bytes;

#[derive(Default)]
pub struct Echo;

#[async_trait]
impl Workload for Echo {
    type Dispatcher = Self;
}

#[async_trait]
impl MessageDispatcher for Echo {
    type Workload = Self;

    async fn dispatch<C: Context>(_: &Self, request: RpcEnvelope, _: &C) -> ActorResult<Bytes> {
        Ok(request.payload.unwrap_or_default())
    }
}

entry!(Echo);

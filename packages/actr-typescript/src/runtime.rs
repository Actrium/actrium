use napi::bindgen_prelude::*;
use napi_derive::napi;
use std::sync::Mutex;

use crate::types::{ActrId, ActrType, PayloadType};
use actr_config::ConfigParser;
use actr_framework::{Context as RtContext, Dest, MessageDispatcher, Workload as RtWorkload};
use actr_hyper::{ActrRef as RuntimeActrRef, Node, Registered};
use actr_protocol::{ActorResult, ActrError, RpcEnvelope};
use async_trait::async_trait;

struct TypeScriptBindingWorkload;

#[async_trait]
impl RtWorkload for TypeScriptBindingWorkload {
    type Dispatcher = TypeScriptBindingDispatcher;
}

struct TypeScriptBindingDispatcher;

#[async_trait]
impl MessageDispatcher for TypeScriptBindingDispatcher {
    type Workload = TypeScriptBindingWorkload;

    async fn dispatch<C: RtContext>(
        _workload: &Self::Workload,
        _envelope: RpcEnvelope,
        _ctx: &C,
    ) -> ActorResult<bytes::Bytes> {
        Err(ActrError::NotImplemented(
            "typescript bindings do not expose inbound local RPC dispatch".to_string(),
        ))
    }
}

#[napi]
pub struct ActrNode {
    inner: Mutex<Option<Node<Registered>>>,
}

#[napi]
impl ActrNode {
    /// Create an ActrNode wrapper from manifest.toml and the sibling actr.toml.
    #[napi(factory)]
    pub async fn from_file(config_path: String) -> Result<ActrNode> {
        // Accept the manifest.toml path, resolve its sibling actr.toml,
        // and feed the manifest's [package] into Hyper so the node
        // registers under the real actr_type instead of the
        // `local:Client:0.0.0` placeholder that `from_config_file`
        // would otherwise synthesise. TypeScript bindings link a
        // minimal static-lib workload here; this surface exposes
        // discovery and outbound calls, not TS-defined service hosting.
        let manifest = ConfigParser::from_manifest_file(&config_path)
            .map_err(crate::error::config_error_to_napi)?;
        let runtime_path = manifest.config_dir.join("actr.toml");

        let init = Node::from_config_with_manifest(&runtime_path, manifest)
            .await
            .map_err(crate::error::hyper_error_to_napi)?;
        crate::logger::init_observability(init.runtime_config().observability.clone());
        let attached = init
            .link(TypeScriptBindingWorkload)
            .await
            .map_err(crate::error::hyper_error_to_napi)?;
        let ais_endpoint = attached.ais_endpoint().to_string();
        let registered = attached
            .register(&ais_endpoint)
            .await
            .map_err(crate::error::hyper_error_to_napi)?;

        Ok(ActrNode {
            inner: Mutex::new(Some(registered)),
        })
    }
    /// Start the node and return ActrRef.
    ///
    /// One-shot: consumes the internal Hyper handle. A second call resolves
    /// with `Node already started`, including concurrent callers.
    #[napi]
    pub async fn start(&self) -> Result<ActrRef> {
        let hyper = self
            .inner
            .lock()
            .map_err(|_| Error::from_reason("Node state lock poisoned"))?
            .take()
            .ok_or_else(|| Error::from_reason("Node already started"))?;

        let actr_ref = hyper
            .start()
            .await
            .map_err(crate::error::protocol_error_to_napi)?;

        Ok(ActrRef { inner: actr_ref })
    }
}

#[napi]
pub struct ActrRef {
    inner: RuntimeActrRef,
}

#[napi]
impl ActrRef {
    /// Get the actor ID.
    #[napi]
    pub fn actor_id(&self) -> ActrId {
        self.inner.actor_id().clone().into()
    }

    /// Discover actors of the given type.
    #[napi]
    pub async fn discover(&self, target_type: ActrType, count: u32) -> Result<Vec<ActrId>> {
        let proto_type: actr_protocol::ActrType = target_type.into();
        let ids = self
            .inner
            .discover_route_candidates(&proto_type, count as usize)
            .await
            .map_err(crate::error::protocol_error_to_napi)?;

        Ok(ids.into_iter().map(|id| id.into()).collect())
    }

    /// Resolve a declared dependency alias to its canonical actor type.
    #[napi]
    pub async fn resolve_dependency(&self, alias: String) -> Result<ActrType> {
        let ctx = self.inner.app_context().await;
        let actr_type = ctx
            .resolve_dependency(&alias)
            .await
            .map_err(crate::error::protocol_error_to_napi)?;
        Ok(actr_type.into())
    }

    /// Call remote actor (RPC).
    #[napi]
    pub async fn call(
        &self,
        target: ActrId,
        route_key: String,
        payload_type: PayloadType,
        request_payload: Buffer,
        timeout_ms: i64,
    ) -> Result<Buffer> {
        let target_id: actr_protocol::ActrId = target.try_into()?;
        let proto_payload_type: actr_protocol::PayloadType = payload_type.into();
        let ctx = self.inner.app_context().await;
        let response = ctx
            .call_raw(
                &Dest::Peer(target_id),
                route_key,
                proto_payload_type,
                bytes::Bytes::from(request_payload.to_vec()),
                timeout_ms,
            )
            .await
            .map_err(crate::error::protocol_error_to_napi)?;

        Ok(response.to_vec().into())
    }

    /// Send one-way message (fire-and-forget).
    #[napi]
    pub async fn tell(
        &self,
        target: ActrId,
        route_key: String,
        payload_type: PayloadType,
        message_payload: Buffer,
    ) -> Result<()> {
        let target_id: actr_protocol::ActrId = target.try_into()?;
        let proto_payload_type: actr_protocol::PayloadType = payload_type.into();
        let ctx = self.inner.app_context().await;
        ctx.tell_raw(
            &Dest::Peer(target_id),
            route_key,
            proto_payload_type,
            bytes::Bytes::from(message_payload.to_vec()),
        )
        .await
        .map_err(crate::error::protocol_error_to_napi)?;

        Ok(())
    }

    /// Trigger shutdown.
    #[napi]
    pub fn shutdown(&self) {
        self.inner.shutdown();
    }

    /// Wait for shutdown to complete.
    #[napi]
    pub async fn wait_for_shutdown(&self) {
        self.inner.wait_for_shutdown().await;
    }

    /// Check if shutdown is in progress.
    #[napi]
    pub fn is_shutting_down(&self) -> bool {
        self.inner.is_shutting_down()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn concurrent_start_on_consumed_node_returns_errors() {
        let node = Arc::new(ActrNode {
            inner: Mutex::new(None),
        });
        let mut tasks = Vec::new();
        for _ in 0..16 {
            let node = Arc::clone(&node);
            tasks.push(tokio::spawn(async move {
                match node.start().await {
                    Err(error) => assert_eq!(error.reason, "Node already started"),
                    Ok(_) => panic!("consumed node must not start again"),
                }
            }));
        }
        for task in tasks {
            task.await.unwrap();
        }
    }
}

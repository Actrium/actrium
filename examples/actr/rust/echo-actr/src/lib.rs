mod generated;

use actr_framework::{Context, entry};
use actr_protocol::ActorResult;
use async_trait::async_trait;

use generated::actr_dispatch::ActrWorkload;
use generated::actr_handler::ActrHandler;
use generated::echo::{EchoRequest, EchoResponse};
use generated::echo_actor::EchoServiceHandler;

// ── Business logic ───────────────────────────────────────────────────────────

/// Concrete implementation of the EchoService.
///
/// This is the only type you need to write. Add your state here and implement
/// [`EchoServiceHandler`] to provide the actual RPC logic.
pub struct EchoService;

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
impl EchoServiceHandler for EchoService {
    async fn echo<C: Context>(&self, req: EchoRequest, _ctx: &C) -> ActorResult<EchoResponse> {
        Ok(EchoResponse {
            reply: format!("Echo: {}", req.message),
            timestamp: current_timestamp(),
        })
    }
}

// ── Entry point ──────────────────────────────────────────────────────────────

entry!(
    ActrWorkload<ActrHandler<EchoService>>,
    ActrWorkload::new(ActrHandler::new(EchoService))
);

// ── Platform helpers ─────────────────────────────────────────────────────────

#[cfg(target_arch = "wasm32")]
#[allow(dead_code)]
fn current_timestamp() -> u64 {
    0
}

#[cfg(not(target_arch = "wasm32"))]
#[allow(dead_code)]
fn current_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

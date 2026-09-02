// ACTR: generated Rust service scaffold
//! Business logic for the EchoService service.
//!
//! Implement each RPC below; the generated `generated::actr_handler::ActrHandler`
//! adapter routes `echo.EchoService.*` envelopes to these methods.

use crate::generated::echo::EchoRequest;
use crate::generated::echo::EchoResponse;
use crate::generated::echo_actor::EchoServiceHandler;
use actr_framework::Context;
use actr_protocol::ActorResult;

pub struct EchoServiceImpl;

impl EchoServiceImpl {
    pub fn new() -> Self {
        Self
    }
}

impl Default for EchoServiceImpl {
    fn default() -> Self {
        Self
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl EchoServiceHandler for EchoServiceImpl {
    async fn echo<C: Context>(&self, req: EchoRequest, _ctx: &C) -> ActorResult<EchoResponse> {
        Ok(EchoResponse {
            reply: format!("Echo: {}", req.message),
            timestamp: current_timestamp(),
        })
    }
}

#[cfg(target_arch = "wasm32")]
fn current_timestamp() -> u64 {
    0
}

#[cfg(not(target_arch = "wasm32"))]
fn current_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

// Guest-side async workload using explicitly async WIT imports and exports.
// wit-bindgen follows the WIT annotations instead of forcing the async ABI
// onto synchronous function types.

wit_bindgen::generate!({
    world: "spike-guest-async",
    path: "../wit",
    generate_all,
});

use actr::spike_async::host::{call_raw, log_message};
use actr::spike_async::types::{ActrId, RpcEnvelope, SpikeError};
use exports::actr::spike_async::workload::Guest;

// Async imports take owned values across suspension points.
async fn log(level: &str, msg: String) {
    log_message(level.to_string(), msg).await;
}

struct EchoAsyncWorkload;

impl Guest for EchoAsyncWorkload {
    async fn on_start() -> Result<(), SpikeError> {
        log("info", "async workload starting".to_string()).await;

        // Exercise an async host call at startup: await + log the reply.
        let target = ActrId { serial_number: 1 };
        match call_raw(
            target,
            "probe.Ping".to_string(),
            b"hello-from-guest-start".to_vec(),
        )
        .await
        {
            Ok(reply) => {
                log(
                    "debug",
                    format!("on_start call-raw ok: {}", String::from_utf8_lossy(&reply)),
                )
                .await;
            }
            Err(e) => {
                log("warn", format!("on_start call-raw err: {:?}", e)).await;
            }
        }

        Ok(())
    }

    async fn dispatch(env: RpcEnvelope) -> Result<Vec<u8>, SpikeError> {
        // Special route: panic AFTER a suspension point (Test 8).
        if env.route_key == "panic.AfterAwait" {
            // Await something first so the panic lands post-suspend.
            let _ = call_raw(
                ActrId { serial_number: 99 },
                "panic.warmup".to_string(),
                env.payload.clone(),
            )
            .await;
            panic!("intentional panic after await for trap-propagation test");
        }

        // Special route: guest returns an error variant (Test 7).
        if env.route_key == "err.Timeout" {
            return Err(SpikeError::Timeout("guest reported timeout".to_string()));
        }

        // Special route: Test 3 probe — record enter/exit tags via log so the host
        // can reason about interleave ordering on same-instance concurrent calls.
        if env.route_key.starts_with("trace.") {
            let route = env.route_key.clone();
            log("trace", format!("dispatch enter route={route}")).await;
            let downstream_reply =
                call_raw(ActrId { serial_number: 7 }, env.route_key, env.payload).await?;
            log("trace", format!("dispatch exit  route={route}")).await;
            let mut reply = b"async-echo: ".to_vec();
            reply.extend_from_slice(&downstream_reply);
            return Ok(reply);
        }

        // Default path: dispatch awaits a downstream call_raw, decorates, returns.
        let downstream_reply = call_raw(
            ActrId { serial_number: 42 },
            "downstream.Echo".to_string(),
            env.payload,
        )
        .await?;

        let mut reply = b"async-echo: ".to_vec();
        reply.extend_from_slice(&downstream_reply);
        Ok(reply)
    }
}

export!(EchoAsyncWorkload);

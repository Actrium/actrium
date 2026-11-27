//! A small native OCI Guest for validating a signed package end to end.

#[cfg(all(unix, feature = "oci-guest"))]
mod guest {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use actr_framework::{Context, MessageDispatcher, Workload};
    use actr_protocol::{ActorResult, ActrError, RpcEnvelope};
    use async_trait::async_trait;
    use bytes::Bytes;

    #[derive(Default)]
    pub struct Echo(AtomicUsize);

    pub(super) fn allocate_past_memory_limit() {
        let mut memory = vec![0u8; 384 * 1024 * 1024];
        for page in memory.chunks_mut(4096) {
            page[0] = 1;
        }
        std::hint::black_box(memory);
    }

    #[async_trait]
    impl Workload for Echo {
        type Dispatcher = Self;

        async fn on_start<C: Context>(&self, _: &C) -> ActorResult<()> {
            if std::env::var("ACTR_ECHO_EXHAUST_MEMORY").as_deref() == Ok("start") {
                allocate_past_memory_limit();
            }
            if let Ok(path) = std::env::var("ACTR_ECHO_DATA") {
                std::fs::write(path, b"started").map_err(|error| {
                    ActrError::Internal(format!("write startup data marker: {error}"))
                })?;
            }
            if let Ok(path) = std::env::var("ACTR_ECHO_READ_ONLY") {
                let marker = std::fs::read(&path)
                    .map_err(|error| ActrError::Internal(format!("read data marker: {error}")))?;
                if marker != b"started"
                    || std::fs::OpenOptions::new().write(true).open(path).is_ok()
                {
                    return Err(ActrError::Internal(
                        "OCI read-only grant did not preserve its access boundary".into(),
                    ));
                }
            }
            Ok(())
        }
    }

    #[async_trait]
    impl MessageDispatcher for Echo {
        type Workload = Self;

        async fn dispatch<C: Context>(
            workload: &Self,
            request: RpcEnvelope,
            context: &C,
        ) -> ActorResult<Bytes> {
            match request.route_key.as_str() {
                "echo.Echo.Ping" => {
                    workload.0.fetch_add(1, Ordering::Relaxed);
                    Ok(request.payload.unwrap_or_default())
                }
                "echo.Echo.Proxy" => {
                    let data = request.payload.unwrap_or_default();
                    context
                        .call_dependency("echo", "echo.Echo.Ping", data)
                        .await
                }
                "echo.Echo.Burst" => {
                    let calls = (0..actr_protocol::oci::MAX_HOST_REQUESTS)
                        .map(|_| context.resolve_dependency("echo"));
                    for result in futures_util::future::join_all(calls).await {
                        result?;
                    }
                    Ok(Bytes::new())
                }
                "echo.Echo.Large" => Ok(Bytes::from(vec![0; actr_protocol::oci::MAX_FRAME_BYTES])),
                "echo.Echo.Block" => {
                    if std::env::var("ACTR_ECHO_EXHAUST_MEMORY").as_deref() == Ok("dispatch") {
                        allocate_past_memory_limit();
                    }
                    std::fs::write("/data/waiting", b"running").map_err(|error| {
                        ActrError::Internal(format!("write invocation data marker: {error}"))
                    })?;
                    while !std::path::Path::new("/data/resume").exists() {
                        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                    }
                    Ok(Bytes::new())
                }
                "echo.Echo.Fetch" => context.https_request("health", Bytes::new()).await,
                _ => Err(ActrError::UnknownRoute(request.route_key)),
            }
        }
    }
}

#[cfg(all(unix, feature = "oci-guest"))]
#[tokio::main(flavor = "current_thread")]
async fn main() -> actr_protocol::ActorResult<()> {
    if std::env::var("ACTR_ECHO_EXHAUST_MEMORY").as_deref() == Ok("handshake") {
        guest::allocate_past_memory_limit();
    }
    actr_framework::guest::oci::run(guest::Echo::default()).await
}

#[cfg(not(all(unix, feature = "oci-guest")))]
fn main() {
    eprintln!("Build this example on a Unix target with the oci-guest feature.");
}

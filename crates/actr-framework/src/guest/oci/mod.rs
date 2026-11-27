//! Native Guest runner for Actr OCI containers.

mod context;
mod observation;

use std::collections::HashMap;
use std::io;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use actr_protocol::oci as wire;
use actr_protocol::{ActorResult, ActrError, ReplyMode};
use tokio::net::UnixStream;
use tokio::net::unix::{OwnedReadHalf, OwnedWriteHalf};
use tokio::sync::{Semaphore, mpsc, oneshot};

use crate::{MessageDispatcher, Workload};
use context::GuestContext;

const IO_TIMEOUT: Duration = Duration::from_secs(15);

type Reply = oneshot::Sender<ActorResult<Vec<u8>>>;
type Outbound = (wire::Frame, oneshot::Sender<io::Result<()>>);

struct Port {
    session: Vec<u8>,
    send: mpsc::Sender<Outbound>,
    pending: Mutex<HashMap<u64, Reply>>,
    slots: Semaphore,
    sequence: Mutex<u64>,
}

impl Port {
    fn enqueue(
        &self,
        sequence: u64,
        body: wire::Body,
    ) -> ActorResult<oneshot::Receiver<io::Result<()>>> {
        let frame = wire::Frame {
            session: self.session.clone(),
            sequence,
            body: Some(body),
        };
        let (complete, receipt) = oneshot::channel();
        self.send
            .try_send((frame, complete))
            .map_err(|_| ActrError::Unavailable("OCI Guest write queue is unavailable".into()))?;
        Ok(receipt)
    }

    async fn send(&self, sequence: u64, body: wire::Body) -> ActorResult<()> {
        self.enqueue(sequence, body)?
            .await
            .map_err(|_| disconnected())?
            .map_err(io_error)
    }
}

/// Connect to the Hyper listener and serve the workload until the host shuts down.
pub async fn run<W: Workload>(workload: W) -> ActorResult<()> {
    let socket = std::env::var_os(wire::SOCKET_ENV)
        .map(PathBuf::from)
        .ok_or_else(|| ActrError::InvalidArgument(format!("missing {}", wire::SOCKET_ENV)))?;
    let stream = UnixStream::connect(socket).await.map_err(io_error)?;
    run_connected(workload, stream).await
}

async fn run_connected<W: Workload>(workload: W, mut stream: UnixStream) -> ActorResult<()> {
    let frame = tokio::time::timeout(IO_TIMEOUT, wire::read_frame(&mut stream))
        .await
        .map_err(|_| ActrError::TimedOut)?
        .map_err(io_error)?;
    let Some(wire::Body::Hello(hello)) = frame.body else {
        return Err(protocol_error());
    };
    if frame.sequence != 0 || hello.major != wire::MAJOR {
        return Err(protocol_error());
    }
    let self_id = hello.actor.ok_or_else(protocol_error)?;
    let ack = wire::Frame {
        session: frame.session.clone(),
        sequence: 0,
        body: Some(wire::Body::HelloAck(wire::HelloAck {
            major: wire::MAJOR,
            minor: wire::MINOR,
            capabilities: hello.capabilities,
        })),
    };
    tokio::time::timeout(IO_TIMEOUT, wire::write_frame(&mut stream, &ack))
        .await
        .map_err(|_| ActrError::TimedOut)?
        .map_err(io_error)?;

    let (read, write) = stream.into_split();
    let (send, outbound) = mpsc::channel(wire::MAX_HOST_REQUESTS + 1);
    let (enqueue, incoming) = mpsc::channel(1);
    let port = Arc::new(Port {
        session: frame.session,
        send,
        pending: Mutex::new(HashMap::new()),
        slots: Semaphore::new(wire::MAX_HOST_REQUESTS),
        sequence: Mutex::new(1),
    });
    tokio::select! {
        result = receive(read, &port, enqueue) => result,
        result = write_frames(write, outbound) => result,
        result = serve(workload, Arc::clone(&port), self_id, incoming) => result,
    }
}

async fn receive(
    mut reader: OwnedReadHalf,
    port: &Port,
    incoming: mpsc::Sender<wire::Frame>,
) -> ActorResult<()> {
    loop {
        let mut frame = wire::read_frame(&mut reader).await.map_err(io_error)?;
        if frame.session != port.session || frame.sequence == 0 {
            return Err(protocol_error());
        }
        match frame.body.take() {
            Some(wire::Body::HostResult(result)) => {
                if let Some(reply) = port.pending.lock().unwrap().remove(&frame.sequence) {
                    let _ = reply.send(result.into_actor_result());
                }
            }
            Some(body @ (wire::Body::Invoke(_) | wire::Body::Shutdown(_))) => {
                frame.body = Some(body);
                incoming.try_send(frame).map_err(|_| protocol_error())?;
            }
            _ => return Err(protocol_error()),
        }
    }
}

async fn write_frames(
    mut writer: OwnedWriteHalf,
    mut outbound: mpsc::Receiver<Outbound>,
) -> ActorResult<()> {
    while let Some((frame, complete)) = outbound.recv().await {
        let sent = tokio::time::timeout(IO_TIMEOUT, wire::write_frame(&mut writer, &frame))
            .await
            .map_err(|_| ActrError::TimedOut)?;
        let failure = sent
            .as_ref()
            .err()
            .map(|error| ActrError::Unavailable(error.to_string()));
        let _ = complete.send(sent);
        if let Some(error) = failure {
            return Err(error);
        }
    }
    Err(disconnected())
}

async fn serve<W: Workload>(
    workload: W,
    port: Arc<Port>,
    self_id: actr_protocol::ActrId,
    mut incoming: mpsc::Receiver<wire::Frame>,
) -> ActorResult<()> {
    let mut previous = 0;
    while let Some(frame) = incoming.recv().await {
        if frame.sequence <= previous {
            return Err(protocol_error());
        }
        previous = frame.sequence;
        match frame.body {
            Some(wire::Body::Invoke(invocation))
                if invocation.token.len() == wire::SESSION_BYTES =>
            {
                let active = Arc::new(AtomicBool::new(true));
                let ctx = GuestContext {
                    port: Arc::clone(&port),
                    active: Arc::clone(&active),
                    token: invocation.token.clone(),
                    self_id: self_id.clone(),
                    caller: invocation.caller.clone(),
                    request_id: invocation.request_id.clone(),
                    expects_response: invocation.expects_response,
                };
                let result = invoke(&workload, invocation, &ctx).await;
                active.store(false, Ordering::Release);
                port.send(frame.sequence, wire::Body::InvokeResult(result.into()))
                    .await?;
            }
            Some(wire::Body::Shutdown(_)) => {
                return port
                    .send(frame.sequence, wire::Body::ShutdownAck(wire::Empty {}))
                    .await;
            }
            _ => return Err(protocol_error()),
        }
    }
    Err(disconnected())
}

async fn invoke<W: Workload>(
    workload: &W,
    invocation: wire::Invocation,
    context: &GuestContext,
) -> ActorResult<Vec<u8>> {
    match invocation.kind() {
        wire::InvocationKind::Start => workload.on_start(context).await?,
        wire::InvocationKind::Ready => workload.on_ready(context).await?,
        wire::InvocationKind::Stop => workload.on_stop(context).await?,
        wire::InvocationKind::Dispatch => {
            let envelope = invocation.envelope.ok_or_else(protocol_error)?;
            if W::Dispatcher::reply_mode_for_route(&envelope.route_key) != ReplyMode::Return {
                return Err(ActrError::NotImplemented(
                    "OCI requires a RETURN RPC route".into(),
                ));
            }
            return W::Dispatcher::dispatch(workload, envelope, context)
                .await
                .map(|body| body.to_vec());
        }
        wire::InvocationKind::Observe => {
            observation::deliver(
                workload,
                context,
                invocation.observation.ok_or_else(protocol_error)?,
            )
            .await?;
        }
        wire::InvocationKind::Unspecified => return Err(protocol_error()),
    }
    Ok(Vec::new())
}

fn protocol_error() -> ActrError {
    ActrError::DecodeFailure("invalid OCI IPC message".into())
}

fn disconnected() -> ActrError {
    ActrError::Unavailable("OCI IPC disconnected".into())
}

fn io_error(error: io::Error) -> ActrError {
    ActrError::Unavailable(format!("OCI IPC: {error}"))
}

#[cfg(test)]
mod tests {
    use actr_protocol::{ActrId, RpcEnvelope};
    use async_trait::async_trait;
    use bytes::Bytes;

    use super::*;
    use crate::Context;

    struct Probe;

    #[async_trait]
    impl Workload for Probe {
        type Dispatcher = Probe;

        async fn on_start<C: Context>(&self, context: &C) -> ActorResult<()> {
            let response = context
                .https_request("probe", Bytes::from_static(b"request"))
                .await?;
            assert_eq!(response, b"ack"[..]);
            Ok(())
        }
    }

    #[async_trait]
    impl MessageDispatcher for Probe {
        type Workload = Probe;

        async fn dispatch<C: Context>(
            _: &Probe,
            envelope: RpcEnvelope,
            _: &C,
        ) -> ActorResult<Bytes> {
            Err(ActrError::UnknownRoute(envelope.route_key))
        }
    }

    #[tokio::test]
    async fn handles_guest_callback_while_host_awaits_start() {
        let (guest, mut host) = UnixStream::pair().unwrap();
        let task = tokio::spawn(run_connected(Probe, guest));
        let session = vec![1; wire::SESSION_BYTES];
        let frame = |sequence, body| wire::Frame {
            session: session.clone(),
            sequence,
            body: Some(body),
        };
        wire::write_frame(
            &mut host,
            &frame(
                0,
                wire::Body::Hello(wire::Hello {
                    major: wire::MAJOR,
                    minor: wire::MINOR,
                    actor: Some(ActrId::default()),
                    capabilities: vec!["net.https".into()],
                }),
            ),
        )
        .await
        .unwrap();
        assert!(matches!(
            wire::read_frame(&mut host).await.unwrap().body,
            Some(wire::Body::HelloAck(_))
        ));
        let token = vec![2; wire::SESSION_BYTES];
        wire::write_frame(
            &mut host,
            &frame(
                1,
                wire::Body::Invoke(wire::Invocation {
                    kind: wire::InvocationKind::Start as i32,
                    token: token.clone(),
                    ..Default::default()
                }),
            ),
        )
        .await
        .unwrap();
        let request = wire::read_frame(&mut host).await.unwrap();
        let Some(wire::Body::HostRequest(call)) = request.body else {
            panic!("expected host callback")
        };
        assert_eq!(call.token, token);
        assert_eq!(call.alias, "probe");
        wire::write_frame(
            &mut host,
            &frame(
                request.sequence,
                wire::Body::HostResult(Ok(b"ack".to_vec()).into()),
            ),
        )
        .await
        .unwrap();
        let result = wire::read_frame(&mut host).await.unwrap();
        assert_eq!(result.sequence, 1);
        let Some(wire::Body::InvokeResult(result)) = result.body else {
            panic!("expected startup result")
        };
        assert!(result.into_actor_result().unwrap().is_empty());
        wire::write_frame(&mut host, &frame(2, wire::Body::Shutdown(wire::Empty {})))
            .await
            .unwrap();
        assert!(matches!(
            wire::read_frame(&mut host).await.unwrap().body,
            Some(wire::Body::ShutdownAck(_))
        ));
        task.await.unwrap().unwrap();
    }
}

//! Actr OCI IPC messages for the `actr.oci-ipc/1` workload boundary.

use std::io;

use prost::{Enumeration, Message, Oneof};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::{ActorResult, ActrError, ActrId, ActrType, ConnectionNotReadyInfo, RpcEnvelope};

pub const MAJOR: u32 = 1;
pub const MINOR: u32 = 0;
pub const MAX_FRAME_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_HOST_REQUESTS: usize = 32;
pub const SESSION_BYTES: usize = 16;
pub const SOCKET_ENV: &str = "ACTR_OCI_SOCKET";
pub const GUEST_SOCKET_PATH: &str = "/run/actr/guest.sock";

#[derive(Clone, PartialEq, Message)]
pub struct Frame {
    #[prost(bytes = "vec", tag = "1")]
    pub session: Vec<u8>,
    #[prost(uint64, tag = "2")]
    pub sequence: u64,
    #[prost(oneof = "Body", tags = "3,4,5,6,7,8,9,10")]
    pub body: Option<Body>,
}

#[derive(Clone, PartialEq, Oneof)]
pub enum Body {
    #[prost(message, tag = "3")]
    Hello(Hello),
    #[prost(message, tag = "4")]
    HelloAck(HelloAck),
    #[prost(message, tag = "5")]
    Invoke(Invocation),
    #[prost(message, tag = "6")]
    InvokeResult(OperationResult),
    #[prost(message, tag = "7")]
    HostRequest(HostRequest),
    #[prost(message, tag = "8")]
    HostResult(OperationResult),
    #[prost(message, tag = "9")]
    Shutdown(Empty),
    #[prost(message, tag = "10")]
    ShutdownAck(Empty),
}

#[derive(Clone, PartialEq, Message)]
pub struct Empty {}

#[derive(Clone, PartialEq, Message)]
pub struct Hello {
    #[prost(uint32, tag = "1")]
    pub major: u32,
    #[prost(uint32, tag = "2")]
    pub minor: u32,
    #[prost(message, optional, tag = "3")]
    pub actor: Option<ActrId>,
    #[prost(string, repeated, tag = "4")]
    pub capabilities: Vec<String>,
}

#[derive(Clone, PartialEq, Message)]
pub struct HelloAck {
    #[prost(uint32, tag = "1")]
    pub major: u32,
    #[prost(uint32, tag = "2")]
    pub minor: u32,
    #[prost(string, repeated, tag = "3")]
    pub capabilities: Vec<String>,
}

#[derive(Clone, PartialEq, Message)]
pub struct Invocation {
    #[prost(bytes = "vec", tag = "1")]
    pub token: Vec<u8>,
    #[prost(message, optional, tag = "2")]
    pub caller: Option<ActrId>,
    #[prost(string, tag = "3")]
    pub request_id: String,
    #[prost(bool, tag = "4")]
    pub expects_response: bool,
    #[prost(enumeration = "InvocationKind", tag = "5")]
    pub kind: i32,
    #[prost(message, optional, tag = "6")]
    pub envelope: Option<RpcEnvelope>,
    #[prost(message, optional, tag = "7")]
    pub observation: Option<Observation>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Enumeration)]
#[repr(i32)]
pub enum InvocationKind {
    Unspecified = 0,
    Start = 1,
    Ready = 2,
    Stop = 3,
    Dispatch = 4,
    Observe = 5,
}

#[derive(Clone, PartialEq, Message)]
pub struct Observation {
    #[prost(enumeration = "EventKind", tag = "1")]
    pub kind: i32,
    #[prost(message, optional, tag = "2")]
    pub peer: Option<ActrId>,
    #[prost(bool, optional, tag = "3")]
    pub relayed: Option<bool>,
    #[prost(uint32, optional, tag = "4")]
    pub peer_status: Option<u32>,
    #[prost(uint64, tag = "5")]
    pub expires_seconds: u64,
    #[prost(uint32, tag = "6")]
    pub expires_nanos: u32,
    #[prost(uint64, tag = "7")]
    pub queue_len: u64,
    #[prost(uint64, tag = "8")]
    pub threshold: u64,
    #[prost(uint64, tag = "9")]
    pub pc_generation: u64,
    #[prost(uint64, tag = "10")]
    pub revision: u64,
    #[prost(bool, tag = "11")]
    pub sendable: bool,
    #[prost(uint32, tag = "12")]
    pub network_path: u32,
    #[prost(uint32, optional, tag = "13")]
    pub unavailable_reason: Option<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Enumeration)]
#[repr(i32)]
pub enum EventKind {
    Unspecified = 0,
    SignalingConnecting = 1,
    SignalingConnected = 2,
    SignalingDisconnected = 3,
    WebsocketConnecting = 4,
    WebsocketConnected = 5,
    WebsocketDisconnected = 6,
    WebrtcConnecting = 7,
    WebrtcConnected = 8,
    WebrtcDisconnected = 9,
    CredentialRenewed = 10,
    CredentialExpiring = 11,
    MailboxBackpressure = 12,
    WebrtcStateChanged = 13,
}

#[derive(Clone, PartialEq, Message)]
pub struct HostRequest {
    #[prost(bytes = "vec", tag = "1")]
    pub token: Vec<u8>,
    #[prost(enumeration = "HostOperation", tag = "2")]
    pub operation: i32,
    #[prost(string, tag = "3")]
    pub alias: String,
    #[prost(string, tag = "4")]
    pub route: String,
    #[prost(message, optional, tag = "5")]
    pub target: Option<ActrId>,
    #[prost(message, optional, tag = "6")]
    pub actor_type: Option<ActrType>,
    #[prost(bytes = "vec", tag = "7")]
    pub payload: Vec<u8>,
    #[prost(int64, tag = "8")]
    pub timeout_ms: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Enumeration)]
#[repr(i32)]
pub enum HostOperation {
    Unspecified = 0,
    Call = 1,
    Tell = 2,
    Resolve = 3,
    Discover = 4,
    Https = 5,
}

#[derive(Clone, PartialEq, Message)]
pub struct OperationResult {
    #[prost(oneof = "OperationOutcome", tags = "1, 2")]
    pub outcome: Option<OperationOutcome>,
}

#[derive(Clone, PartialEq, Oneof)]
pub enum OperationOutcome {
    #[prost(bytes, tag = "1")]
    Response(Vec<u8>),
    #[prost(message, tag = "2")]
    Failure(Failure),
}

#[derive(Clone, PartialEq, Message)]
pub struct Failure {
    #[prost(enumeration = "ErrorCode", tag = "1")]
    pub code: i32,
    #[prost(string, tag = "2")]
    pub message: String,
    #[prost(string, tag = "3")]
    pub detail: String,
    #[prost(uint64, optional, tag = "4")]
    pub retry_after_ms: Option<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Enumeration)]
#[repr(i32)]
pub enum ErrorCode {
    Internal = 0,
    Unavailable = 1,
    TimedOut = 2,
    DeliveryUncertain = 3,
    NotFound = 4,
    PermissionDenied = 5,
    InvalidArgument = 6,
    UnknownRoute = 7,
    DependencyNotFound = 8,
    DecodeFailure = 9,
    NotImplemented = 10,
    ConnectionNotReady = 11,
}

impl From<ActorResult<Vec<u8>>> for OperationResult {
    fn from(result: ActorResult<Vec<u8>>) -> Self {
        let outcome = match result {
            Ok(payload) => OperationOutcome::Response(payload),
            Err(error) => OperationOutcome::Failure(error.into()),
        };
        Self {
            outcome: Some(outcome),
        }
    }
}

impl OperationResult {
    pub fn into_actor_result(self) -> ActorResult<Vec<u8>> {
        match self.outcome {
            Some(OperationOutcome::Response(payload)) => Ok(payload),
            Some(OperationOutcome::Failure(error)) => Err(error.into()),
            None => Err(ActrError::DecodeFailure(
                "missing OCI operation result".into(),
            )),
        }
    }
}

impl From<ActrError> for Failure {
    fn from(error: ActrError) -> Self {
        use ErrorCode as Code;
        let mut detail = String::new();
        let mut retry_after_ms = None;
        let (code, message) = match error {
            ActrError::Unavailable(message) => (Code::Unavailable, message),
            ActrError::TimedOut => (Code::TimedOut, String::new()),
            ActrError::DeliveryUncertain {
                operation_id,
                cause,
            } => {
                detail = operation_id;
                (Code::DeliveryUncertain, cause)
            }
            ActrError::NotFound(message) => (Code::NotFound, message),
            ActrError::PermissionDenied(message) => (Code::PermissionDenied, message),
            ActrError::InvalidArgument(message) => (Code::InvalidArgument, message),
            ActrError::UnknownRoute(route) => (Code::UnknownRoute, route),
            ActrError::DependencyNotFound {
                service_name,
                message,
            } => {
                detail = service_name;
                (Code::DependencyNotFound, message)
            }
            ActrError::DecodeFailure(message) => (Code::DecodeFailure, message),
            ActrError::NotImplemented(message) => (Code::NotImplemented, message),
            ActrError::ConnectionNotReady(info) => {
                retry_after_ms = info.retry_after_ms;
                (Code::ConnectionNotReady, String::new())
            }
            ActrError::Internal(message) => (Code::Internal, message),
        };
        Self {
            code: code as i32,
            message,
            detail,
            retry_after_ms,
        }
    }
}

impl From<Failure> for ActrError {
    fn from(value: Failure) -> Self {
        use ErrorCode as Code;
        let Failure {
            code,
            message,
            detail,
            retry_after_ms,
        } = value;
        match Code::try_from(code).unwrap_or(Code::Internal) {
            Code::Internal => Self::Internal(message),
            Code::Unavailable => Self::Unavailable(message),
            Code::TimedOut => Self::TimedOut,
            Code::DeliveryUncertain => Self::DeliveryUncertain {
                operation_id: detail,
                cause: message,
            },
            Code::NotFound => Self::NotFound(message),
            Code::PermissionDenied => Self::PermissionDenied(message),
            Code::InvalidArgument => Self::InvalidArgument(message),
            Code::UnknownRoute => Self::UnknownRoute(message),
            Code::DependencyNotFound => Self::DependencyNotFound {
                service_name: detail,
                message,
            },
            Code::DecodeFailure => Self::DecodeFailure(message),
            Code::NotImplemented => Self::NotImplemented(message),
            Code::ConnectionNotReady => {
                Self::ConnectionNotReady(ConnectionNotReadyInfo { retry_after_ms })
            }
        }
    }
}

/// Reads one complete frame. A reader must be retained across calls or closed on cancellation.
pub async fn read_frame(reader: &mut (impl AsyncRead + Unpin)) -> io::Result<Frame> {
    let length = reader.read_u32().await? as usize;
    check_length(length)?;
    let mut payload = vec![0; length];
    reader.read_exact(&mut payload).await?;
    let frame = Frame::decode(payload.as_slice()).map_err(io::Error::other)?;
    if frame.session.len() != SESSION_BYTES || frame.body.is_none() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid OCI IPC envelope",
        ));
    }
    Ok(frame)
}

/// Writes one complete frame. A writer error terminates the session.
pub async fn write_frame(writer: &mut (impl AsyncWrite + Unpin), frame: &Frame) -> io::Result<()> {
    let length = frame.encoded_len();
    check_length(length)?;
    writer.write_u32(length as u32).await?;
    writer.write_all(&frame.encode_to_vec()).await
}

fn check_length(length: usize) -> io::Result<()> {
    if (1..=MAX_FRAME_BYTES).contains(&length) {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid OCI IPC frame length",
        ))
    }
}

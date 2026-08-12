//! application.ist Host API v0.1 native binding.
//!
//! The public functions below cover the closed v0.1 catalog (32 methods and 15
//! events). Complex closed records use [`Value`] until generated record structs
//! are added; byte fields still use KBS1 segments, never JSON number arrays.
//! Byte sinks borrow guest memory for the whole future, making concurrent guest
//! access a Rust type error in safe code.

use crate::store_net_encode::{decode_reply, encode_value, Encoded, Value};
use crate::{host_call, HostCall};
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;
use core::future::Future;
use core::marker::PhantomData;
use core::pin::Pin;
use core::task::{Context, Poll};

pub const API_VERSION: &str = "0.1";

pub const METHODS: [&str; 32] = [
    "canvas.getInfo",
    "canvas.draw",
    "canvas.present",
    "canvas.measureText",
    "asset.read",
    "asset.loadFont",
    "playback.open",
    "playback.write",
    "playback.stop",
    "input.focus",
    "ime.openSession",
    "ime.updateState",
    "ime.updateGeometry",
    "ime.closeSession",
    "capture.requestCamera",
    "capture.requestMic",
    "capture.frame",
    "capture.readAudio",
    "capture.stop",
    "store.get",
    "store.put",
    "store.delete",
    "store.list",
    "store.deletePrefix",
    "clipboard.readText",
    "clipboard.writeText",
    "net.fetch",
    "identity.status",
    "identity.login",
    "identity.logout",
    "env.snapshot",
    "print.open",
];

pub const EVENTS: [&str; 15] = [
    "canvas.frame",
    "canvas.resize",
    "playback.ready",
    "playback.ended",
    "input.pointer",
    "input.key",
    "input.wheel",
    "input.focusChange",
    "ime.textUpdate",
    "ime.composition",
    "ime.formatUpdate",
    "ime.characterBoundsRequest",
    "ime.ended",
    "capture.ended",
    "env.change",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostError {
    pub method: String,
    pub code: String,
    pub retryable: bool,
}

impl HostError {
    fn internal(method: &str) -> Self {
        Self {
            method: method.to_string(),
            code: "internal".to_string(),
            retryable: false,
        }
    }

    fn cancelled(method: &str) -> Self {
        Self {
            method: method.to_string(),
            code: "cancelled".to_string(),
            retryable: false,
        }
    }

    fn from_wire(method: &str, bytes: &[u8]) -> Self {
        let Ok(Value::Obj(fields)) = decode_reply(bytes) else {
            return Self::internal(method);
        };
        let mut wire_method = None;
        let mut code = None;
        let mut retryable = None;
        for (key, value) in fields {
            match (key.as_str(), value) {
                ("method", Value::Str(value)) => wire_method = Some(value),
                ("code", Value::Str(value)) => code = Some(value),
                ("retryable", Value::Bool(value)) => retryable = Some(value),
                _ => {}
            }
        }
        match (wire_method, code, retryable) {
            (Some(wire_method), Some(code), Some(retryable)) if wire_method == method => Self {
                method: wire_method,
                code,
                retryable,
            },
            _ => Self::internal(method),
        }
    }
}

/// Opaque, typed session handle. Construct only from a schema-checked host reply.
#[derive(Debug, PartialEq, Eq)]
pub struct Handle<K: HandleKind> {
    id: u32,
    _kind: PhantomData<K>,
}

pub trait HandleKind {
    const WIRE_KIND: &'static str;
}
pub enum Font {}
impl HandleKind for Font {
    const WIRE_KIND: &'static str = "font";
}
pub enum AudioOutput {}
impl HandleKind for AudioOutput {
    const WIRE_KIND: &'static str = "audio-output";
}
pub enum ImeSession {}
impl HandleKind for ImeSession {
    const WIRE_KIND: &'static str = "ime-session";
}
pub enum CameraStream {}
impl HandleKind for CameraStream {
    const WIRE_KIND: &'static str = "camera-stream";
}
pub enum MicrophoneStream {}
impl HandleKind for MicrophoneStream {
    const WIRE_KIND: &'static str = "microphone-stream";
}

impl<K: HandleKind> Handle<K> {
    pub fn id(&self) -> u32 {
        self.id
    }

    pub fn from_value(value: &Value) -> Option<Self> {
        let Value::Obj(fields) = value else {
            return None;
        };
        let mut id = None;
        let mut kind = None;
        for (key, value) in fields {
            match (key.as_str(), value) {
                ("id", Value::Num(value))
                    if *value >= 0.0
                        && *value <= u32::MAX as f64
                        && *value == (*value as u32) as f64 =>
                {
                    id = Some(*value as u32)
                }
                ("kind", Value::Str(value)) => kind = Some(value.as_str()),
                _ => {}
            }
        }
        match (id, kind) {
            (Some(id), Some(kind)) if kind == K::WIRE_KIND => Some(Self {
                id,
                _kind: PhantomData,
            }),
            _ => None,
        }
    }
}

/// An exclusive native ByteSink borrow encoded as a checked pointer/capacity.
pub struct ByteSink<'a> {
    ptr: u32,
    cap: u32,
    _borrow: PhantomData<&'a mut [u8]>,
}

impl<'a> ByteSink<'a> {
    pub fn new(bytes: &'a mut [u8]) -> Result<Self, HostError> {
        let ptr = bytes.as_mut_ptr() as usize;
        if ptr > u32::MAX as usize || bytes.len() > u32::MAX as usize || bytes.is_empty() {
            return Err(HostError {
                method: "runtime.bytesink".to_string(),
                code: "limit-exceeded".to_string(),
                retryable: false,
            });
        }
        Ok(Self {
            ptr: ptr as u32,
            cap: bytes.len() as u32,
            _borrow: PhantomData,
        })
    }

    fn value(&self) -> Value {
        obj(vec![("ptr", num(self.ptr)), ("cap", num(self.cap))])
    }
}

pub struct V01Call<'a> {
    inner: HostCall,
    method: &'static str,
    cancelled_before_send: bool,
    preflight_error: Option<HostError>,
    _borrow: PhantomData<&'a mut [u8]>,
}

impl Future for V01Call<'_> {
    type Output = Result<Value, HostError>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = &mut *self;
        if let Some(error) = this.preflight_error.take() {
            return Poll::Ready(Err(error));
        }
        match Pin::new(&mut this.inner).poll(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready((_ok, _bytes)) if this.cancelled_before_send => {
                Poll::Ready(Err(HostError::cancelled(this.method)))
            }
            Poll::Ready((ok, bytes)) if ok == 1 => {
                Poll::Ready(decode_reply(&bytes).map_err(|_| HostError::internal(this.method)))
            }
            Poll::Ready((_ok, bytes)) => {
                Poll::Ready(Err(HostError::from_wire(this.method, &bytes)))
            }
        }
    }
}

pub struct CancellableV01Call<'a>(V01Call<'a>);

impl CancellableV01Call<'_> {
    pub fn cancel(&mut self) {
        if !self.0.inner_was_sent() {
            self.0.cancelled_before_send = true;
            // Cancellation before entry wins without encoding or dispatch.
            self.0.preflight_error = None;
        }
        self.0.inner.cancel();
    }
}

impl Future for CancellableV01Call<'_> {
    type Output = Result<Value, HostError>;
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        Pin::new(&mut self.0).poll(cx)
    }
}

impl V01Call<'_> {
    fn inner_was_sent(&self) -> bool {
        self.inner.was_sent()
    }
}

fn num(value: u32) -> Value {
    Value::Num(value as f64)
}
fn string(value: &str) -> Value {
    Value::Str(value.to_string())
}
fn u64_value(value: u64) -> Value {
    Value::Str(value.to_string())
}
fn obj(entries: Vec<(&str, Value)>) -> Value {
    Value::Obj(
        entries
            .into_iter()
            .map(|(key, value)| (key.to_string(), value))
            .collect(),
    )
}
fn handle<K: HandleKind>(value: &Handle<K>) -> Value {
    obj(vec![("id", num(value.id)), ("kind", string(K::WIRE_KIND))])
}

fn encode(input: Value) -> Result<Vec<u8>, HostError> {
    match encode_value(&input) {
        Ok(Encoded::Plain(bytes) | Encoded::Envelope(bytes)) => Ok(bytes),
        Err(_) => Err(HostError {
            method: "runtime.encode".to_string(),
            code: "limit-exceeded".to_string(),
            retryable: false,
        }),
    }
}

fn call<'a>(
    module: &'static str,
    method: &'static str,
    logical: &'static str,
    input: Value,
) -> V01Call<'a> {
    let (bytes, preflight_error) = match encode(input) {
        Ok(bytes) => (bytes, None),
        Err(mut error) => {
            error.method = logical.to_string();
            (Vec::new(), Some(error))
        }
    };
    V01Call {
        inner: host_call(module, method, bytes),
        method: logical,
        cancelled_before_send: false,
        preflight_error,
        _borrow: PhantomData,
    }
}
fn cancellable<'a>(
    module: &'static str,
    method: &'static str,
    logical: &'static str,
    input: Value,
) -> CancellableV01Call<'a> {
    CancellableV01Call(call(module, method, logical, input))
}

pub mod canvas {
    use super::*;
    pub fn get_info(canvas_id: &str) -> V01Call<'static> {
        call(
            "host:canvas",
            "getInfo",
            "canvas.getInfo",
            obj(vec![("canvasId", string(canvas_id))]),
        )
    }
    pub fn draw(canvas_id: &str, operations: Value) -> V01Call<'static> {
        call(
            "host:canvas",
            "draw",
            "canvas.draw",
            obj(vec![
                ("canvasId", string(canvas_id)),
                ("operations", operations),
            ]),
        )
    }
    pub fn present(canvas_id: &str, pixels: &[u8], frame: Value) -> V01Call<'static> {
        call(
            "host:canvas",
            "present",
            "canvas.present",
            obj(vec![
                ("canvasId", string(canvas_id)),
                ("pixels", Value::Bytes(pixels.to_vec())),
                ("frame", frame),
            ]),
        )
    }
    pub fn measure_text(canvas_id: &str, texts: Value, style: Value) -> V01Call<'static> {
        call(
            "host:canvas",
            "measureText",
            "canvas.measureText",
            obj(vec![
                ("canvasId", string(canvas_id)),
                ("texts", texts),
                ("style", style),
            ]),
        )
    }
}

pub mod asset {
    use super::*;
    pub fn read<'a>(asset_id: &str, output: ByteSink<'a>, offset: u32) -> V01Call<'a> {
        call(
            "host:asset",
            "read",
            "asset.read",
            obj(vec![
                ("assetId", string(asset_id)),
                ("output", output.value()),
                ("offset", num(offset)),
            ]),
        )
    }
    pub fn load_font(asset_id: &str) -> V01Call<'static> {
        call(
            "host:asset",
            "loadFont",
            "asset.loadFont",
            obj(vec![("assetId", string(asset_id))]),
        )
    }
}

pub mod playback {
    use super::*;
    pub fn open(options: Value) -> V01Call<'static> {
        call(
            "host:playback",
            "open",
            "playback.open",
            obj(vec![("options", options)]),
        )
    }
    pub fn write(output: &Handle<AudioOutput>, samples: &[u8]) -> V01Call<'static> {
        call(
            "host:playback",
            "write",
            "playback.write",
            obj(vec![
                ("output", handle(output)),
                ("samples", Value::Bytes(samples.to_vec())),
            ]),
        )
    }
    pub fn stop(output: Handle<AudioOutput>, mode: &str) -> V01Call<'static> {
        call(
            "host:playback",
            "stop",
            "playback.stop",
            obj(vec![("output", handle(&output)), ("mode", string(mode))]),
        )
    }
}

pub mod input {
    use super::*;
    pub fn focus(canvas_id: &str) -> V01Call<'static> {
        call(
            "host:input",
            "focus",
            "input.focus",
            obj(vec![("canvasId", string(canvas_id))]),
        )
    }
}

pub mod ime {
    use super::*;
    pub fn open_session(canvas_id: &str, state: Value) -> V01Call<'static> {
        call(
            "host:ime",
            "openSession",
            "ime.openSession",
            obj(vec![("canvasId", string(canvas_id)), ("state", state)]),
        )
    }
    pub fn update_state(session: &Handle<ImeSession>, state: Value) -> V01Call<'static> {
        call(
            "host:ime",
            "updateState",
            "ime.updateState",
            obj(vec![("session", handle(session)), ("state", state)]),
        )
    }
    pub fn update_geometry(session: &Handle<ImeSession>, geometry: Value) -> V01Call<'static> {
        call(
            "host:ime",
            "updateGeometry",
            "ime.updateGeometry",
            obj(vec![("session", handle(session)), ("geometry", geometry)]),
        )
    }
    pub fn close_session(session: Handle<ImeSession>) -> V01Call<'static> {
        call(
            "host:ime",
            "closeSession",
            "ime.closeSession",
            obj(vec![("session", handle(&session))]),
        )
    }
}

pub enum CaptureHandle<'a> {
    Camera(&'a Handle<CameraStream>),
    Microphone(&'a Handle<MicrophoneStream>),
}

pub mod capture {
    use super::*;
    pub fn request_camera(constraints: Value) -> CancellableV01Call<'static> {
        cancellable(
            "host:capture",
            "requestCamera",
            "capture.requestCamera",
            obj(vec![("constraints", constraints)]),
        )
    }
    pub fn request_mic(constraints: Value) -> CancellableV01Call<'static> {
        cancellable(
            "host:capture",
            "requestMic",
            "capture.requestMic",
            obj(vec![("constraints", constraints)]),
        )
    }
    pub fn frame<'a>(
        camera: &Handle<CameraStream>,
        output: ByteSink<'a>,
        after_sequence: Option<u64>,
    ) -> CancellableV01Call<'a> {
        cancellable(
            "host:capture",
            "frame",
            "capture.frame",
            obj(vec![
                ("camera", handle(camera)),
                ("output", output.value()),
                (
                    "afterSequence",
                    after_sequence.map(u64_value).unwrap_or(Value::Null),
                ),
            ]),
        )
    }
    pub fn read_audio<'a>(
        microphone: &Handle<MicrophoneStream>,
        output: ByteSink<'a>,
    ) -> CancellableV01Call<'a> {
        cancellable(
            "host:capture",
            "readAudio",
            "capture.readAudio",
            obj(vec![
                ("microphone", handle(microphone)),
                ("output", output.value()),
            ]),
        )
    }
    pub fn stop(handle_value: CaptureHandle<'_>) -> V01Call<'static> {
        let value = match handle_value {
            CaptureHandle::Camera(v) => handle(v),
            CaptureHandle::Microphone(v) => handle(v),
        };
        call(
            "host:capture",
            "stop",
            "capture.stop",
            obj(vec![("handle", value)]),
        )
    }
}

pub mod store {
    use super::*;
    pub fn get(key: &str) -> V01Call<'static> {
        call(
            "host:store",
            "get",
            "store.get",
            obj(vec![("key", string(key))]),
        )
    }
    pub fn put(key: &str, value: &[u8], condition: Value) -> V01Call<'static> {
        call(
            "host:store",
            "put",
            "store.put",
            obj(vec![
                ("key", string(key)),
                ("value", Value::Bytes(value.to_vec())),
                ("condition", condition),
            ]),
        )
    }
    pub fn delete(key: &str, condition: Value) -> V01Call<'static> {
        call(
            "host:store",
            "delete",
            "store.delete",
            obj(vec![("key", string(key)), ("condition", condition)]),
        )
    }
    pub fn list(prefix: Option<&str>, cursor: Option<&str>) -> V01Call<'static> {
        call(
            "host:store",
            "list",
            "store.list",
            obj(vec![
                ("prefix", prefix.map(string).unwrap_or(Value::Null)),
                ("cursor", cursor.map(string).unwrap_or(Value::Null)),
            ]),
        )
    }
    pub fn delete_prefix(prefix: &str) -> V01Call<'static> {
        call(
            "host:store",
            "deletePrefix",
            "store.deletePrefix",
            obj(vec![("prefix", string(prefix))]),
        )
    }
}

pub mod clipboard {
    use super::*;
    pub fn read_text() -> CancellableV01Call<'static> {
        cancellable(
            "host:clipboard",
            "readText",
            "clipboard.readText",
            Value::Null,
        )
    }
    pub fn write_text(text: &str) -> CancellableV01Call<'static> {
        cancellable(
            "host:clipboard",
            "writeText",
            "clipboard.writeText",
            obj(vec![("text", string(text))]),
        )
    }
}

pub mod net {
    use super::*;
    pub fn fetch(request: Value, body: Option<&[u8]>) -> CancellableV01Call<'static> {
        cancellable(
            "host:net",
            "fetch",
            "net.fetch",
            obj(vec![
                ("request", request),
                (
                    "body",
                    body.map(|v| Value::Bytes(v.to_vec()))
                        .unwrap_or(Value::Null),
                ),
            ]),
        )
    }
}

pub mod identity {
    use super::*;
    pub fn status() -> V01Call<'static> {
        call("host:identity", "status", "identity.status", Value::Null)
    }
    pub fn login() -> CancellableV01Call<'static> {
        cancellable("host:identity", "login", "identity.login", Value::Null)
    }
    pub fn logout() -> CancellableV01Call<'static> {
        cancellable("host:identity", "logout", "identity.logout", Value::Null)
    }
}

pub mod env {
    use super::*;
    pub fn snapshot() -> V01Call<'static> {
        call("host:env", "snapshot", "env.snapshot", Value::Null)
    }
}
pub mod print {
    use super::*;
    pub fn open(document: &[u8]) -> CancellableV01Call<'static> {
        cancellable(
            "host:print",
            "open",
            "print.open",
            obj(vec![("document", Value::Bytes(document.to_vec()))]),
        )
    }
}

pub mod events {
    macro_rules! event_fn {
        ($name:ident, $wire:literal) => {
            pub fn $name(handler: impl Fn(&[u8]) + 'static) -> u32 {
                crate::events::on($wire, handler)
            }
        };
    }
    event_fn!(canvas_frame, "canvas.frame");
    event_fn!(canvas_resize, "canvas.resize");
    event_fn!(playback_ready, "playback.ready");
    event_fn!(playback_ended, "playback.ended");
    event_fn!(input_pointer, "input.pointer");
    event_fn!(input_key, "input.key");
    event_fn!(input_wheel, "input.wheel");
    event_fn!(input_focus_change, "input.focusChange");
    event_fn!(ime_text_update, "ime.textUpdate");
    event_fn!(ime_composition, "ime.composition");
    event_fn!(ime_format_update, "ime.formatUpdate");
    event_fn!(ime_character_bounds_request, "ime.characterBoundsRequest");
    event_fn!(ime_ended, "ime.ended");
    event_fn!(capture_ended, "capture.ended");
    event_fn!(env_change, "env.change");
    pub fn off(id: u32) {
        crate::events::off(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::collections::BTreeSet;
    use core::task::{Context, Poll, Waker};

    #[test]
    fn catalog_is_closed_unique_and_complete() {
        assert_eq!(METHODS.len(), 32);
        assert_eq!(EVENTS.len(), 15);
        assert_eq!(
            METHODS.iter().copied().collect::<BTreeSet<_>>().len(),
            METHODS.len()
        );
        assert_eq!(
            EVENTS.iter().copied().collect::<BTreeSet<_>>().len(),
            EVENTS.len()
        );
    }

    #[test]
    fn handles_reject_wrong_kind_and_fractional_ids() {
        let wrong = obj(vec![("id", num(7)), ("kind", string("camera-stream"))]);
        assert!(Handle::<AudioOutput>::from_value(&wrong).is_none());
        let fractional = obj(vec![
            ("id", Value::Num(1.5)),
            ("kind", string("audio-output")),
        ]);
        assert!(Handle::<AudioOutput>::from_value(&fractional).is_none());
        let right = obj(vec![("id", num(7)), ("kind", string("audio-output"))]);
        assert_eq!(Handle::<AudioOutput>::from_value(&right).unwrap().id(), 7);
    }

    #[test]
    fn host_error_fails_closed_on_malformed_or_cross_method_reply() {
        assert_eq!(
            HostError::from_wire("net.fetch", b"not-json").code,
            "internal"
        );
        assert_eq!(
            HostError::from_wire(
                "net.fetch",
                br#"{"method":"store.get","code":"denied","retryable":false}"#
            )
            .code,
            "internal"
        );
        assert_eq!(
            HostError::from_wire(
                "net.fetch",
                br#"{"method":"net.fetch","code":"denied","retryable":false}"#
            )
            .code,
            "denied"
        );
    }

    #[test]
    fn cancellation_before_first_poll_issues_no_host_call() {
        let _guard = crate::wire_lock();
        crate::test_shims::CALLS.lock().unwrap().clear();
        let mut call = identity::login();
        call.cancel();
        let mut cx = Context::from_waker(Waker::noop());
        let result = Pin::new(&mut call).poll(&mut cx);
        assert_eq!(
            result,
            Poll::Ready(Err(HostError::cancelled("identity.login")))
        );
        assert!(crate::test_shims::CALLS.lock().unwrap().is_empty());
    }

    #[test]
    fn byte_source_is_snapshotted_when_the_sdk_call_is_created() {
        let _guard = crate::wire_lock();
        crate::test_shims::CALLS.lock().unwrap().clear();
        let wire = obj(vec![("id", num(3)), ("kind", string("audio-output"))]);
        let output = Handle::<AudioOutput>::from_value(&wire).unwrap();
        let mut samples = Vec::from(&b"first"[..]);
        let mut call = playback::write(&output, &samples);
        samples.fill(b'x');
        let mut cx = Context::from_waker(Waker::noop());
        assert!(matches!(Pin::new(&mut call).poll(&mut cx), Poll::Pending));
        let calls = crate::test_shims::CALLS.lock().unwrap();
        assert_eq!(calls.len(), 1);
        let Value::Obj(fields) = decode_reply(&calls[0].2).unwrap() else {
            panic!("request must be an object")
        };
        let snap = fields
            .into_iter()
            .find(|(key, _)| key == "samples")
            .unwrap()
            .1;
        assert_eq!(snap, Value::Bytes(Vec::from(&b"first"[..])));
    }
}

//! application.ist Host API v0.1 native binding.
//!
//! The public functions below cover the closed v0.1 catalog (27 methods and 15
//! events). Public method and event signatures use closed typed records; byte
//! fields use KBS1 segments, never JSON number arrays. Byte sinks borrow guest
//! memory for the whole future, making concurrent guest access a Rust type error
//! in safe code.

pub use crate::host_api_v01_types::*;
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

pub const METHODS: [&str; 27] = [
    "canvas.getInfo",
    "canvas.draw",
    "canvas.present",
    "canvas.measureText",
    "asset.read",
    "asset.loadFont",
    "playback.open",
    "input.focus",
    "ime.openSession",
    "capture.requestCamera",
    "capture.requestMic",
    "store.get",
    "store.put",
    "store.delete",
    "store.list",
    "store.deletePrefix",
    "clipboard.readText",
    "clipboard.writeText",
    "env.snapshot",
    "playback.write",
    "playback.stop",
    "ime.updateState",
    "ime.updateGeometry",
    "ime.closeSession",
    "capture.frame",
    "capture.readAudio",
    "capture.stop",
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

/// FNV-1a/64 lock for `METHODS` then `EVENTS`, each UTF-8 name followed by a
/// zero byte. Downstream conformance tests can compare this without parsing
/// Rust source; changing either closed set requires an explicit lock update.
pub const CATALOG_LOCK_FNV1A64: u64 = 0xbe9f_8b25_220e_c718;

#[cfg(test)]
fn catalog_fingerprint() -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for group in [&METHODS[..], &EVENTS[..]] {
        for name in group {
            for byte in name.as_bytes().iter().chain(core::iter::once(&0)) {
                hash ^= *byte as u64;
                hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
            }
        }
    }
    hash
}

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
        // HostError is a closed wire record.  Rejecting unknown and duplicate
        // fields is security-relevant: otherwise a producer and consumer can
        // disagree about which duplicate value was authenticated.
        if fields.len() != 3 {
            return Self::internal(method);
        }
        let mut wire_method = None;
        let mut code = None;
        let mut retryable = None;
        for (key, value) in fields {
            match (key.as_str(), value) {
                ("method", Value::Str(value)) if wire_method.is_none() => wire_method = Some(value),
                ("code", Value::Str(value)) if code.is_none() => code = Some(value),
                ("retryable", Value::Bool(value)) if retryable.is_none() => retryable = Some(value),
                _ => return Self::internal(method),
            }
        }
        match (wire_method, code, retryable) {
            (Some(wire_method), Some(code), Some(retryable))
                if wire_method == method && valid_error_code(&code) =>
            {
                Self {
                    method: wire_method,
                    code,
                    retryable,
                }
            }
            _ => Self::internal(method),
        }
    }
}

/// Keep the native decoder aligned with the public v0.1 lexical contract and
/// the TypeScript host boundary: one lower-case ASCII letter followed by at
/// most 62 lower-case letters, digits or hyphens.
fn valid_error_code(code: &str) -> bool {
    let bytes = code.as_bytes();
    matches!(bytes.first(), Some(b'a'..=b'z'))
        && bytes.len() <= 63
        && bytes[1..]
            .iter()
            .all(|byte| matches!(byte, b'a'..=b'z' | b'0'..=b'9' | b'-'))
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
#[derive(Debug, PartialEq, Eq)]
pub enum Font {}
impl HandleKind for Font {
    const WIRE_KIND: &'static str = "font";
}
#[derive(Debug, PartialEq, Eq)]
pub enum AudioOutput {}
impl HandleKind for AudioOutput {
    const WIRE_KIND: &'static str = "audio-output";
}
#[derive(Debug, PartialEq, Eq)]
pub enum ImeSession {}
impl HandleKind for ImeSession {
    const WIRE_KIND: &'static str = "ime-session";
}
#[derive(Debug, PartialEq, Eq)]
pub enum CameraStream {}
impl HandleKind for CameraStream {
    const WIRE_KIND: &'static str = "camera-stream";
}
#[derive(Debug, PartialEq, Eq)]
pub enum MicrophoneStream {}
impl HandleKind for MicrophoneStream {
    const WIRE_KIND: &'static str = "microphone-stream";
}

impl<K: HandleKind> Handle<K> {
    pub fn id(&self) -> u32 {
        self.id
    }

    pub(crate) fn from_value(value: &Value) -> Option<Self> {
        let Value::Obj(fields) = value else {
            return None;
        };
        if fields.len() != 2 {
            return None;
        }
        let mut id = None;
        let mut kind = None;
        for (key, value) in fields {
            match (key.as_str(), value) {
                ("id", Value::Num(value))
                    if id.is_none()
                        && *value >= 1.0
                        && *value <= u32::MAX as f64
                        && *value == (*value as u32) as f64 =>
                {
                    id = Some(*value as u32)
                }
                ("kind", Value::Str(value)) if kind.is_none() => kind = Some(value.as_str()),
                _ => return None,
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

    pub(crate) fn wire_value(&self) -> Value {
        handle(self)
    }
}

/// An exclusive native ByteSink borrow.
///
/// Only its capacity crosses the asynchronous ABI. The host returns a bounded
/// KBS1 `bytes` field; this SDK copies it into the borrowed slice only after the
/// whole success record validates. A dropped/cancelled future therefore cannot
/// leave the host holding a stale guest pointer.
pub struct ByteSink<'a> {
    bytes: &'a mut [u8],
}

impl<'a> ByteSink<'a> {
    pub fn new(bytes: &'a mut [u8]) -> Result<Self, HostError> {
        if bytes.len() > u32::MAX as usize || bytes.is_empty() {
            return Err(HostError {
                method: "runtime.bytesink".to_string(),
                code: "limit-exceeded".to_string(),
                retryable: false,
            });
        }
        Ok(Self { bytes })
    }

    fn into_wire(self) -> (Value, &'a mut [u8]) {
        let capacity = self.bytes.len() as u32;
        (obj(vec![("capacity", num(capacity))]), self.bytes)
    }
}

pub struct V01Call<'a, T> {
    inner: HostCall,
    method: &'static str,
    decode: fn(Value) -> Option<T>,
    sink: Option<&'a mut [u8]>,
    sink_written: Option<fn(&T) -> Option<usize>>,
    cancellation_requested: bool,
    preflight_error: Option<HostError>,
    _borrow: PhantomData<&'a mut [u8]>,
}

impl<T> Future for V01Call<'_, T> {
    type Output = Result<T, HostError>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = &mut *self;
        if let Some(error) = this.preflight_error.take() {
            return Poll::Ready(Err(error));
        }
        match Pin::new(&mut this.inner).poll(cx) {
            Poll::Pending => Poll::Pending,
            // Cancellation wins even if a completion crossed the ABI just
            // before the guest was re-polled. We still poll the inner future to
            // consume that completion and avoid leaving an orphan result.
            Poll::Ready((_ok, _bytes)) if this.cancellation_requested => {
                Poll::Ready(Err(HostError::cancelled(this.method)))
            }
            Poll::Ready((1, bytes)) => {
                let decoded = decode_reply(&bytes).ok().and_then(|mut value| {
                    let returned_bytes = if this.sink.is_some() {
                        match &mut value {
                            Value::Null => None,
                            Value::Obj(fields) => {
                                let indexes = fields
                                    .iter()
                                    .enumerate()
                                    .filter_map(|(index, (key, _))| {
                                        (key == "bytes").then_some(index)
                                    })
                                    .collect::<Vec<_>>();
                                if indexes.len() != 1 {
                                    return None;
                                }
                                let (_, value) = fields.remove(indexes[0]);
                                let Value::Bytes(bytes) = value else {
                                    return None;
                                };
                                Some(bytes)
                            }
                            _ => return None,
                        }
                    } else {
                        None
                    };
                    let output = (this.decode)(value)?;
                    if let (Some(sink), Some(written)) = (&mut this.sink, this.sink_written) {
                        match (written(&output), returned_bytes) {
                            (None, None) => {}
                            (Some(expected), Some(bytes))
                                if expected == bytes.len() && bytes.len() <= sink.len() =>
                            {
                                sink[..bytes.len()].copy_from_slice(&bytes);
                            }
                            _ => return None,
                        }
                    }
                    Some(output)
                });
                Poll::Ready(decoded.ok_or_else(|| HostError {
                    method: this.method.to_string(),
                    code: "protocol-error".to_string(),
                    retryable: false,
                }))
            }
            Poll::Ready((_ok, bytes)) => {
                Poll::Ready(Err(HostError::from_wire(this.method, &bytes)))
            }
        }
    }
}

pub struct CancellableV01Call<'a, T>(V01Call<'a, T>);

impl<T> CancellableV01Call<'_, T> {
    pub fn cancel(&mut self) {
        if self.0.cancellation_requested {
            return;
        }
        self.0.cancellation_requested = true;
        if !self.0.inner_was_sent() {
            // Cancellation before entry wins without encoding or dispatch.
            self.0.preflight_error = None;
        }
        self.0.inner.cancel();
    }
}

impl<T> Future for CancellableV01Call<'_, T> {
    type Output = Result<T, HostError>;
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        Pin::new(&mut self.0).poll(cx)
    }
}

impl<T> V01Call<'_, T> {
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

fn call<'a, T>(
    module: &'static str,
    method: &'static str,
    logical: &'static str,
    input: Value,
    decode: fn(Value) -> Option<T>,
) -> V01Call<'a, T> {
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
        decode,
        sink: None,
        sink_written: None,
        cancellation_requested: false,
        preflight_error,
        _borrow: PhantomData,
    }
}
fn call_sink<'a, T: SinkOutput>(
    module: &'static str,
    method: &'static str,
    logical: &'static str,
    input: Value,
    sink: &'a mut [u8],
    decode: fn(Value) -> Option<T>,
) -> V01Call<'a, T> {
    let mut call = call(module, method, logical, input, decode);
    call.sink = Some(sink);
    call.sink_written = Some(SinkOutput::written);
    call
}
fn cancellable_sink<'a, T: SinkOutput>(
    module: &'static str,
    method: &'static str,
    logical: &'static str,
    input: Value,
    sink: &'a mut [u8],
    decode: fn(Value) -> Option<T>,
) -> CancellableV01Call<'a, T> {
    CancellableV01Call(call_sink(module, method, logical, input, sink, decode))
}
fn cancellable<'a, T>(
    module: &'static str,
    method: &'static str,
    logical: &'static str,
    input: Value,
    decode: fn(Value) -> Option<T>,
) -> CancellableV01Call<'a, T> {
    CancellableV01Call(call(module, method, logical, input, decode))
}

pub mod canvas {
    use super::*;
    pub fn get_info(canvas_id: &str) -> V01Call<'static, Option<CanvasInfo>> {
        call(
            "host:canvas",
            "getInfo",
            METHODS[0],
            obj(vec![("canvasId", string(canvas_id))]),
            decode_optional::<CanvasInfo>,
        )
    }
    pub fn draw(canvas_id: &str, operations: Vec<DrawOperation>) -> V01Call<'static, ()> {
        call(
            "host:canvas",
            "draw",
            METHODS[1],
            obj(vec![
                ("canvasId", string(canvas_id)),
                (
                    "operations",
                    Value::Arr(operations.into_iter().map(encode_wire).collect()),
                ),
            ]),
            decode_void,
        )
    }
    pub fn present(canvas_id: &str, pixels: &[u8], frame: FrameInfo) -> V01Call<'static, ()> {
        call(
            "host:canvas",
            "present",
            METHODS[2],
            obj(vec![
                ("canvasId", string(canvas_id)),
                ("pixels", Value::Bytes(pixels.to_vec())),
                ("frame", encode_wire(frame)),
            ]),
            decode_void,
        )
    }
    pub fn measure_text(
        canvas_id: &str,
        texts: Vec<String>,
        style: TextStyle,
    ) -> V01Call<'static, Vec<TextMetrics>> {
        call(
            "host:canvas",
            "measureText",
            METHODS[3],
            obj(vec![
                ("canvasId", string(canvas_id)),
                (
                    "texts",
                    Value::Arr(texts.into_iter().map(Value::Str).collect()),
                ),
                ("style", encode_wire(style)),
            ]),
            decode_vec::<TextMetrics>,
        )
    }
}

pub mod asset {
    use super::*;
    pub fn read<'a>(
        asset_id: &str,
        output: ByteSink<'a>,
        offset: u64,
    ) -> V01Call<'a, AssetReadResult> {
        let (output, sink) = output.into_wire();
        call_sink(
            "host:asset",
            "read",
            METHODS[4],
            obj(vec![
                ("assetId", string(asset_id)),
                ("output", output),
                ("offset", u64_value(offset)),
            ]),
            sink,
            decode_wire::<AssetReadResult>,
        )
    }
    pub fn load_font(asset_id: &str) -> V01Call<'static, Handle<Font>> {
        call(
            "host:asset",
            "loadFont",
            METHODS[5],
            obj(vec![("assetId", string(asset_id))]),
            decode_wire::<Handle<Font>>,
        )
    }
}

pub mod playback {
    use super::*;
    pub fn open(options: PlaybackOptions) -> V01Call<'static, PlaybackStream> {
        call(
            "host:playback",
            "open",
            METHODS[6],
            obj(vec![("options", encode_wire(options))]),
            decode_wire::<PlaybackStream>,
        )
    }
    pub fn write(
        output: &Handle<AudioOutput>,
        samples: &[u8],
    ) -> V01Call<'static, PlaybackWriteResult> {
        call(
            "host:playback",
            "write",
            METHODS[19],
            obj(vec![
                ("output", handle(output)),
                ("samples", Value::Bytes(samples.to_vec())),
            ]),
            decode_wire::<PlaybackWriteResult>,
        )
    }
    pub fn stop(output: Handle<AudioOutput>, mode: StopMode) -> V01Call<'static, ()> {
        call(
            "host:playback",
            "stop",
            METHODS[20],
            obj(vec![
                ("output", handle(&output)),
                ("mode", encode_wire(mode)),
            ]),
            decode_void,
        )
    }
}

pub mod input {
    use super::*;
    pub fn focus(canvas_id: &str) -> V01Call<'static, FocusResult> {
        call(
            "host:input",
            "focus",
            METHODS[7],
            obj(vec![("canvasId", string(canvas_id))]),
            decode_wire::<FocusResult>,
        )
    }
}

pub mod ime {
    use super::*;
    pub fn open_session(canvas_id: &str, state: ImeState) -> V01Call<'static, Handle<ImeSession>> {
        call(
            "host:ime",
            "openSession",
            METHODS[8],
            obj(vec![
                ("canvasId", string(canvas_id)),
                ("state", encode_wire(state)),
            ]),
            decode_wire::<Handle<ImeSession>>,
        )
    }
    pub fn update_state(session: &Handle<ImeSession>, state: ImeState) -> V01Call<'static, ()> {
        call(
            "host:ime",
            "updateState",
            METHODS[21],
            obj(vec![
                ("session", handle(session)),
                ("state", encode_wire(state)),
            ]),
            decode_void,
        )
    }
    pub fn update_geometry(
        session: &Handle<ImeSession>,
        geometry: ImeGeometry,
    ) -> V01Call<'static, ()> {
        call(
            "host:ime",
            "updateGeometry",
            METHODS[22],
            obj(vec![
                ("session", handle(session)),
                ("geometry", encode_wire(geometry)),
            ]),
            decode_void,
        )
    }
    pub fn close_session(session: Handle<ImeSession>) -> V01Call<'static, ()> {
        call(
            "host:ime",
            "closeSession",
            METHODS[23],
            obj(vec![("session", handle(&session))]),
            decode_void,
        )
    }
}

pub enum CaptureHandle<'a> {
    Camera(&'a Handle<CameraStream>),
    Microphone(&'a Handle<MicrophoneStream>),
}

pub mod capture {
    use super::*;
    pub fn request_camera(
        constraints: CameraConstraints,
    ) -> CancellableV01Call<'static, Option<CameraSession>> {
        cancellable(
            "host:capture",
            "requestCamera",
            METHODS[9],
            obj(vec![("constraints", encode_wire(constraints))]),
            decode_optional::<CameraSession>,
        )
    }
    pub fn request_mic(
        constraints: MicrophoneConstraints,
    ) -> CancellableV01Call<'static, Option<MicrophoneSession>> {
        cancellable(
            "host:capture",
            "requestMic",
            METHODS[10],
            obj(vec![("constraints", encode_wire(constraints))]),
            decode_optional::<MicrophoneSession>,
        )
    }
    pub fn frame<'a>(
        camera: &Handle<CameraStream>,
        output: ByteSink<'a>,
        after_sequence: Option<u64>,
    ) -> CancellableV01Call<'a, VideoFrameResult> {
        let (output, sink) = output.into_wire();
        cancellable_sink(
            "host:capture",
            "frame",
            METHODS[24],
            obj(vec![
                ("camera", handle(camera)),
                ("output", output),
                (
                    "afterSequence",
                    after_sequence.map(u64_value).unwrap_or(Value::Null),
                ),
            ]),
            sink,
            decode_wire::<VideoFrameResult>,
        )
    }
    pub fn read_audio<'a>(
        microphone: &Handle<MicrophoneStream>,
        output: ByteSink<'a>,
    ) -> CancellableV01Call<'a, AudioReadResult> {
        let (output, sink) = output.into_wire();
        cancellable_sink(
            "host:capture",
            "readAudio",
            METHODS[25],
            obj(vec![("microphone", handle(microphone)), ("output", output)]),
            sink,
            decode_wire::<AudioReadResult>,
        )
    }
    pub fn stop(handle_value: CaptureHandle<'_>) -> V01Call<'static, ()> {
        let value = match handle_value {
            CaptureHandle::Camera(v) => handle(v),
            CaptureHandle::Microphone(v) => handle(v),
        };
        call(
            "host:capture",
            "stop",
            METHODS[26],
            obj(vec![("handle", value)]),
            decode_void,
        )
    }
}

pub mod store {
    use super::*;
    pub fn get<'a>(key: &str, output: ByteSink<'a>) -> V01Call<'a, Option<StoreGetResult>> {
        let (output, sink) = output.into_wire();
        call_sink(
            "host:store",
            "get",
            METHODS[11],
            obj(vec![("key", string(key)), ("output", output)]),
            sink,
            decode_optional::<StoreGetResult>,
        )
    }
    pub fn put(
        key: &str,
        value: &[u8],
        condition: StoreCondition,
    ) -> V01Call<'static, VersionResult> {
        call(
            "host:store",
            "put",
            METHODS[12],
            obj(vec![
                ("key", string(key)),
                ("value", Value::Bytes(value.to_vec())),
                ("condition", encode_wire(condition)),
            ]),
            decode_wire::<VersionResult>,
        )
    }
    pub fn delete(key: &str, condition: StoreDeleteCondition) -> V01Call<'static, DeletedResult> {
        call(
            "host:store",
            "delete",
            METHODS[13],
            obj(vec![
                ("key", string(key)),
                ("condition", encode_wire(condition)),
            ]),
            decode_wire::<DeletedResult>,
        )
    }
    pub fn list(prefix: Option<&str>, cursor: Option<&str>) -> V01Call<'static, StoreListPage> {
        call(
            "host:store",
            "list",
            METHODS[14],
            obj(vec![
                ("prefix", prefix.map(string).unwrap_or(Value::Null)),
                ("cursor", cursor.map(string).unwrap_or(Value::Null)),
            ]),
            decode_wire::<StoreListPage>,
        )
    }
    pub fn delete_prefix(prefix: &str) -> V01Call<'static, DeleteCount> {
        call(
            "host:store",
            "deletePrefix",
            METHODS[15],
            obj(vec![("prefix", string(prefix))]),
            decode_wire::<DeleteCount>,
        )
    }
}

pub mod clipboard {
    use super::*;
    pub fn read_text<'a>(
        output: ByteSink<'a>,
    ) -> CancellableV01Call<'a, Option<ClipboardTextResult>> {
        let (output, sink) = output.into_wire();
        cancellable_sink(
            "host:clipboard",
            "readText",
            METHODS[16],
            obj(vec![("output", output)]),
            sink,
            decode_optional::<ClipboardTextResult>,
        )
    }
    pub fn write_text(text: &str) -> CancellableV01Call<'static, ()> {
        cancellable(
            "host:clipboard",
            "writeText",
            METHODS[17],
            obj(vec![("text", string(text))]),
            decode_void,
        )
    }
}

pub mod env {
    use super::*;
    pub fn snapshot() -> V01Call<'static, EnvironmentSnapshot> {
        call(
            "host:env",
            "snapshot",
            METHODS[18],
            Value::Null,
            decode_wire::<EnvironmentSnapshot>,
        )
    }
}

pub mod events {
    use super::*;

    fn typed<T: WireDecode + 'static>(wire: &'static str, handler: impl Fn(T) + 'static) -> u32 {
        crate::events::on(wire, move |bytes| {
            if let Ok(value) = decode_reply(bytes) {
                if let Some(payload) = T::decode(value) {
                    handler(payload);
                }
            }
        })
    }

    macro_rules! event_fn {
        ($name:ident, $wire:expr, $payload:ty) => {
            pub fn $name(handler: impl Fn($payload) + 'static) -> u32 {
                typed($wire, handler)
            }
        };
    }
    event_fn!(canvas_frame, EVENTS[0], CanvasFrameEvent);
    event_fn!(canvas_resize, EVENTS[1], CanvasResizeEvent);
    event_fn!(playback_ready, EVENTS[2], PlaybackReadyEvent);
    event_fn!(playback_ended, EVENTS[3], PlaybackEndedEvent);
    event_fn!(input_pointer, EVENTS[4], PointerEvent);
    event_fn!(input_key, EVENTS[5], KeyEvent);
    event_fn!(input_wheel, EVENTS[6], WheelEvent);
    event_fn!(input_focus_change, EVENTS[7], FocusChangeEvent);
    event_fn!(ime_text_update, EVENTS[8], ImeTextUpdateEvent);
    event_fn!(ime_composition, EVENTS[9], ImeCompositionEvent);
    event_fn!(ime_format_update, EVENTS[10], ImeFormatUpdateEvent);
    event_fn!(
        ime_character_bounds_request,
        EVENTS[11],
        ImeCharacterBoundsRequestEvent
    );
    event_fn!(ime_ended, EVENTS[12], ImeEndedEvent);
    event_fn!(capture_ended, EVENTS[13], CaptureEndedEvent);
    event_fn!(env_change, EVENTS[14], EnvironmentChangeEvent);
    pub fn off(id: u32) {
        crate::events::off(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::boxed::Box;
    use alloc::collections::BTreeSet;
    use alloc::rc::Rc;
    use core::cell::Cell;
    use core::fmt::Debug;
    use core::task::{Context, Poll, Waker};

    fn finish<F, T>(mut call: F, ok: u32, reply: &[u8]) -> Result<T, HostError>
    where
        F: Future<Output = Result<T, HostError>> + Unpin,
        T: Debug + PartialEq,
    {
        let mut cx = Context::from_waker(Waker::noop());
        assert!(matches!(Pin::new(&mut call).poll(&mut cx), Poll::Pending));
        let cb = crate::test_shims::CALLS.lock().unwrap().last().unwrap().3;
        // Safety: reply remains valid for the duration of resolve.
        unsafe { crate::rt::resolve(cb, ok, reply.as_ptr(), reply.len()) };
        match Pin::new(&mut call).poll(&mut cx) {
            Poll::Ready(result) => result,
            Poll::Pending => panic!("resolved call remained pending"),
        }
    }

    fn fake_sink() -> ByteSink<'static> {
        ByteSink::new(Box::leak(vec![0; 64].into_boxed_slice())).unwrap()
    }

    fn finish_value<F, T>(call: F, value: Value) -> Result<T, HostError>
    where
        F: Future<Output = Result<T, HostError>> + Unpin,
        T: Debug + PartialEq,
    {
        let wire = match encode_value(&value).unwrap() {
            Encoded::Plain(bytes) | Encoded::Envelope(bytes) => bytes,
        };
        finish(call, 1, &wire)
    }

    #[test]
    fn catalog_is_closed_unique_and_complete() {
        assert_eq!(METHODS.len(), 27);
        assert_eq!(EVENTS.len(), 15);
        assert_eq!(
            METHODS.iter().copied().collect::<BTreeSet<_>>().len(),
            METHODS.len()
        );
        assert_eq!(
            EVENTS.iter().copied().collect::<BTreeSet<_>>().len(),
            EVENTS.len()
        );
        assert_eq!(catalog_fingerprint(), CATALOG_LOCK_FNV1A64);
    }

    #[test]
    fn public_wrappers_dispatch_the_closed_method_catalog() {
        fn record<F: Future + Unpin>(mut call: F) {
            let mut cx = Context::from_waker(Waker::noop());
            assert!(matches!(Pin::new(&mut call).poll(&mut cx), Poll::Pending));
        }
        fn sink() -> ByteSink<'static> {
            ByteSink::new(Box::leak(vec![0; 4].into_boxed_slice())).unwrap()
        }
        fn audio() -> Handle<AudioOutput> {
            Handle::from_value(&obj(vec![
                ("id", num(1)),
                ("kind", string(AudioOutput::WIRE_KIND)),
            ]))
            .unwrap()
        }
        fn ime_session() -> Handle<ImeSession> {
            Handle::from_value(&obj(vec![
                ("id", num(2)),
                ("kind", string(ImeSession::WIRE_KIND)),
            ]))
            .unwrap()
        }
        fn camera() -> Handle<CameraStream> {
            Handle::from_value(&obj(vec![
                ("id", num(3)),
                ("kind", string(CameraStream::WIRE_KIND)),
            ]))
            .unwrap()
        }
        fn microphone() -> Handle<MicrophoneStream> {
            Handle::from_value(&obj(vec![
                ("id", num(4)),
                ("kind", string(MicrophoneStream::WIRE_KIND)),
            ]))
            .unwrap()
        }

        let _guard = crate::wire_lock();
        crate::test_shims::CALLS.lock().unwrap().clear();
        crate::test_shims::CANCELS.lock().unwrap().clear();
        record(canvas::get_info("main"));
        record(canvas::draw("main", vec![]));
        record(canvas::present(
            "main",
            &[],
            FrameInfo {
                width: 1,
                height: 1,
                stride: 4,
                format: PixelFormat::Rgba8,
                sequence: 0,
            },
        ));
        record(canvas::measure_text(
            "main",
            vec![],
            TextStyle {
                font: None,
                size: 12.0,
                align: TextAlign::Start,
                direction: TextDirection::Ltr,
            },
        ));
        record(asset::read("asset", sink(), 0));
        record(asset::load_font("font"));
        record(playback::open(PlaybackOptions::default()));
        record(playback::write(&audio(), &[]));
        record(playback::stop(audio(), StopMode::Immediate));
        record(input::focus("main"));
        let state = || ImeState {
            text: String::new(),
            selection: TextRange { start: 0, end: 0 },
            composition: None,
            input_mode: InputMode::Text,
            multiline: false,
            revision: 0,
        };
        let geometry = || ImeGeometry {
            revision: 0,
            editor_bounds: Rect {
                x: 0.0,
                y: 0.0,
                width: 1.0,
                height: 1.0,
            },
            caret_bounds: Rect {
                x: 0.0,
                y: 0.0,
                width: 1.0,
                height: 1.0,
            },
            character_bounds: None,
        };
        record(ime::open_session("main", state()));
        record(ime::update_state(&ime_session(), state()));
        record(ime::update_geometry(&ime_session(), geometry()));
        record(ime::close_session(ime_session()));
        record(capture::request_camera(CameraConstraints::default()));
        record(capture::request_mic(MicrophoneConstraints::default()));
        record(capture::frame(&camera(), sink(), None));
        record(capture::read_audio(&microphone(), sink()));
        let camera = camera();
        record(capture::stop(CaptureHandle::Camera(&camera)));
        record(store::get("key", sink()));
        record(store::put("key", &[], StoreCondition::Any));
        record(store::delete("key", StoreDeleteCondition::Any));
        record(store::list(None, None));
        record(store::delete_prefix("prefix"));
        record(clipboard::read_text(sink()));
        record(clipboard::write_text("text"));
        record(env::snapshot());

        let actual = crate::test_shims::CALLS
            .lock()
            .unwrap()
            .iter()
            .map(|(module, method, _, _)| {
                alloc::format!(
                    "{}.{}",
                    module.strip_prefix("host:").unwrap_or(module),
                    method
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(actual.len(), METHODS.len());
        assert_eq!(
            actual.into_iter().collect::<BTreeSet<_>>(),
            METHODS
                .iter()
                .map(|method| (*method).to_string())
                .collect::<BTreeSet<_>>()
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

        for malformed in [
            obj(vec![("id", num(0)), ("kind", string("audio-output"))]),
            obj(vec![
                ("id", num(7)),
                ("kind", string("audio-output")),
                ("extra", Value::Bool(true)),
            ]),
            obj(vec![
                ("id", num(7)),
                ("id", num(8)),
                ("kind", string("audio-output")),
            ]),
            obj(vec![
                ("id", num(7)),
                ("kind", string("audio-output")),
                ("kind", string("audio-output")),
            ]),
        ] {
            assert!(Handle::<AudioOutput>::from_value(&malformed).is_none());
        }
    }

    #[test]
    fn host_error_fails_closed_on_malformed_or_cross_method_reply() {
        assert_eq!(
            HostError::from_wire("store.get", b"not-json").code,
            "internal"
        );
        assert_eq!(
            HostError::from_wire(
                "store.get",
                br#"{"method":"store.put","code":"denied","retryable":false}"#
            )
            .code,
            "internal"
        );
        assert_eq!(
            HostError::from_wire(
                "store.get",
                br#"{"method":"store.get","code":"denied","retryable":false}"#
            )
            .code,
            "denied"
        );
        let longest_code = "a".repeat(63);
        let longest_wire =
            alloc::format!(r#"{{"method":"store.get","code":"{longest_code}","retryable":false}}"#);
        assert_eq!(
            HostError::from_wire("store.get", longest_wire.as_bytes()).code,
            longest_code
        );
        let oversized_code = "a".repeat(64);
        let oversized_wire = alloc::format!(
            r#"{{"method":"store.get","code":"{oversized_code}","retryable":false}}"#
        );
        assert_eq!(
            HostError::from_wire("store.get", oversized_wire.as_bytes()).code,
            "internal"
        );

        for malformed in [
            br#"{"method":"store.get","code":"denied","retryable":false,"debug":"secret"}"#
                .as_slice(),
            br#"{"method":"store.get","method":"store.get","code":"denied","retryable":false}"#
                .as_slice(),
            br#"{"method":"store.get","code":"denied","code":"timeout","retryable":false}"#
                .as_slice(),
            br#"{"method":"store.get","code":"Denied","retryable":false}"#.as_slice(),
            br#"{"method":"store.get","code":"a_underscore","retryable":false}"#.as_slice(),
            br#"{"method":"store.get","code":"","retryable":false}"#.as_slice(),
        ] {
            assert_eq!(
                HostError::from_wire("store.get", malformed).code,
                "internal"
            );
        }
    }

    #[test]
    fn method_success_records_reject_extra_wrong_u64_and_handle_fields() {
        let _guard = crate::wire_lock();
        crate::test_shims::CALLS.lock().unwrap().clear();

        assert_eq!(
            finish(input::focus("main"), 1, br#"{"focused":true}"#).unwrap(),
            FocusResult { focused: true }
        );
        for malformed in [
            br#"{"focused":true,"identity":"secret"}"#.as_slice(),
            br#"{"focused":"true"}"#.as_slice(),
            br#"{"focused":true,"focused":false}"#.as_slice(),
        ] {
            assert_eq!(
                finish(input::focus("main"), 1, malformed).unwrap_err().code,
                "protocol-error"
            );
        }

        for malformed in [
            br#"{"version":1}"#.as_slice(),
            br#"{"version":"01"}"#.as_slice(),
            br#"{"version":"18446744073709551616"}"#.as_slice(),
            br#"{"version":"1","extra":false}"#.as_slice(),
        ] {
            assert_eq!(
                finish(store::put("key", &[], StoreCondition::Any), 1, malformed)
                    .unwrap_err()
                    .code,
                "protocol-error"
            );
        }

        for malformed in [
            br#"{"id":0,"kind":"font"}"#.as_slice(),
            br#"{"id":1,"kind":"camera-stream"}"#.as_slice(),
            br#"{"id":1,"kind":"font","rights":["use"]}"#.as_slice(),
            br#"{"id":1,"id":2,"kind":"font"}"#.as_slice(),
        ] {
            assert_eq!(
                finish(asset::load_font("font"), 1, malformed)
                    .unwrap_err()
                    .code,
                "protocol-error"
            );
        }
    }

    #[test]
    fn byte_sink_is_pointer_free_atomic_and_length_checked() {
        let _guard = crate::wire_lock();
        crate::test_shims::CALLS.lock().unwrap().clear();

        let mut success = [9u8; 4];
        let result = finish_value(
            asset::read("asset", ByteSink::new(&mut success).unwrap(), 0),
            obj(vec![
                ("written", num(2)),
                ("totalSize", string("2")),
                ("eof", Value::Bool(true)),
                ("bytes", Value::Bytes(vec![1, 2])),
            ]),
        )
        .unwrap();
        assert_eq!(result.written, 2);
        assert_eq!(success, [1, 2, 9, 9]);

        let request =
            decode_reply(&crate::test_shims::CALLS.lock().unwrap().last().unwrap().2).unwrap();
        let Value::Obj(fields) = request else {
            panic!("request must be an object")
        };
        let Value::Obj(sink) = fields
            .into_iter()
            .find(|(key, _)| key == "output")
            .unwrap()
            .1
        else {
            panic!("sink must be an object")
        };
        assert_eq!(sink, vec![("capacity".to_string(), num(4))]);

        for response in [
            obj(vec![
                ("written", num(3)),
                ("totalSize", string("3")),
                ("eof", Value::Bool(true)),
                ("bytes", Value::Bytes(vec![1, 2])),
            ]),
            obj(vec![
                ("written", num(2)),
                ("totalSize", string("2")),
                ("eof", Value::Bool(true)),
                ("bytes", Value::Bytes(vec![1, 2])),
                ("debug", Value::Bool(true)),
            ]),
            obj(vec![
                ("written", num(5)),
                ("totalSize", string("5")),
                ("eof", Value::Bool(true)),
                ("bytes", Value::Bytes(vec![1, 2, 3, 4, 5])),
            ]),
        ] {
            let mut untouched = [7u8; 4];
            assert_eq!(
                finish_value(
                    asset::read("asset", ByteSink::new(&mut untouched).unwrap(), 0),
                    response
                )
                .unwrap_err()
                .code,
                "protocol-error"
            );
            assert_eq!(untouched, [7; 4]);
        }
    }

    #[test]
    fn every_method_uses_its_typed_success_decoder() {
        let _guard = crate::wire_lock();
        crate::test_shims::CALLS.lock().unwrap().clear();
        macro_rules! ok {
            ($call:expr, $reply:literal) => {
                assert!(finish($call, 1, $reply).is_ok())
            };
        }
        let audio = || {
            Handle::<AudioOutput>::from_value(&obj(vec![
                ("id", num(1)),
                ("kind", string("audio-output")),
            ]))
            .unwrap()
        };
        let ime = || {
            Handle::<ImeSession>::from_value(&obj(vec![
                ("id", num(2)),
                ("kind", string("ime-session")),
            ]))
            .unwrap()
        };
        let camera = || {
            Handle::<CameraStream>::from_value(&obj(vec![
                ("id", num(3)),
                ("kind", string("camera-stream")),
            ]))
            .unwrap()
        };
        let microphone = || {
            Handle::<MicrophoneStream>::from_value(&obj(vec![
                ("id", num(4)),
                ("kind", string("microphone-stream")),
            ]))
            .unwrap()
        };
        let state = || ImeState {
            text: String::new(),
            selection: TextRange { start: 0, end: 0 },
            composition: None,
            input_mode: InputMode::Text,
            multiline: false,
            revision: 0,
        };
        let geometry = || ImeGeometry {
            revision: 0,
            editor_bounds: Rect {
                x: 0.0,
                y: 0.0,
                width: 1.0,
                height: 1.0,
            },
            caret_bounds: Rect {
                x: 0.0,
                y: 0.0,
                width: 1.0,
                height: 1.0,
            },
            character_bounds: None,
        };

        ok!(canvas::get_info("main"), br#"{"width":1,"height":1,"dpr":1,"mode":"draw","safeArea":{"top":0,"right":0,"bottom":0,"left":0}}"#);
        ok!(canvas::draw("main", vec![]), b"null");
        ok!(
            canvas::present(
                "main",
                &[0; 4],
                FrameInfo {
                    width: 1,
                    height: 1,
                    stride: 4,
                    format: PixelFormat::Rgba8,
                    sequence: 0
                }
            ),
            b"null"
        );
        ok!(
            canvas::measure_text(
                "main",
                vec!["x".into()],
                TextStyle {
                    font: None,
                    size: 12.0,
                    align: TextAlign::Start,
                    direction: TextDirection::Ltr
                }
            ),
            br#"[{"advance":4}]"#
        );
        assert!(finish_value(
            asset::read("asset", fake_sink(), 0),
            obj(vec![
                ("written", num(0)),
                ("totalSize", string("0")),
                ("eof", Value::Bool(true)),
                ("bytes", Value::Bytes(vec![]))
            ])
        )
        .is_ok());
        ok!(asset::load_font("font"), br#"{"id":5,"kind":"font"}"#);
        ok!(playback::open(PlaybackOptions::default()), br#"{"output":{"id":1,"kind":"audio-output"},"sampleRate":48000,"channels":2,"maxQueuedFrames":1024}"#);
        ok!(
            playback::write(&audio(), &[0; 8]),
            br#"{"acceptedFrames":1,"queuedFrames":1}"#
        );
        ok!(playback::stop(audio(), StopMode::Immediate), b"null");
        ok!(input::focus("main"), br#"{"focused":true}"#);
        ok!(
            ime::open_session("main", state()),
            br#"{"id":2,"kind":"ime-session"}"#
        );
        ok!(ime::update_state(&ime(), state()), b"null");
        ok!(ime::update_geometry(&ime(), geometry()), b"null");
        ok!(ime::close_session(ime()), b"null");
        ok!(
            capture::request_camera(CameraConstraints::default()),
            b"null"
        );
        ok!(
            capture::request_mic(MicrophoneConstraints::default()),
            b"null"
        );
        assert!(finish_value(
            capture::frame(&camera(), fake_sink(), None),
            obj(vec![
                ("written", num(4)),
                ("width", num(1)),
                ("height", num(1)),
                ("stride", num(4)),
                ("format", string("rgba8")),
                ("sequence", string("1")),
                ("bytes", Value::Bytes(vec![1, 2, 3, 4]))
            ])
        )
        .is_ok());
        assert!(finish_value(
            capture::read_audio(&microphone(), fake_sink()),
            obj(vec![
                ("written", num(4)),
                ("sampleRate", num(48000)),
                ("channels", num(1)),
                ("format", string("f32le")),
                ("frames", num(1)),
                ("sequence", string("0")),
                ("droppedFrames", num(0)),
                ("bytes", Value::Bytes(vec![0; 4]))
            ])
        )
        .is_ok());
        let camera_handle = camera();
        ok!(
            capture::stop(CaptureHandle::Camera(&camera_handle)),
            b"null"
        );
        ok!(store::get("key", fake_sink()), b"null");
        ok!(
            store::put("key", &[], StoreCondition::Any),
            br#"{"version":"1"}"#
        );
        ok!(
            store::delete("key", StoreDeleteCondition::Any),
            br#"{"deleted":true}"#
        );
        ok!(store::list(None, None), br#"{"items":[]}"#);
        ok!(store::delete_prefix("prefix"), br#"{"deleted":"0"}"#);
        ok!(clipboard::read_text(fake_sink()), b"null");
        ok!(clipboard::write_text("text"), b"null");
        ok!(env::snapshot(), br#"{"colorScheme":"light","locale":{"language":"en"},"timeZoneOffsetMinutes":0,"accessibility":{"reducedMotion":false,"increasedContrast":false}}"#);
    }

    #[test]
    fn typed_events_reject_unknown_duplicate_and_wrong_fields() {
        let _guard = crate::wire_lock();
        let calls = Rc::new(Cell::new(0));
        let observed = calls.clone();
        let id = events::canvas_frame(move |event| {
            assert_eq!(event.canvas_id, "main");
            assert_eq!(event.tick, 7);
            assert_eq!(event.time_ms, 3.0);
            observed.set(observed.get() + 1);
        });
        let name = EVENTS[0].as_bytes();
        for payload in [
            br#"{"canvasId":"main","tick":"7","timeMs":3}"#.as_slice(),
            br#"{"canvasId":"main","tick":7,"timeMs":3}"#.as_slice(),
            br#"{"canvasId":"main","tick":"7","timeMs":3,"debug":true}"#.as_slice(),
            br#"{"canvasId":"main","tick":"7","tick":"8","timeMs":3}"#.as_slice(),
        ] {
            // Safety: both slices are valid for the synchronous dispatch.
            unsafe {
                crate::events::dispatch(name.as_ptr(), name.len(), payload.as_ptr(), payload.len())
            };
        }
        events::off(id);
        assert_eq!(calls.get(), 1);
    }

    #[test]
    fn all_fifteen_event_payload_schemas_accept_the_closed_contract() {
        fn valid<T: WireDecode>(bytes: &[u8]) -> bool {
            decode_reply(bytes).ok().and_then(T::decode).is_some()
        }
        assert!(valid::<CanvasFrameEvent>(
            br#"{"canvasId":"main","tick":"1","timeMs":4294967296}"#
        ));
        assert!(!valid::<CanvasFrameEvent>(
            br#"{"canvasId":"main","tick":"1","timeMs":0.5}"#
        ));
        assert!(valid::<CanvasResizeEvent>(br#"{"canvasId":"main","width":1,"height":1,"dpr":1,"safeArea":{"top":0,"right":0,"bottom":0,"left":0}}"#));
        assert!(valid::<PlaybackReadyEvent>(
            br#"{"output":{"id":1,"kind":"audio-output"},"writableFrames":8}"#
        ));
        assert!(valid::<PlaybackEndedEvent>(
            br#"{"output":{"id":1,"kind":"audio-output"},"reason":"stopped"}"#
        ));
        assert!(valid::<PointerEvent>(br#"{"canvasId":"main","kind":"move","pointerId":1,"pointerType":"mouse","x":0,"y":0,"buttons":0,"coalesced":[],"droppedMoves":0,"modifiers":{"shift":false,"control":false,"alt":false,"meta":false}}"#));
        assert!(valid::<KeyEvent>(br#"{"canvasId":"main","kind":"down","key":"a","repeat":false,"modifiers":{"shift":false,"control":false,"alt":false,"meta":false}}"#));
        assert!(valid::<WheelEvent>(br#"{"canvasId":"main","x":0,"y":0,"deltaX":0,"deltaY":1,"modifiers":{"shift":false,"control":false,"alt":false,"meta":false}}"#));
        assert!(valid::<FocusChangeEvent>(
            br#"{"canvasId":"main","focused":true,"reason":"requested"}"#
        ));
        assert!(valid::<ImeTextUpdateEvent>(br#"{"handle":{"id":2,"kind":"ime-session"},"update":{"range":{"start":0,"end":0},"text":"x"},"baseRevision":"0"}"#));
        assert!(valid::<ImeCompositionEvent>(
            br#"{"handle":{"id":2,"kind":"ime-session"},"phase":"start","revision":"1"}"#
        ));
        assert!(valid::<ImeFormatUpdateEvent>(br#"{"handle":{"id":2,"kind":"ime-session"},"revision":"1","formats":[{"range":{"start":0,"end":1},"style":"input"}]}"#));
        assert!(valid::<ImeCharacterBoundsRequestEvent>(br#"{"handle":{"id":2,"kind":"ime-session"},"revision":"1","range":{"start":0,"end":1}}"#));
        assert!(valid::<ImeEndedEvent>(
            br#"{"handle":{"id":2,"kind":"ime-session"},"reason":"closed"}"#
        ));
        assert!(valid::<CaptureEndedEvent>(
            br#"{"handle":{"id":3,"kind":"camera-stream"},"reason":"stopped"}"#
        ));
        assert!(valid::<EnvironmentChangeEvent>(
            br#"{"changedFields":["locale","accessibility"]}"#
        ));

        assert!(!valid::<CaptureEndedEvent>(
            br#"{"handle":{"id":3,"kind":"font"},"reason":"stopped"}"#
        ));
        assert!(!valid::<EnvironmentChangeEvent>(
            br#"{"changedFields":["locale","locale"]}"#
        ));
        assert!(!valid::<ImeCompositionEvent>(br#"{"handle":{"id":2,"kind":"ime-session"},"phase":"start","revision":"1","debug":true}"#));
    }

    #[test]
    fn cancellation_before_first_poll_issues_no_host_call() {
        let _guard = crate::wire_lock();
        crate::test_shims::CALLS.lock().unwrap().clear();
        let mut call = clipboard::write_text("text");
        call.cancel();
        let mut cx = Context::from_waker(Waker::noop());
        let result = Pin::new(&mut call).poll(&mut cx);
        assert_eq!(
            result,
            Poll::Ready(Err(HostError::cancelled("clipboard.writeText")))
        );
        assert!(crate::test_shims::CALLS.lock().unwrap().is_empty());
    }

    #[test]
    fn cancellation_is_idempotent_and_wins_a_queued_success() {
        let _guard = crate::wire_lock();
        crate::test_shims::CALLS.lock().unwrap().clear();
        crate::test_shims::CANCELS.lock().unwrap().clear();
        let mut call = clipboard::write_text("text");
        let mut cx = Context::from_waker(Waker::noop());
        assert_eq!(Pin::new(&mut call).poll(&mut cx), Poll::Pending);
        let cb = crate::test_shims::CALLS.lock().unwrap()[0].3;
        // Safety: the static JSON bytes are valid for the duration of resolve.
        unsafe { crate::rt::resolve(cb, 1, b"null".as_ptr(), 4) };

        call.cancel();
        call.cancel();
        assert_eq!(
            Pin::new(&mut call).poll(&mut cx),
            Poll::Ready(Err(HostError::cancelled("clipboard.writeText")))
        );
        assert_eq!(*crate::test_shims::CANCELS.lock().unwrap(), vec![cb]);
    }

    #[test]
    fn cancelled_byte_sink_never_copies_a_queued_success() {
        let _guard = crate::wire_lock();
        crate::test_shims::CALLS.lock().unwrap().clear();
        crate::test_shims::CANCELS.lock().unwrap().clear();
        let mut bytes = [9u8; 4];
        let mut call = clipboard::read_text(ByteSink::new(&mut bytes).unwrap());
        let mut cx = Context::from_waker(Waker::noop());
        assert_eq!(Pin::new(&mut call).poll(&mut cx), Poll::Pending);
        let cb = crate::test_shims::CALLS.lock().unwrap()[0].3;
        let response = match encode_value(&obj(vec![
            ("written", num(2)),
            ("present", Value::Bool(true)),
            ("bytes", Value::Bytes(vec![1, 2])),
        ]))
        .unwrap()
        {
            Encoded::Plain(bytes) | Encoded::Envelope(bytes) => bytes,
        };
        // Safety: `response` remains alive for the synchronous resolve call.
        unsafe { crate::rt::resolve(cb, 1, response.as_ptr(), response.len()) };

        call.cancel();
        assert_eq!(
            Pin::new(&mut call).poll(&mut cx),
            Poll::Ready(Err(HostError::cancelled("clipboard.readText")))
        );
        drop(call);
        assert_eq!(bytes, [9, 9, 9, 9]);
        assert_eq!(*crate::test_shims::CANCELS.lock().unwrap(), vec![cb]);
    }

    #[test]
    fn dropping_a_pending_call_cancels_only_that_callback() {
        let _guard = crate::wire_lock();
        crate::test_shims::CALLS.lock().unwrap().clear();
        crate::test_shims::CANCELS.lock().unwrap().clear();
        let mut call = env::snapshot();
        let mut cx = Context::from_waker(Waker::noop());
        assert_eq!(Pin::new(&mut call).poll(&mut cx), Poll::Pending);
        let cb = crate::test_shims::CALLS.lock().unwrap()[0].3;

        drop(call);

        assert_eq!(*crate::test_shims::CANCELS.lock().unwrap(), vec![cb]);
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

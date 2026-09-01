//! Closed application.ist Host API v0.1 wire records.
//!
//! This module deliberately does not expose `Value` in public method signatures.
//! Every record rejects unknown and duplicate fields; every enum rejects unknown
//! strings; all integers and u64 decimal strings are lossless.

use crate::host_api_v01::{
    AudioOutput, CameraStream, Font, Handle, HandleKind, ImeSession, MicrophoneStream,
    ResourceMetadata,
};
use crate::store_net_encode::Value;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;
use unicode_normalization::UnicodeNormalization;

pub(crate) trait WireDecode: Sized {
    fn decode(value: Value) -> Option<Self>;
}

pub(crate) trait WireEncode {
    fn encode(self) -> Value;
}

pub(crate) trait SinkOutput {
    /// `None` means the method returned its explicit absent/null result.
    fn written(&self) -> Option<usize>;
}
impl<T: SinkOutput> SinkOutput for Option<T> {
    fn written(&self) -> Option<usize> {
        self.as_ref().and_then(SinkOutput::written)
    }
}

pub(crate) fn decode_wire<T: WireDecode>(value: Value) -> Option<T> {
    T::decode(value)
}

pub(crate) fn encode_wire<T: WireEncode>(value: T) -> Value {
    value.encode()
}

pub(crate) struct Record(Vec<(String, Value)>);

impl Record {
    pub(crate) fn closed(value: Value, required: &[&str], optional: &[&str]) -> Option<Self> {
        let Value::Obj(fields) = value else {
            return None;
        };
        if fields.len() < required.len() || fields.len() > required.len() + optional.len() {
            return None;
        }
        for (index, (key, _)) in fields.iter().enumerate() {
            if !required.contains(&key.as_str()) && !optional.contains(&key.as_str()) {
                return None;
            }
            if fields[..index].iter().any(|(prior, _)| prior == key) {
                return None;
            }
        }
        if required
            .iter()
            .any(|required| !fields.iter().any(|(key, _)| key == required))
        {
            return None;
        }
        Some(Self(fields))
    }

    pub(crate) fn take(&mut self, key: &str) -> Option<Value> {
        let index = self.0.iter().position(|(candidate, _)| candidate == key)?;
        Some(self.0.remove(index).1)
    }

    pub(crate) fn optional(&mut self, key: &str) -> Option<Value> {
        self.take(key)
    }
}

fn obj(entries: Vec<(&str, Value)>) -> Value {
    Value::Obj(
        entries
            .into_iter()
            .map(|(key, value)| (key.to_string(), value))
            .collect(),
    )
}
fn strv(value: impl Into<String>) -> Value {
    Value::Str(value.into())
}
fn num(value: u32) -> Value {
    Value::Num(value as f64)
}
fn u64v(value: u64) -> Value {
    strv(value.to_string())
}
fn string(value: Value) -> Option<String> {
    match value {
        Value::Str(value) => Some(value),
        _ => None,
    }
}
fn boolean(value: Value) -> Option<bool> {
    match value {
        Value::Bool(value) => Some(value),
        _ => None,
    }
}
fn finite(value: Value) -> Option<f64> {
    match value {
        Value::Num(value) if value.is_finite() => Some(value),
        _ => None,
    }
}
fn u32v(value: Value) -> Option<u32> {
    match value {
        Value::Num(value)
            if value >= 0.0 && value <= u32::MAX as f64 && value == (value as u32) as f64 =>
        {
            Some(value as u32)
        }
        _ => None,
    }
}
fn u64d(value: Value) -> Option<u64> {
    let Value::Str(value) = value else {
        return None;
    };
    if value.is_empty()
        || (value.len() > 1 && value.starts_with('0'))
        || !value.bytes().all(|byte| byte.is_ascii_digit())
    {
        return None;
    }
    value.parse().ok()
}
fn array<T: WireDecode>(value: Value) -> Option<Vec<T>> {
    let Value::Arr(values) = value else {
        return None;
    };
    values.into_iter().map(T::decode).collect()
}
fn optional<T>(value: Option<Value>, decode: fn(Value) -> Option<T>) -> Option<Option<T>> {
    match value {
        Some(value) => decode(value).map(Some),
        None => Some(None),
    }
}

impl WireDecode for String {
    fn decode(value: Value) -> Option<Self> {
        string(value)
    }
}
impl WireDecode for bool {
    fn decode(value: Value) -> Option<Self> {
        boolean(value)
    }
}
impl WireDecode for u32 {
    fn decode(value: Value) -> Option<Self> {
        u32v(value)
    }
}
impl WireDecode for u64 {
    fn decode(value: Value) -> Option<Self> {
        u64d(value)
    }
}
impl<K: HandleKind> WireDecode for Handle<K> {
    fn decode(value: Value) -> Option<Self> {
        Handle::from_value(&value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CanvasMode {
    Draw,
    Pixels,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PixelFormat {
    Rgba8,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextAlign {
    Start,
    Center,
    End,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextDirection {
    Ltr,
    Rtl,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Facing {
    User,
    Environment,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioFormat {
    F32Le,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopMode {
    Drain,
    Immediate,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    Text,
    Numeric,
    Email,
    Url,
    Search,
}

macro_rules! string_enum {
    ($ty:ty, $($variant:path => $wire:literal),+ $(,)?) => {
        impl WireDecode for $ty {
            fn decode(value: Value) -> Option<Self> {
                match string(value)?.as_str() { $($wire => Some($variant),)+ _ => None }
            }
        }
        impl WireEncode for $ty {
            fn encode(self) -> Value {
                strv(match self { $($variant => $wire,)+ })
            }
        }
    };
}
string_enum!(CanvasMode, CanvasMode::Draw => "draw", CanvasMode::Pixels => "pixels");
string_enum!(PixelFormat, PixelFormat::Rgba8 => "rgba8");
string_enum!(TextAlign, TextAlign::Start => "start", TextAlign::Center => "center", TextAlign::End => "end");
string_enum!(TextDirection, TextDirection::Ltr => "ltr", TextDirection::Rtl => "rtl");
string_enum!(Facing, Facing::User => "user", Facing::Environment => "environment");
string_enum!(AudioFormat, AudioFormat::F32Le => "f32le");
string_enum!(StopMode, StopMode::Drain => "drain", StopMode::Immediate => "immediate");
string_enum!(InputMode, InputMode::Text => "text", InputMode::Numeric => "numeric", InputMode::Email => "email", InputMode::Url => "url", InputMode::Search => "search");

#[derive(Debug, Clone, PartialEq)]
pub struct Insets {
    pub top: u32,
    pub right: u32,
    pub bottom: u32,
    pub left: u32,
}
impl WireDecode for Insets {
    fn decode(value: Value) -> Option<Self> {
        let mut r = Record::closed(value, &["top", "right", "bottom", "left"], &[])?;
        Some(Self {
            top: u32v(r.take("top")?)?,
            right: u32v(r.take("right")?)?,
            bottom: u32v(r.take("bottom")?)?,
            left: u32v(r.take("left")?)?,
        })
    }
}
impl WireEncode for Insets {
    fn encode(self) -> Value {
        obj(vec![
            ("top", num(self.top)),
            ("right", num(self.right)),
            ("bottom", num(self.bottom)),
            ("left", num(self.left)),
        ])
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}
impl WireDecode for Rect {
    fn decode(value: Value) -> Option<Self> {
        let mut r = Record::closed(value, &["x", "y", "width", "height"], &[])?;
        let out = Self {
            x: finite(r.take("x")?)?,
            y: finite(r.take("y")?)?,
            width: finite(r.take("width")?)?,
            height: finite(r.take("height")?)?,
        };
        (out.width >= 0.0 && out.height >= 0.0).then_some(out)
    }
}
impl WireEncode for Rect {
    fn encode(self) -> Value {
        obj(vec![
            ("x", Value::Num(self.x)),
            ("y", Value::Num(self.y)),
            ("width", Value::Num(self.width)),
            ("height", Value::Num(self.height)),
        ])
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}
impl WireDecode for Color {
    fn decode(value: Value) -> Option<Self> {
        let mut x = Record::closed(value, &["r", "g", "b", "a"], &[])?;
        Some(Self {
            r: u32v(x.take("r")?)?.try_into().ok()?,
            g: u32v(x.take("g")?)?.try_into().ok()?,
            b: u32v(x.take("b")?)?.try_into().ok()?,
            a: u32v(x.take("a")?)?.try_into().ok()?,
        })
    }
}
impl WireEncode for Color {
    fn encode(self) -> Value {
        obj(vec![
            ("r", num(self.r as u32)),
            ("g", num(self.g as u32)),
            ("b", num(self.b as u32)),
            ("a", num(self.a as u32)),
        ])
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Transform {
    pub a: f64,
    pub b: f64,
    pub c: f64,
    pub d: f64,
    pub e: f64,
    pub f: f64,
}
impl WireEncode for Transform {
    fn encode(self) -> Value {
        obj(vec![
            ("a", Value::Num(self.a)),
            ("b", Value::Num(self.b)),
            ("c", Value::Num(self.c)),
            ("d", Value::Num(self.d)),
            ("e", Value::Num(self.e)),
            ("f", Value::Num(self.f)),
        ])
    }
}

#[derive(Debug, PartialEq)]
pub struct TextStyle {
    pub font: Option<Handle<Font>>,
    pub size: f64,
    pub align: TextAlign,
    pub direction: TextDirection,
}
impl WireEncode for TextStyle {
    fn encode(self) -> Value {
        let mut v = vec![
            ("size", Value::Num(self.size)),
            ("align", self.align.encode()),
            ("direction", self.direction.encode()),
        ];
        if let Some(font) = self.font {
            v.push(("font", font.wire_value()))
        }
        obj(v)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum PathCommand {
    MoveTo {
        x: f64,
        y: f64,
    },
    LineTo {
        x: f64,
        y: f64,
    },
    QuadraticTo {
        cpx: f64,
        cpy: f64,
        x: f64,
        y: f64,
    },
    CubicTo {
        cp1x: f64,
        cp1y: f64,
        cp2x: f64,
        cp2y: f64,
        x: f64,
        y: f64,
    },
    Close,
}
impl WireEncode for PathCommand {
    fn encode(self) -> Value {
        match self {
            Self::MoveTo { x, y } => obj(vec![
                ("kind", strv("moveTo")),
                ("x", Value::Num(x)),
                ("y", Value::Num(y)),
            ]),
            Self::LineTo { x, y } => obj(vec![
                ("kind", strv("lineTo")),
                ("x", Value::Num(x)),
                ("y", Value::Num(y)),
            ]),
            Self::QuadraticTo { cpx, cpy, x, y } => obj(vec![
                ("kind", strv("quadraticTo")),
                ("cpx", Value::Num(cpx)),
                ("cpy", Value::Num(cpy)),
                ("x", Value::Num(x)),
                ("y", Value::Num(y)),
            ]),
            Self::CubicTo {
                cp1x,
                cp1y,
                cp2x,
                cp2y,
                x,
                y,
            } => obj(vec![
                ("kind", strv("cubicTo")),
                ("cp1x", Value::Num(cp1x)),
                ("cp1y", Value::Num(cp1y)),
                ("cp2x", Value::Num(cp2x)),
                ("cp2y", Value::Num(cp2y)),
                ("x", Value::Num(x)),
                ("y", Value::Num(y)),
            ]),
            Self::Close => obj(vec![("kind", strv("close"))]),
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum DrawOperation {
    Save,
    Restore,
    SetTransform(Transform),
    ClipRect(Rect),
    Clear(Color),
    FillRect {
        rect: Rect,
        color: Color,
    },
    StrokeRect {
        rect: Rect,
        color: Color,
        width: f64,
    },
    FillPath {
        commands: Vec<PathCommand>,
        color: Color,
        fill_rule: FillRule,
    },
    StrokePath {
        commands: Vec<PathCommand>,
        color: Color,
        width: f64,
        line_cap: LineCap,
        line_join: LineJoin,
    },
    FillText {
        x: f64,
        y: f64,
        text: String,
        style: TextStyle,
        color: Color,
    },
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FillRule {
    Nonzero,
    Evenodd,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineCap {
    Butt,
    Round,
    Square,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineJoin {
    Miter,
    Round,
    Bevel,
}
string_enum!(FillRule,FillRule::Nonzero=>"nonzero",FillRule::Evenodd=>"evenodd");
string_enum!(LineCap,LineCap::Butt=>"butt",LineCap::Round=>"round",LineCap::Square=>"square");
string_enum!(LineJoin,LineJoin::Miter=>"miter",LineJoin::Round=>"round",LineJoin::Bevel=>"bevel");
impl WireEncode for DrawOperation {
    fn encode(self) -> Value {
        match self {
            Self::Save => obj(vec![("op", strv("save"))]),
            Self::Restore => obj(vec![("op", strv("restore"))]),
            Self::SetTransform(v) => obj(vec![
                ("op", strv("setTransform")),
                ("transform", v.encode()),
            ]),
            Self::ClipRect(v) => obj(vec![("op", strv("clipRect")), ("rect", v.encode())]),
            Self::Clear(v) => obj(vec![("op", strv("clear")), ("color", v.encode())]),
            Self::FillRect { rect, color } => obj(vec![
                ("op", strv("fillRect")),
                ("rect", rect.encode()),
                ("color", color.encode()),
            ]),
            Self::StrokeRect { rect, color, width } => obj(vec![
                ("op", strv("strokeRect")),
                ("rect", rect.encode()),
                ("color", color.encode()),
                ("width", Value::Num(width)),
            ]),
            Self::FillPath {
                commands,
                color,
                fill_rule,
            } => obj(vec![
                ("op", strv("fillPath")),
                (
                    "commands",
                    Value::Arr(commands.into_iter().map(WireEncode::encode).collect()),
                ),
                ("color", color.encode()),
                ("fillRule", fill_rule.encode()),
            ]),
            Self::StrokePath {
                commands,
                color,
                width,
                line_cap,
                line_join,
            } => obj(vec![
                ("op", strv("strokePath")),
                (
                    "commands",
                    Value::Arr(commands.into_iter().map(WireEncode::encode).collect()),
                ),
                ("color", color.encode()),
                ("width", Value::Num(width)),
                ("lineCap", line_cap.encode()),
                ("lineJoin", line_join.encode()),
            ]),
            Self::FillText {
                x,
                y,
                text,
                style,
                color,
            } => obj(vec![
                ("op", strv("fillText")),
                ("x", Value::Num(x)),
                ("y", Value::Num(y)),
                ("text", strv(text)),
                ("style", style.encode()),
                ("color", color.encode()),
            ]),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct FrameInfo {
    pub width: u32,
    pub height: u32,
    pub stride: u32,
    pub format: PixelFormat,
    pub sequence: u64,
}
impl WireEncode for FrameInfo {
    fn encode(self) -> Value {
        obj(vec![
            ("width", num(self.width)),
            ("height", num(self.height)),
            ("stride", num(self.stride)),
            ("format", self.format.encode()),
            ("sequence", u64v(self.sequence)),
        ])
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Latency {
    Interactive,
    Balanced,
}
string_enum!(Latency,Latency::Interactive=>"interactive",Latency::Balanced=>"balanced");
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PlaybackOptions {
    pub preferred_sample_rate: Option<u32>,
    pub channels: Option<u8>,
    pub latency: Option<Latency>,
}
impl WireEncode for PlaybackOptions {
    fn encode(self) -> Value {
        let mut x = Vec::new();
        if let Some(v) = self.preferred_sample_rate {
            x.push(("preferredSampleRate", num(v)))
        }
        if let Some(v) = self.channels {
            x.push(("channels", num(v as u32)))
        }
        if let Some(v) = self.latency {
            x.push(("latency", v.encode()))
        }
        obj(x)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TextRange {
    pub start: u32,
    pub end: u32,
}
impl WireDecode for TextRange {
    fn decode(v: Value) -> Option<Self> {
        let mut r = Record::closed(v, &["start", "end"], &[])?;
        let x = Self {
            start: u32v(r.take("start")?)?,
            end: u32v(r.take("end")?)?,
        };
        (x.start <= x.end).then_some(x)
    }
}
impl WireEncode for TextRange {
    fn encode(self) -> Value {
        obj(vec![("start", num(self.start)), ("end", num(self.end))])
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct ImeState {
    pub text: String,
    pub selection: TextRange,
    pub composition: Option<TextRange>,
    pub input_mode: InputMode,
    pub multiline: bool,
    pub revision: u64,
}
impl WireEncode for ImeState {
    fn encode(self) -> Value {
        let mut x = vec![
            ("text", strv(self.text)),
            ("selection", self.selection.encode()),
            ("inputMode", self.input_mode.encode()),
            ("multiline", Value::Bool(self.multiline)),
            ("revision", u64v(self.revision)),
        ];
        if let Some(v) = self.composition {
            x.push(("composition", v.encode()))
        }
        obj(x)
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct CharacterBounds {
    pub index: u32,
    pub rect: Rect,
}
impl WireEncode for CharacterBounds {
    fn encode(self) -> Value {
        obj(vec![
            ("index", num(self.index)),
            ("rect", self.rect.encode()),
        ])
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct ImeGeometry {
    pub revision: u64,
    pub editor_bounds: Rect,
    pub caret_bounds: Rect,
    pub character_bounds: Option<Vec<CharacterBounds>>,
}
impl WireEncode for ImeGeometry {
    fn encode(self) -> Value {
        let mut x = vec![
            ("revision", u64v(self.revision)),
            ("editorBounds", self.editor_bounds.encode()),
            ("caretBounds", self.caret_bounds.encode()),
        ];
        if let Some(v) = self.character_bounds {
            x.push((
                "characterBounds",
                Value::Arr(v.into_iter().map(WireEncode::encode).collect()),
            ))
        }
        obj(x)
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct CameraConstraints {
    pub facing: Option<Facing>,
    pub max_width: Option<u32>,
    pub max_height: Option<u32>,
    pub max_frame_rate: Option<u32>,
}
impl WireEncode for CameraConstraints {
    fn encode(self) -> Value {
        let mut x = Vec::new();
        if let Some(v) = self.facing {
            x.push(("facing", v.encode()))
        }
        if let Some(v) = self.max_width {
            x.push(("maxWidth", num(v)))
        }
        if let Some(v) = self.max_height {
            x.push(("maxHeight", num(v)))
        }
        if let Some(v) = self.max_frame_rate {
            x.push(("maxFrameRate", num(v)))
        }
        obj(x)
    }
}
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MicrophoneConstraints {
    pub preferred_sample_rate: Option<u32>,
    pub max_channels: Option<u8>,
}
impl WireEncode for MicrophoneConstraints {
    fn encode(self) -> Value {
        let mut x = Vec::new();
        if let Some(v) = self.preferred_sample_rate {
            x.push(("preferredSampleRate", num(v)))
        }
        if let Some(v) = self.max_channels {
            x.push(("maxChannels", num(v as u32)))
        }
        obj(x)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum StoreCondition {
    Any,
    Absent,
    Version(u64),
}
impl WireEncode for StoreCondition {
    fn encode(self) -> Value {
        match self {
            Self::Any => strv("any"),
            Self::Absent => strv("absent"),
            Self::Version(v) => obj(vec![("version", u64v(v))]),
        }
    }
}
#[derive(Debug, Clone, PartialEq)]
pub enum StoreDeleteCondition {
    Any,
    Version(u64),
}
impl WireEncode for StoreDeleteCondition {
    fn encode(self) -> Value {
        match self {
            Self::Any => strv("any"),
            Self::Version(v) => obj(vec![("version", u64v(v))]),
        }
    }
}

// ---- Method success records -------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct CanvasInfo {
    pub width: u32,
    pub height: u32,
    pub dpr: f64,
    pub mode: CanvasMode,
    pub safe_area: Insets,
}
impl WireDecode for CanvasInfo {
    fn decode(v: Value) -> Option<Self> {
        let mut r = Record::closed(v, &["width", "height", "dpr", "mode", "safeArea"], &[])?;
        let out = Self {
            width: u32v(r.take("width")?)?,
            height: u32v(r.take("height")?)?,
            dpr: finite(r.take("dpr")?)?,
            mode: CanvasMode::decode(r.take("mode")?)?,
            safe_area: Insets::decode(r.take("safeArea")?)?,
        };
        let horizontal = out.safe_area.left.checked_add(out.safe_area.right)?;
        let vertical = out.safe_area.top.checked_add(out.safe_area.bottom)?;
        (horizontal <= out.width
            && vertical <= out.height
            && (0.5..=8.0).contains(&out.dpr)
            && out.dpr * 8.0 == ((out.dpr * 8.0) as u32) as f64)
            .then_some(out)
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct TextMetrics {
    pub advance: f64,
}
impl WireDecode for TextMetrics {
    fn decode(v: Value) -> Option<Self> {
        let mut r = Record::closed(v, &["advance"], &[])?;
        let advance = finite(r.take("advance")?)?;
        (advance >= 0.0).then_some(Self { advance })
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct AssetReadResult {
    pub written: u32,
    pub total_size: u64,
    pub eof: bool,
}
impl WireDecode for AssetReadResult {
    fn decode(v: Value) -> Option<Self> {
        let mut r = Record::closed(v, &["written", "totalSize", "eof"], &[])?;
        Some(Self {
            written: u32v(r.take("written")?)?,
            total_size: u64d(r.take("totalSize")?)?,
            eof: boolean(r.take("eof")?)?,
        })
    }
}
impl SinkOutput for AssetReadResult {
    fn written(&self) -> Option<usize> {
        Some(self.written as usize)
    }
}
#[derive(Debug, PartialEq)]
pub struct PlaybackStream {
    pub output: Handle<AudioOutput>,
    pub sample_rate: u32,
    pub channels: u8,
    pub max_queued_frames: u32,
}
impl WireDecode for PlaybackStream {
    fn decode(v: Value) -> Option<Self> {
        let mut r = Record::closed(
            v,
            &["output", "sampleRate", "channels", "maxQueuedFrames"],
            &[],
        )?;
        let c: u8 = u32v(r.take("channels")?)?.try_into().ok()?;
        if c != 1 && c != 2 {
            return None;
        }
        let output = Handle::from_value(&r.take("output")?)?;
        let sample_rate = u32v(r.take("sampleRate")?)?;
        let max_queued_frames = u32v(r.take("maxQueuedFrames")?)?;
        if !(8_000..=192_000).contains(&sample_rate) || max_queued_frames == 0 {
            return None;
        }
        output.set_metadata(ResourceMetadata::AudioOutput {
            channels: c,
            max_queued_frames,
        });
        Some(Self {
            output,
            sample_rate,
            channels: c,
            max_queued_frames,
        })
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct PlaybackWriteResult {
    pub accepted_frames: u32,
    pub queued_frames: u32,
}
impl WireDecode for PlaybackWriteResult {
    fn decode(v: Value) -> Option<Self> {
        let mut r = Record::closed(v, &["acceptedFrames", "queuedFrames"], &[])?;
        Some(Self {
            accepted_frames: u32v(r.take("acceptedFrames")?)?,
            queued_frames: u32v(r.take("queuedFrames")?)?,
        })
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct FocusResult {
    pub focused: bool,
}
impl WireDecode for FocusResult {
    fn decode(v: Value) -> Option<Self> {
        let mut r = Record::closed(v, &["focused"], &[])?;
        boolean(r.take("focused")?)?.then_some(Self { focused: true })
    }
}
#[derive(Debug, PartialEq)]
pub struct CameraSession {
    pub camera: Handle<CameraStream>,
    pub facing: Facing,
    pub width: u32,
    pub height: u32,
    pub stride: u32,
    pub format: PixelFormat,
    pub frame_bytes: u32,
    pub nominal_frame_rate: u32,
}
impl WireDecode for CameraSession {
    fn decode(v: Value) -> Option<Self> {
        let mut r = Record::closed(
            v,
            &[
                "camera",
                "facing",
                "width",
                "height",
                "stride",
                "format",
                "frameBytes",
                "nominalFrameRate",
            ],
            &[],
        )?;
        let camera = Handle::from_value(&r.take("camera")?)?;
        let out = Self {
            camera,
            facing: Facing::decode(r.take("facing")?)?,
            width: u32v(r.take("width")?)?,
            height: u32v(r.take("height")?)?,
            stride: u32v(r.take("stride")?)?,
            format: PixelFormat::decode(r.take("format")?)?,
            frame_bytes: u32v(r.take("frameBytes")?)?,
            nominal_frame_rate: u32v(r.take("nominalFrameRate")?)?,
        };
        let row_bytes = out.width.checked_mul(4)?;
        let expected_frame_bytes = out.stride.checked_mul(out.height)?;
        if out.width == 0
            || out.height == 0
            || out.stride < row_bytes
            || out.frame_bytes == 0
            || out.frame_bytes != expected_frame_bytes
            || out.nominal_frame_rate == 0
        {
            return None;
        }
        out.camera.set_metadata(ResourceMetadata::Camera {
            width: out.width,
            height: out.height,
            stride: out.stride,
            frame_bytes: out.frame_bytes,
            sequences: Default::default(),
        });
        Some(out)
    }
}
#[derive(Debug, PartialEq)]
pub struct MicrophoneSession {
    pub microphone: Handle<MicrophoneStream>,
    pub sample_rate: u32,
    pub channels: u8,
    pub format: AudioFormat,
    pub max_frames_per_read: u32,
}
impl WireDecode for MicrophoneSession {
    fn decode(v: Value) -> Option<Self> {
        let mut r = Record::closed(
            v,
            &[
                "microphone",
                "sampleRate",
                "channels",
                "format",
                "maxFramesPerRead",
            ],
            &[],
        )?;
        let c: u8 = u32v(r.take("channels")?)?.try_into().ok()?;
        if c != 1 && c != 2 {
            return None;
        }
        let microphone = Handle::from_value(&r.take("microphone")?)?;
        let sample_rate = u32v(r.take("sampleRate")?)?;
        let max_frames_per_read = u32v(r.take("maxFramesPerRead")?)?;
        if !(8_000..=192_000).contains(&sample_rate) || max_frames_per_read == 0 {
            return None;
        }
        microphone.set_metadata(ResourceMetadata::Microphone {
            sample_rate,
            channels: c,
            max_frames_per_read,
            last: None,
        });
        Some(Self {
            microphone,
            sample_rate,
            channels: c,
            format: AudioFormat::decode(r.take("format")?)?,
            max_frames_per_read,
        })
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct VideoFrameResult {
    pub written: u32,
    pub width: u32,
    pub height: u32,
    pub stride: u32,
    pub format: PixelFormat,
    pub sequence: u64,
}
impl WireDecode for VideoFrameResult {
    fn decode(v: Value) -> Option<Self> {
        let mut r = Record::closed(
            v,
            &["written", "width", "height", "stride", "format", "sequence"],
            &[],
        )?;
        let out = Self {
            written: u32v(r.take("written")?)?,
            width: u32v(r.take("width")?)?,
            height: u32v(r.take("height")?)?,
            stride: u32v(r.take("stride")?)?,
            format: PixelFormat::decode(r.take("format")?)?,
            sequence: u64d(r.take("sequence")?)?,
        };
        let row_bytes = out.width.checked_mul(4)?;
        let expected = out.stride.checked_mul(out.height)?;
        (out.width > 0
            && out.height > 0
            && out.stride >= row_bytes
            && out.written > 0
            && out.written == expected)
            .then_some(out)
    }
}
impl SinkOutput for VideoFrameResult {
    fn written(&self) -> Option<usize> {
        Some(self.written as usize)
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct AudioReadResult {
    pub written: u32,
    pub sample_rate: u32,
    pub channels: u8,
    pub format: AudioFormat,
    pub frames: u32,
    pub sequence: u64,
    pub dropped_frames: u32,
}
impl WireDecode for AudioReadResult {
    fn decode(v: Value) -> Option<Self> {
        let mut r = Record::closed(
            v,
            &[
                "written",
                "sampleRate",
                "channels",
                "format",
                "frames",
                "sequence",
                "droppedFrames",
            ],
            &[],
        )?;
        let c: u8 = u32v(r.take("channels")?)?.try_into().ok()?;
        if c != 1 && c != 2 {
            return None;
        }
        let out = Self {
            written: u32v(r.take("written")?)?,
            sample_rate: u32v(r.take("sampleRate")?)?,
            channels: c,
            format: AudioFormat::decode(r.take("format")?)?,
            frames: u32v(r.take("frames")?)?,
            sequence: u64d(r.take("sequence")?)?,
            dropped_frames: u32v(r.take("droppedFrames")?)?,
        };
        let expected = out
            .frames
            .checked_mul(out.channels as u32)?
            .checked_mul(4)?;
        (out.frames > 0
            && out.written > 0
            && out.written == expected
            && (8_000..=192_000).contains(&out.sample_rate))
        .then_some(out)
    }
}
impl SinkOutput for AudioReadResult {
    fn written(&self) -> Option<usize> {
        Some(self.written as usize)
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct StoreGetResult {
    pub version: u64,
    pub written: u32,
    pub required: u32,
}
impl WireDecode for StoreGetResult {
    fn decode(v: Value) -> Option<Self> {
        let mut r = Record::closed(v, &["version", "written", "required"], &[])?;
        Some(Self {
            version: u64d(r.take("version")?)?,
            written: u32v(r.take("written")?)?,
            required: u32v(r.take("required")?)?,
        })
    }
}
impl SinkOutput for StoreGetResult {
    fn written(&self) -> Option<usize> {
        Some(self.written as usize)
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct VersionResult {
    pub version: u64,
}
impl WireDecode for VersionResult {
    fn decode(v: Value) -> Option<Self> {
        let mut r = Record::closed(v, &["version"], &[])?;
        Some(Self {
            version: u64d(r.take("version")?)?,
        })
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct DeletedResult {
    pub deleted: bool,
}
impl WireDecode for DeletedResult {
    fn decode(v: Value) -> Option<Self> {
        let mut r = Record::closed(v, &["deleted"], &[])?;
        Some(Self {
            deleted: boolean(r.take("deleted")?)?,
        })
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct DeleteCount {
    pub deleted: u64,
}
impl WireDecode for DeleteCount {
    fn decode(v: Value) -> Option<Self> {
        let mut r = Record::closed(v, &["deleted"], &[])?;
        Some(Self {
            deleted: u64d(r.take("deleted")?)?,
        })
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct StoreListItem {
    pub key: String,
    pub version: u64,
    pub size: u64,
}
impl WireDecode for StoreListItem {
    fn decode(v: Value) -> Option<Self> {
        let mut r = Record::closed(v, &["key", "version", "size"], &[])?;
        let key = string(r.take("key")?)?;
        if key.is_empty()
            || key
                .chars()
                .any(|character| character <= '\u{1f}' || character == '\u{7f}')
            || !key.nfc().eq(key.chars())
        {
            return None;
        }
        Some(Self {
            key,
            version: u64d(r.take("version")?)?,
            size: u64d(r.take("size")?)?,
        })
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct StoreListPage {
    pub items: Vec<StoreListItem>,
    pub cursor: Option<String>,
}
impl WireDecode for StoreListPage {
    fn decode(v: Value) -> Option<Self> {
        let mut r = Record::closed(v, &["items"], &["cursor"])?;
        let items: Vec<StoreListItem> = array(r.take("items")?)?;
        if items.windows(2).any(|pair| pair[0].key >= pair[1].key) {
            return None;
        }
        let cursor = optional(r.optional("cursor"), string)?;
        if cursor.as_ref().is_some_and(|value| {
            value.is_empty()
                || value.len() > 256
                || !value
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
        }) {
            return None;
        }
        Some(Self { items, cursor })
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct ClipboardTextResult {
    pub present: bool,
    pub written: u32,
}
impl WireDecode for ClipboardTextResult {
    fn decode(v: Value) -> Option<Self> {
        let mut r = Record::closed(v, &["present", "written"], &[])?;
        if !boolean(r.take("present")?)? {
            return None;
        }
        Some(Self {
            present: true,
            written: u32v(r.take("written")?)?,
        })
    }
}
impl SinkOutput for ClipboardTextResult {
    fn written(&self) -> Option<usize> {
        Some(self.written as usize)
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct Locale {
    pub language: String,
    pub script: Option<String>,
}
impl WireDecode for Locale {
    fn decode(v: Value) -> Option<Self> {
        let mut r = Record::closed(v, &["language"], &["script"])?;
        let language = string(r.take("language")?)?;
        let script = optional(r.optional("script"), string)?;
        let valid_language = (2..=8).contains(&language.len())
            && language.bytes().all(|byte| byte.is_ascii_lowercase());
        let valid_script = script.as_ref().is_none_or(|value| {
            value.len() == 4
                && value.as_bytes()[0].is_ascii_uppercase()
                && value.as_bytes()[1..].iter().all(u8::is_ascii_lowercase)
        });
        (valid_language && valid_script).then_some(Self { language, script })
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct Accessibility {
    pub reduced_motion: bool,
    pub increased_contrast: bool,
}
impl WireDecode for Accessibility {
    fn decode(v: Value) -> Option<Self> {
        let mut r = Record::closed(v, &["reducedMotion", "increasedContrast"], &[])?;
        Some(Self {
            reduced_motion: boolean(r.take("reducedMotion")?)?,
            increased_contrast: boolean(r.take("increasedContrast")?)?,
        })
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorScheme {
    Light,
    Dark,
}
string_enum!(ColorScheme,ColorScheme::Light=>"light",ColorScheme::Dark=>"dark");
#[derive(Debug, Clone, PartialEq)]
pub struct EnvironmentSnapshot {
    pub color_scheme: ColorScheme,
    pub locale: Locale,
    pub time_zone_offset_minutes: i16,
    pub accessibility: Accessibility,
}
impl WireDecode for EnvironmentSnapshot {
    fn decode(v: Value) -> Option<Self> {
        let mut r = Record::closed(
            v,
            &[
                "colorScheme",
                "locale",
                "timeZoneOffsetMinutes",
                "accessibility",
            ],
            &[],
        )?;
        let offset = finite(r.take("timeZoneOffsetMinutes")?)?;
        if offset != (offset as i16) as f64
            || !(-840.0..=840.0).contains(&offset)
            || (offset as i16) % 15 != 0
        {
            return None;
        }
        Some(Self {
            color_scheme: ColorScheme::decode(r.take("colorScheme")?)?,
            locale: Locale::decode(r.take("locale")?)?,
            time_zone_offset_minutes: offset as i16,
            accessibility: Accessibility::decode(r.take("accessibility")?)?,
        })
    }
}
pub(crate) fn decode_void(value: Value) -> Option<()> {
    matches!(value, Value::Null).then_some(())
}
pub(crate) fn decode_optional<T: WireDecode>(value: Value) -> Option<Option<T>> {
    if matches!(value, Value::Null) {
        Some(None)
    } else {
        T::decode(value).map(Some)
    }
}
pub(crate) fn decode_vec<T: WireDecode>(value: Value) -> Option<Vec<T>> {
    array(value)
}
// ---- Typed event payloads ---------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct CanvasFrameEvent {
    pub canvas_id: String,
    pub tick: u64,
    pub time_ms: f64,
}
impl WireDecode for CanvasFrameEvent {
    fn decode(v: Value) -> Option<Self> {
        let mut r = Record::closed(v, &["canvasId", "tick", "timeMs"], &[])?;
        let time_ms = finite(r.take("timeMs")?)?;
        if time_ms < 0.0 || time_ms % 1.0 != 0.0 {
            return None;
        }
        Some(Self {
            canvas_id: string(r.take("canvasId")?)?,
            tick: u64d(r.take("tick")?)?,
            time_ms,
        })
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct CanvasResizeEvent {
    pub canvas_id: String,
    pub width: u32,
    pub height: u32,
    pub dpr: f64,
    pub safe_area: Insets,
}
impl WireDecode for CanvasResizeEvent {
    fn decode(v: Value) -> Option<Self> {
        let mut r = Record::closed(v, &["canvasId", "width", "height", "dpr", "safeArea"], &[])?;
        Some(Self {
            canvas_id: string(r.take("canvasId")?)?,
            width: u32v(r.take("width")?)?,
            height: u32v(r.take("height")?)?,
            dpr: finite(r.take("dpr")?)?,
            safe_area: Insets::decode(r.take("safeArea")?)?,
        })
    }
}
#[derive(Debug, PartialEq)]
pub struct PlaybackReadyEvent {
    pub output: Handle<AudioOutput>,
    pub writable_frames: u32,
}
impl WireDecode for PlaybackReadyEvent {
    fn decode(v: Value) -> Option<Self> {
        let mut r = Record::closed(v, &["output", "writableFrames"], &[])?;
        Some(Self {
            output: Handle::from_value(&r.take("output")?)?,
            writable_frames: u32v(r.take("writableFrames")?)?,
        })
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaybackEndedReason {
    Stopped,
    DeviceLost,
    PermissionRevoked,
    BackgroundPolicy,
    Error,
}
string_enum!(PlaybackEndedReason,PlaybackEndedReason::Stopped=>"stopped",PlaybackEndedReason::DeviceLost=>"device-lost",PlaybackEndedReason::PermissionRevoked=>"permission-revoked",PlaybackEndedReason::BackgroundPolicy=>"background-policy",PlaybackEndedReason::Error=>"error");
#[derive(Debug, PartialEq)]
pub struct PlaybackEndedEvent {
    pub output: Handle<AudioOutput>,
    pub reason: PlaybackEndedReason,
}
impl WireDecode for PlaybackEndedEvent {
    fn decode(v: Value) -> Option<Self> {
        let mut r = Record::closed(v, &["output", "reason"], &[])?;
        Some(Self {
            output: Handle::from_value(&r.take("output")?)?,
            reason: PlaybackEndedReason::decode(r.take("reason")?)?,
        })
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct Modifiers {
    pub shift: bool,
    pub control: bool,
    pub alt: bool,
    pub meta: bool,
}
impl WireDecode for Modifiers {
    fn decode(v: Value) -> Option<Self> {
        let mut r = Record::closed(v, &["shift", "control", "alt", "meta"], &[])?;
        Some(Self {
            shift: boolean(r.take("shift")?)?,
            control: boolean(r.take("control")?)?,
            alt: boolean(r.take("alt")?)?,
            meta: boolean(r.take("meta")?)?,
        })
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct PointerSample {
    pub x: f64,
    pub y: f64,
    pub pressure: Option<f64>,
}
impl WireDecode for PointerSample {
    fn decode(v: Value) -> Option<Self> {
        let mut r = Record::closed(v, &["x", "y"], &["pressure"])?;
        let p = optional(r.optional("pressure"), finite)?;
        if p.is_some_and(|x| !(0.0..=1.0).contains(&x)) {
            return None;
        }
        Some(Self {
            x: finite(r.take("x")?)?,
            y: finite(r.take("y")?)?,
            pressure: p,
        })
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointerKind {
    Enter,
    Move,
    Down,
    Up,
    Cancel,
    Leave,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointerType {
    Mouse,
    Touch,
    Pen,
    Unknown,
}
string_enum!(PointerKind,PointerKind::Enter=>"enter",PointerKind::Move=>"move",PointerKind::Down=>"down",PointerKind::Up=>"up",PointerKind::Cancel=>"cancel",PointerKind::Leave=>"leave");
string_enum!(PointerType,PointerType::Mouse=>"mouse",PointerType::Touch=>"touch",PointerType::Pen=>"pen",PointerType::Unknown=>"unknown");
#[derive(Debug, Clone, PartialEq)]
pub struct PointerEvent {
    pub canvas_id: String,
    pub kind: PointerKind,
    pub pointer_id: u32,
    pub pointer_type: PointerType,
    pub x: f64,
    pub y: f64,
    pub buttons: u32,
    pub pressure: Option<f64>,
    pub coalesced: Vec<PointerSample>,
    pub dropped_moves: u32,
    pub modifiers: Modifiers,
}
impl WireDecode for PointerEvent {
    fn decode(v: Value) -> Option<Self> {
        let mut r = Record::closed(
            v,
            &[
                "canvasId",
                "kind",
                "pointerId",
                "pointerType",
                "x",
                "y",
                "buttons",
                "coalesced",
                "droppedMoves",
                "modifiers",
            ],
            &["pressure"],
        )?;
        let pressure = optional(r.optional("pressure"), finite)?;
        if pressure.is_some_and(|x| !(0.0..=1.0).contains(&x)) {
            return None;
        }
        Some(Self {
            canvas_id: string(r.take("canvasId")?)?,
            kind: PointerKind::decode(r.take("kind")?)?,
            pointer_id: u32v(r.take("pointerId")?)?,
            pointer_type: PointerType::decode(r.take("pointerType")?)?,
            x: finite(r.take("x")?)?,
            y: finite(r.take("y")?)?,
            buttons: u32v(r.take("buttons")?)?,
            pressure,
            coalesced: array(r.take("coalesced")?)?,
            dropped_moves: u32v(r.take("droppedMoves")?)?,
            modifiers: Modifiers::decode(r.take("modifiers")?)?,
        })
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyKind {
    Down,
    Up,
}
string_enum!(KeyKind,KeyKind::Down=>"down",KeyKind::Up=>"up");
#[derive(Debug, Clone, PartialEq)]
pub struct KeyEvent {
    pub canvas_id: String,
    pub kind: KeyKind,
    pub key: String,
    pub repeat: bool,
    pub modifiers: Modifiers,
}
impl WireDecode for KeyEvent {
    fn decode(v: Value) -> Option<Self> {
        let mut r = Record::closed(v, &["canvasId", "kind", "key", "repeat", "modifiers"], &[])?;
        Some(Self {
            canvas_id: string(r.take("canvasId")?)?,
            kind: KeyKind::decode(r.take("kind")?)?,
            key: string(r.take("key")?)?,
            repeat: boolean(r.take("repeat")?)?,
            modifiers: Modifiers::decode(r.take("modifiers")?)?,
        })
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct WheelEvent {
    pub canvas_id: String,
    pub x: f64,
    pub y: f64,
    pub delta_x: f64,
    pub delta_y: f64,
    pub modifiers: Modifiers,
}
impl WireDecode for WheelEvent {
    fn decode(v: Value) -> Option<Self> {
        let mut r = Record::closed(
            v,
            &["canvasId", "x", "y", "deltaX", "deltaY", "modifiers"],
            &[],
        )?;
        Some(Self {
            canvas_id: string(r.take("canvasId")?)?,
            x: finite(r.take("x")?)?,
            y: finite(r.take("y")?)?,
            delta_x: finite(r.take("deltaX")?)?,
            delta_y: finite(r.take("deltaY")?)?,
            modifiers: Modifiers::decode(r.take("modifiers")?)?,
        })
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusReason {
    Requested,
    User,
    System,
    Hidden,
    Destroyed,
    TrustedUi,
}
string_enum!(FocusReason,FocusReason::Requested=>"requested",FocusReason::User=>"user",FocusReason::System=>"system",FocusReason::Hidden=>"hidden",FocusReason::Destroyed=>"destroyed",FocusReason::TrustedUi=>"trusted-ui");
#[derive(Debug, Clone, PartialEq)]
pub struct FocusChangeEvent {
    pub canvas_id: String,
    pub focused: bool,
    pub reason: FocusReason,
}
impl WireDecode for FocusChangeEvent {
    fn decode(v: Value) -> Option<Self> {
        let mut r = Record::closed(v, &["canvasId", "focused", "reason"], &[])?;
        Some(Self {
            canvas_id: string(r.take("canvasId")?)?,
            focused: boolean(r.take("focused")?)?,
            reason: FocusReason::decode(r.take("reason")?)?,
        })
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct TextUpdate {
    pub range: TextRange,
    pub text: String,
}
impl WireDecode for TextUpdate {
    fn decode(v: Value) -> Option<Self> {
        let mut r = Record::closed(v, &["range", "text"], &[])?;
        Some(Self {
            range: TextRange::decode(r.take("range")?)?,
            text: string(r.take("text")?)?,
        })
    }
}
#[derive(Debug, PartialEq)]
pub struct ImeTextUpdateEvent {
    pub handle: Handle<ImeSession>,
    pub update: TextUpdate,
    pub base_revision: u64,
}
impl WireDecode for ImeTextUpdateEvent {
    fn decode(v: Value) -> Option<Self> {
        let mut r = Record::closed(v, &["handle", "update", "baseRevision"], &[])?;
        Some(Self {
            handle: Handle::from_value(&r.take("handle")?)?,
            update: TextUpdate::decode(r.take("update")?)?,
            base_revision: u64d(r.take("baseRevision")?)?,
        })
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompositionPhase {
    Start,
    Update,
    Commit,
    Cancel,
}
string_enum!(CompositionPhase,CompositionPhase::Start=>"start",CompositionPhase::Update=>"update",CompositionPhase::Commit=>"commit",CompositionPhase::Cancel=>"cancel");
#[derive(Debug, PartialEq)]
pub struct ImeCompositionEvent {
    pub handle: Handle<ImeSession>,
    pub phase: CompositionPhase,
    pub range: Option<TextRange>,
    pub revision: u64,
}
impl WireDecode for ImeCompositionEvent {
    fn decode(v: Value) -> Option<Self> {
        let mut r = Record::closed(v, &["handle", "phase", "revision"], &["range"])?;
        Some(Self {
            handle: Handle::from_value(&r.take("handle")?)?,
            phase: CompositionPhase::decode(r.take("phase")?)?,
            range: optional(r.optional("range"), TextRange::decode)?,
            revision: u64d(r.take("revision")?)?,
        })
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormatStyle {
    Input,
    Converted,
    Target,
    Selected,
}
string_enum!(FormatStyle,FormatStyle::Input=>"input",FormatStyle::Converted=>"converted",FormatStyle::Target=>"target",FormatStyle::Selected=>"selected");
#[derive(Debug, Clone, PartialEq)]
pub struct FormatRange {
    pub range: TextRange,
    pub style: FormatStyle,
}
impl WireDecode for FormatRange {
    fn decode(v: Value) -> Option<Self> {
        let mut r = Record::closed(v, &["range", "style"], &[])?;
        Some(Self {
            range: TextRange::decode(r.take("range")?)?,
            style: FormatStyle::decode(r.take("style")?)?,
        })
    }
}
#[derive(Debug, PartialEq)]
pub struct ImeFormatUpdateEvent {
    pub handle: Handle<ImeSession>,
    pub revision: u64,
    pub formats: Vec<FormatRange>,
}
impl WireDecode for ImeFormatUpdateEvent {
    fn decode(v: Value) -> Option<Self> {
        let mut r = Record::closed(v, &["handle", "revision", "formats"], &[])?;
        Some(Self {
            handle: Handle::from_value(&r.take("handle")?)?,
            revision: u64d(r.take("revision")?)?,
            formats: array(r.take("formats")?)?,
        })
    }
}
#[derive(Debug, PartialEq)]
pub struct ImeCharacterBoundsRequestEvent {
    pub handle: Handle<ImeSession>,
    pub revision: u64,
    pub range: TextRange,
}
impl WireDecode for ImeCharacterBoundsRequestEvent {
    fn decode(v: Value) -> Option<Self> {
        let mut r = Record::closed(v, &["handle", "revision", "range"], &[])?;
        Some(Self {
            handle: Handle::from_value(&r.take("handle")?)?,
            revision: u64d(r.take("revision")?)?,
            range: TextRange::decode(r.take("range")?)?,
        })
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImeEndedReason {
    Closed,
    LostFocus,
    CanvasDestroyed,
    Error,
}
string_enum!(ImeEndedReason,ImeEndedReason::Closed=>"closed",ImeEndedReason::LostFocus=>"lost-focus",ImeEndedReason::CanvasDestroyed=>"canvas-destroyed",ImeEndedReason::Error=>"error");
#[derive(Debug, PartialEq)]
pub struct ImeEndedEvent {
    pub handle: Handle<ImeSession>,
    pub reason: ImeEndedReason,
}
impl WireDecode for ImeEndedEvent {
    fn decode(v: Value) -> Option<Self> {
        let mut r = Record::closed(v, &["handle", "reason"], &[])?;
        Some(Self {
            handle: Handle::from_value(&r.take("handle")?)?,
            reason: ImeEndedReason::decode(r.take("reason")?)?,
        })
    }
}
#[derive(Debug, PartialEq)]
pub enum CaptureEventHandle {
    Camera(Handle<CameraStream>),
    Microphone(Handle<MicrophoneStream>),
}
impl WireDecode for CaptureEventHandle {
    fn decode(v: Value) -> Option<Self> {
        Handle::<CameraStream>::from_value(&v)
            .map(Self::Camera)
            .or_else(|| Handle::<MicrophoneStream>::from_value(&v).map(Self::Microphone))
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureEndedReason {
    Stopped,
    PermissionRevoked,
    DeviceLost,
    BackgroundPolicy,
    Error,
}
string_enum!(CaptureEndedReason,CaptureEndedReason::Stopped=>"stopped",CaptureEndedReason::PermissionRevoked=>"permission-revoked",CaptureEndedReason::DeviceLost=>"device-lost",CaptureEndedReason::BackgroundPolicy=>"background-policy",CaptureEndedReason::Error=>"error");
#[derive(Debug, PartialEq)]
pub struct CaptureEndedEvent {
    pub handle: CaptureEventHandle,
    pub reason: CaptureEndedReason,
}
impl WireDecode for CaptureEndedEvent {
    fn decode(v: Value) -> Option<Self> {
        let mut r = Record::closed(v, &["handle", "reason"], &[])?;
        Some(Self {
            handle: CaptureEventHandle::decode(r.take("handle")?)?,
            reason: CaptureEndedReason::decode(r.take("reason")?)?,
        })
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnvironmentField {
    ColorScheme,
    Locale,
    TimeZoneOffsetMinutes,
    Accessibility,
}
string_enum!(EnvironmentField,EnvironmentField::ColorScheme=>"colorScheme",EnvironmentField::Locale=>"locale",EnvironmentField::TimeZoneOffsetMinutes=>"timeZoneOffsetMinutes",EnvironmentField::Accessibility=>"accessibility");
#[derive(Debug, Clone, PartialEq)]
pub struct EnvironmentChangeEvent {
    pub changed_fields: Vec<EnvironmentField>,
}
impl WireDecode for EnvironmentChangeEvent {
    fn decode(v: Value) -> Option<Self> {
        let mut r = Record::closed(v, &["changedFields"], &[])?;
        let values: Vec<EnvironmentField> = array(r.take("changedFields")?)?;
        for i in 0..values.len() {
            if values[..i].contains(&values[i]) {
                return None;
            }
        }
        Some(Self {
            changed_fields: values,
        })
    }
}

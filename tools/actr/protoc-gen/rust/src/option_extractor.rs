//! Method-option extraction via `prost-reflect` (RFC-0323).
//!
//! `prost` drops unknown extension fields on decode, so the plugin cannot read
//! `actr.reply_mode` (or `actr.payload_type`) off the `MethodDescriptorProto`s
//! it decodes from `CodeGeneratorRequest.proto_file`. `prost-reflect`'s
//! `DescriptorPool`, by contrast, decodes the descriptor tree with its own
//! retaining types and surfaces custom options via `MethodDescriptor::options()`.
//!
//! The plugin receives a `CodeGeneratorRequest` on stdin — *not* a
//! `FileDescriptorSet` — so [`DescriptorPool::decode`] (which expects a
//! `FileDescriptorSet`) does not apply directly. Instead we scan the raw
//! request bytes for the `proto_file` field (tag 15, length-delimited), feed
//! each raw `FileDescriptorProto` slice to
//! [`DescriptorPool::decode_file_descriptor_proto`], and read the
//! `actr.reply_mode` extension off every method. protoc emits `proto_file`
//! in topological order (each file before any file that imports it), so the
//! per-file decode succeeds without forward references.

use std::collections::HashMap;

use anyhow::{Context, Result};
use prost_reflect::{DescriptorPool, Value};

use actr_protocol::ReplyMode;

/// Scan `raw` (a protobuf-encoded message) for every length-delimited field
/// with the given field number and return the raw byte slices, in wire order.
///
/// This is a minimal protobuf wire-format scanner: it walks top-level fields,
/// reads the tag varint, and skips values by wire type. Only length-delimited
/// (wire type 2) fields matching `field_number` are captured. Groups (wire
/// types 3/4) are not produced by protoc for `CodeGeneratorRequest` or
/// `FileDescriptorSet`; encountering one stops the scan.
fn extract_length_delimited_field(raw: &[u8], field_number: u32) -> Vec<&[u8]> {
    let mut out = Vec::new();
    let mut pos = 0;
    while pos < raw.len() {
        let (tag, n) = match read_varint(&raw[pos..]) {
            Some(v) => v,
            None => break,
        };
        pos += n;
        let fnum = (tag >> 3) as u32;
        let wiretype = (tag & 0x7) as u8;
        match wiretype {
            0 => {
                // varint value — skip it
                let (_, n) = match read_varint(&raw[pos..]) {
                    Some(v) => v,
                    None => break,
                };
                pos += n;
            }
            1 => {
                // 64-bit — skip 8 bytes
                pos = match pos.checked_add(8) {
                    Some(p) if p <= raw.len() => p,
                    _ => raw.len(),
                };
            }
            2 => {
                // length-delimited
                let (len, n) = match read_varint(&raw[pos..]) {
                    Some(v) => v,
                    None => break,
                };
                pos += n;
                let len = len as usize;
                let end = match pos.checked_add(len) {
                    Some(p) if p <= raw.len() => p,
                    _ => raw.len(),
                };
                if fnum == field_number {
                    out.push(&raw[pos..end]);
                }
                pos = end;
            }
            5 => {
                // 32-bit — skip 4 bytes
                pos = match pos.checked_add(4) {
                    Some(p) if p <= raw.len() => p,
                    _ => raw.len(),
                };
            }
            _ => break, // group start/end (3/4) or invalid — stop
        }
    }
    out
}

/// Read a base-128 varint from `buf`. Returns the value and the number of
/// bytes consumed, or `None` if the buffer is truncated or the varint
/// overflows 64 bits.
fn read_varint(buf: &[u8]) -> Option<(u64, usize)> {
    let mut result: u64 = 0;
    let mut shift = 0;
    for (i, &byte) in buf.iter().enumerate() {
        result |= ((byte & 0x7F) as u64) << shift;
        if byte & 0x80 == 0 {
            return Some((result, i + 1));
        }
        shift += 7;
        if shift >= 64 {
            return None;
        }
    }
    None
}

/// Build a `route_key -> ReplyMode` map from raw `FileDescriptorProto` byte
/// slices (one per file, in topological order).
///
/// `route_key` is formatted as `"{package}.{service}.{method}"` when a package
/// exists, or `"{service}.{method}"` otherwise, matching the generator's own
/// route_key construction exactly so lookups align.
pub fn build_reply_mode_map(proto_file_bytes: &[&[u8]]) -> Result<HashMap<String, ReplyMode>> {
    let mut pool = DescriptorPool::new();
    for file_bytes in proto_file_bytes {
        pool.decode_file_descriptor_proto(*file_bytes)
            .context("decoding proto_file via prost-reflect")?;
    }

    let reply_mode_ext = pool.get_extension_by_name("actr.reply_mode");
    let mut map = HashMap::new();
    for file in pool.files() {
        let package = file.package_name();
        for svc in file.services() {
            let svc_name = svc.name();
            for method in svc.methods() {
                let route_key = if package.is_empty() {
                    format!("{}.{}", svc_name, method.name())
                } else {
                    format!("{}.{}.{}", package, svc_name, method.name())
                };
                let mode = reply_mode_of(&method, reply_mode_ext.as_ref());
                map.insert(route_key, mode);
            }
        }
    }
    Ok(map)
}

/// Read the `actr.reply_mode` extension off a method's options. Returns
/// `ReplyMode::Return` when the extension is unset or absent from the pool
/// (i.e. `options.proto` wasn't imported).
fn reply_mode_of(
    method: &prost_reflect::MethodDescriptor,
    ext: Option<&prost_reflect::ExtensionDescriptor>,
) -> ReplyMode {
    let Some(ext) = ext else {
        return ReplyMode::Return;
    };
    let opts = method.options();
    let val = opts.get_extension(ext);
    match &*val {
        Value::EnumNumber(1) => ReplyMode::Explicit,
        _ => ReplyMode::Return,
    }
}

/// Extract the `route_key -> ReplyMode` map from a raw `CodeGeneratorRequest`
/// buffer (the plugin's stdin). Scans for `proto_file` (field 15) slices and
/// builds the pool from them.
pub fn extract_reply_modes(raw_request: &[u8]) -> Result<HashMap<String, ReplyMode>> {
    let proto_files = extract_length_delimited_field(raw_request, 15);
    build_reply_mode_map(&proto_files)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::process::Command;
    use tempfile::TempDir;

    const OPTIONS_PROTO: &str =
        include_str!("../../../../../crates/actr-protocol/proto/actr/options.proto");

    const SERVICE_PROTO: &str = r#"syntax = "proto2";
package demo;
import "actr/options.proto";
message Req { optional string s = 1; }
message Resp { optional string r = 1; }
service Svc {
  rpc Probe(Req) returns (Resp);
  rpc Transcode(Req) returns (Resp) {
    option (actr.reply_mode) = REPLY_MODE_EXPLICIT;
  }
  rpc Signal(Req) returns (Resp) {
    option (actr.payload_type) = RPC_SIGNAL;
  }
}
"#;

    /// Run protoc to produce a descriptor set (options preserved), then exercise
    /// the *plugin's* path: pull raw `FileDescriptorProto` byte-slices out of
    /// the descriptor set (field 1) and feed them to `build_reply_mode_map` —
    /// the same function the plugin calls on the `proto_file` (field 15) slices
    /// it scans out of `CodeGeneratorRequest` stdin.
    #[test]
    fn reply_mode_map_matches_proto_options() {
        let temp = TempDir::new().expect("temp dir");
        let proto_root = temp.path().join("proto");
        let actr_dir = proto_root.join("actr");
        fs::create_dir_all(&actr_dir).expect("actr dir");
        fs::write(actr_dir.join("options.proto"), OPTIONS_PROTO).expect("options.proto");
        fs::write(proto_root.join("svc.proto"), SERVICE_PROTO).expect("svc.proto");

        let desc_set = temp.path().join("desc.bin");
        let output = Command::new("protoc")
            .arg(format!("--proto_path={}", proto_root.display()))
            .arg("--include_imports")
            .arg(format!("--descriptor_set_out={}", desc_set.display()))
            .arg(proto_root.join("svc.proto"))
            .output()
            .expect("run protoc");
        assert!(
            output.status.success(),
            "protoc failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );

        let bytes = fs::read(&desc_set).expect("read descriptor set");
        // A FileDescriptorSet stores its files at field 1 (length-delimited).
        let file_slices = extract_length_delimited_field(&bytes, 1);
        assert!(
            !file_slices.is_empty(),
            "descriptor set had no file entries"
        );

        let map = build_reply_mode_map(&file_slices).expect("build map");

        assert_eq!(
            map.get("demo.Svc.Probe")
                .copied()
                .unwrap_or(ReplyMode::Return),
            ReplyMode::Return,
            "Probe has no option → default Return"
        );
        assert_eq!(
            map.get("demo.Svc.Transcode")
                .copied()
                .unwrap_or(ReplyMode::Return),
            ReplyMode::Explicit,
            "Transcode is marked EXPLICIT"
        );
        // Signal sets payload_type, not reply_mode → Return.
        assert_eq!(
            map.get("demo.Svc.Signal")
                .copied()
                .unwrap_or(ReplyMode::Explicit),
            ReplyMode::Return,
            "Signal sets payload_type, not reply_mode → Return"
        );
    }

    /// The plugin reads `proto_file` at field 15 of `CodeGeneratorRequest`.
    /// Wrap the descriptor set's file-slices as field-15 entries and confirm
    /// `extract_reply_modes` recovers the same map the plugin would build.
    #[test]
    fn extract_reply_modes_scans_code_generator_request_field_15() {
        let temp = TempDir::new().expect("temp dir");
        let proto_root = temp.path().join("proto");
        let actr_dir = proto_root.join("actr");
        fs::create_dir_all(&actr_dir).expect("actr dir");
        fs::write(actr_dir.join("options.proto"), OPTIONS_PROTO).expect("options.proto");
        fs::write(proto_root.join("svc.proto"), SERVICE_PROTO).expect("svc.proto");

        let desc_set = temp.path().join("desc.bin");
        let output = Command::new("protoc")
            .arg(format!("--proto_path={}", proto_root.display()))
            .arg("--include_imports")
            .arg(format!("--descriptor_set_out={}", desc_set.display()))
            .arg(proto_root.join("svc.proto"))
            .output()
            .expect("run protoc");
        assert!(output.status.success(), "protoc failed");

        let bytes = fs::read(&desc_set).expect("read descriptor set");
        let file_slices = extract_length_delimited_field(&bytes, 1);

        // Build a CodeGeneratorRequest-shaped buffer: only `proto_file` (field
        // 15, wire type 2) entries, each carrying a raw FileDescriptorProto.
        let mut req_buf = Vec::new();
        for slice in &file_slices {
            write_field_15_proto_file(&mut req_buf, slice);
        }

        let map = extract_reply_modes(&req_buf).expect("extract from CGR");
        assert_eq!(
            map.get("demo.Svc.Transcode")
                .copied()
                .unwrap_or(ReplyMode::Return),
            ReplyMode::Explicit
        );
        assert_eq!(
            map.get("demo.Svc.Probe")
                .copied()
                .unwrap_or(ReplyMode::Explicit),
            ReplyMode::Return
        );
    }

    /// Write `file_bytes` as a `proto_file` (field 15, length-delimited) entry.
    fn write_field_15_proto_file(out: &mut Vec<u8>, file_bytes: &[u8]) {
        // tag = (15 << 3) | 2 = 122 = 0x7A
        out.push(0x7A);
        write_varint(out, file_bytes.len() as u64);
        out.extend_from_slice(file_bytes);
    }

    fn write_varint(out: &mut Vec<u8>, mut value: u64) {
        while value >= 0x80 {
            out.push((value as u8) | 0x80);
            value >>= 7;
        }
        out.push(value as u8);
    }

    #[test]
    fn read_varint_handles_single_and_multi_byte() {
        assert_eq!(read_varint(&[0x00]), Some((0, 1)));
        assert_eq!(read_varint(&[0x01]), Some((1, 1)));
        // 300 = 0b100101100 → two bytes: [0xAC, 0x02]
        assert_eq!(read_varint(&[0xAC, 0x02]), Some((300, 2)));
    }

    #[test]
    fn read_varint_truncated_returns_none() {
        assert_eq!(read_varint(&[]), None);
        // continuation bit set but no following byte
        assert_eq!(read_varint(&[0x80]), None);
    }

    #[test]
    fn extract_length_delimited_field_picks_only_matching_field() {
        // field 1 (varint), field 2 (length-delimited, "hello"), field 15
        // (length-delimited, "world"), field 15 (length-delimited, "!")
        // field 1, wiretype 0: tag = (1<<3)|0 = 8, value 5
        let mut buf = vec![0x08, 0x05];
        // field 2, wiretype 2: tag = (2<<3)|2 = 18 = 0x12, len 5, "hello"
        buf.push(0x12);
        buf.push(0x05);
        buf.extend_from_slice(b"hello");
        // field 15, wiretype 2: tag = (15<<3)|2 = 122 = 0x7A, len 5, "world"
        buf.push(0x7A);
        buf.push(0x05);
        buf.extend_from_slice(b"world");
        // field 15, wiretype 2: len 1, "!"
        buf.push(0x7A);
        buf.push(0x01);
        buf.push(b'!');

        let f15 = extract_length_delimited_field(&buf, 15);
        assert_eq!(f15.len(), 2);
        assert_eq!(f15[0], b"world");
        assert_eq!(f15[1], b"!");

        let f2 = extract_length_delimited_field(&buf, 2);
        assert_eq!(f2.len(), 1);
        assert_eq!(f2[0], b"hello");
    }

    #[test]
    fn extract_reply_modes_empty_buffer_is_empty_map() {
        let map = extract_reply_modes(&[]).expect("empty input");
        assert!(map.is_empty());
    }
}

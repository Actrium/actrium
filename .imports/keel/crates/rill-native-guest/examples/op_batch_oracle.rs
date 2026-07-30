//! op-batch wire-bytes oracle CLI — dev harness helper, never shipped.
//!
//! Reads a JSON ARRAY of op-batches on stdin (each in the
//! `SerializedOperationBatch` shape the golden vectors use:
//! `{version, batchId, operations: [...]}`) and prints ONE lowercase-hex line
//! per batch: the exact `contracts/op-batch-wire.json` bytes produced by the
//! schema-locked Rust encoder (`wire_encode.rs`). A FRESH encoder is used per
//! batch (intern indices from 0, header flags NONE) — the same convention the
//! golden vectors are generated with.
//!
//! Consumer: `src/host/native-guest/__benchmarks__/op-batch-bench.ts`. Its
//! faithfulness gate pushes the checked-in golden vectors through this CLI and
//! byte-compares the output against the golden hex before timing anything, so
//! the JSON mapping (`wire_fixture_map.rs`, shared with the golden re-encode
//! tests) is pinned to the oracle corpus on every bench run.
//!
//! Run:
//!   cargo run -p rill-native-guest --features binary-protocol \
//!     --example op_batch_oracle < batches.json

use rill_native_guest::wire_encode::{Batch, Encoder, Op, Value};
use std::io::Read;

// The crate's minimal JSON parser and the golden-JSON -> {Op, Value} mapping.
// Both are `include!`d (the same pattern build.rs uses for mini_json): they are
// std-side helpers of an otherwise `no_std` crate and are not part of its
// public API.
include!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/mini_json.rs"));
include!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/src/wire_fixture_map.rs"
));

fn main() {
    let mut input = String::new();
    std::io::stdin()
        .read_to_string(&mut input)
        .expect("read stdin");
    let doc = Json::parse(&input).expect("stdin is not valid JSON");
    let batches = doc
        .items()
        .expect("expected a top-level JSON array of batches");

    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::new();
    for (i, b) in batches.iter().enumerate() {
        let batch = Batch {
            batch_id: b
                .get("batchId")
                .and_then(Json::as_u64)
                .unwrap_or_else(|| panic!("batch[{i}]: missing u32 batchId"))
                as u32,
            ops: b
                .get("operations")
                .and_then(Json::items)
                .unwrap_or_else(|| panic!("batch[{i}]: missing operations[]"))
                .iter()
                .map(json_to_op)
                .collect(),
        };
        let mut encoder = Encoder::new();
        let bytes = encoder
            .encode_batch(&batch)
            .unwrap_or_else(|e| panic!("batch[{i}]: encode failed: {e:?}"));
        out.reserve(bytes.len() * 2 + 1);
        for byte in &bytes {
            out.push(HEX[(byte >> 4) as usize] as char);
            out.push(HEX[(byte & 0x0f) as usize] as char);
        }
        out.push('\n');
    }
    print!("{out}");
}

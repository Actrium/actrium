//! Probes whether prost retains unknown extension fields on a decode→encode
//! roundtrip. This determines whether `reply_mode` / `payload_type` options can
//! be extracted from `MethodDescriptorProto` without a reflection crate.

use prost::Message;
use prost_types::MethodOptions;

#[test]
fn prost_drops_unknown_extension_fields() {
    // Hand-encoded MethodOptions carrying an unknown extension:
    //   field 50002 (reply_mode), wiretype 0 (varint), value 1 (EXPLICIT).
    //   tag varint for 50002<<3 = 400016 → [144, 181, 24]; value 1 → [1].
    let raw: Vec<u8> = vec![144, 181, 24, 1];

    let opts = MethodOptions::decode(&*raw).expect("decode");

    let mut reencoded = Vec::new();
    let _ = opts.encode(&mut reencoded);

    // If prost retained unknown fields, `reencoded == raw`. prost drops them,
    // so `reencoded` is empty and `uninterpreted_option` is too. This is the
    // blocker that makes option extraction require a reflection crate (or raw
    // descriptor parsing) — shared with the existing `payload_type` extractor.
    let retained = raw == reencoded;
    assert!(
        !retained,
        "prost retained the unknown extension; re-parsing encoded bytes would work"
    );
    assert!(opts.uninterpreted_option.is_empty());
    assert!(reencoded.is_empty());
}

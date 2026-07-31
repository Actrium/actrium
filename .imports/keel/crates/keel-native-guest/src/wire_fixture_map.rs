// Fixture-side mapping: golden/matrix JSON (the SerializedOperationBatch shape
// of contracts/op-batch-wire.golden.json) -> `wire_encode` {Op, Value} input,
// applying the contract's NUMBER RULE (integer in i32 range -> Int32, any other
// finite number -> Float64) and the fixture-only discriminators (`__mv`
// undefined sentinel, `__type` special kinds).
//
// This file is `include!`d — never compiled standalone — by BOTH std-side
// consumers of the otherwise `no_std` crate (same pattern as `mini_json.rs`):
//
//  - the `wire_encode` test module, whose golden/matrix re-encode tests lock
//    the mapping + encoder byte-for-byte to the oracle corpus;
//  - `examples/op_batch_oracle.rs`, the dev-harness CLI that hands the same
//    oracle bytes to `src/host/native-guest/__benchmarks__/op-batch-bench.ts`
//    (whose faithfulness gate re-encodes the golden vectors through the CLI
//    and byte-compares, pinning this mapping a second time).
//
// Expected in scope at the include! site: `Json` (mini_json), `Op`, `Value`
// (wire_encode), plus std/alloc `Vec` and format/panic macros.

/// Map a golden JSON prop/arg value to an encoder [`Value`], applying the
/// contract's NUMBER RULE (integer in i32 range -> Int32, else Float64).
/// Only the JSON kinds the golden vectors use are needed, but objects and
/// arrays are handled generically so richer vectors would work too.
fn json_to_value(j: &Json) -> Value<'_> {
    match j {
        Json::Null => Value::Null,
        Json::Bool(b) => Value::Bool(*b),
        Json::Str(s) => Value::Str(s.as_str()),
        Json::Num(n) => {
            if n.fract() == 0.0 && *n >= i32::MIN as f64 && *n <= i32::MAX as f64 {
                Value::Int32(*n as i32)
            } else {
                Value::Float64(*n)
            }
        }
        Json::Arr(items) => Value::Array(items.iter().map(json_to_value).collect()),
        Json::Obj(entries) => {
            // A JSON object is a plain OBJECT value UNLESS it carries one of
            // the matrix fixture's two fixture-only discriminators: an "__mv"
            // sentinel (for a value JSON cannot express, e.g. undefined) or a
            // "__type" tag (function/date/error/regexp/map/set/promise, the
            // guest SerializedValue special kinds). The golden vectors use
            // neither, so their plain objects/props fall straight through.
            if let Some(mv) = j.get("__mv").and_then(Json::as_str) {
                match mv {
                    "undefined" => Value::Undefined,
                    other => panic!("unknown __mv sentinel {other:?}"),
                }
            } else if let Some(ty) = j.get("__type").and_then(Json::as_str) {
                match ty {
                    "function" => Value::Function {
                        fn_id: req_str(j, "__fnId"),
                        name: j.get("__name").and_then(Json::as_str),
                        source_file: j.get("__sourceFile").and_then(Json::as_str),
                        source_line: j
                            .get("__sourceLine")
                            .and_then(Json::as_u64)
                            .map(|n| n as u32),
                    },
                    "date" => Value::Date {
                        epoch_ms: req_f64(j, "__epochMs"),
                    },
                    "error" => Value::Error {
                        name: req_str(j, "__name"),
                        message: req_str(j, "__message"),
                        // Empty/absent stack interns as "" (decoders map that
                        // back to an absent stack).
                        stack: j.get("__stack").and_then(Json::as_str).unwrap_or(""),
                    },
                    "regexp" => Value::Regexp {
                        source: req_str(j, "__source"),
                        flags: req_str(j, "__flags"),
                    },
                    "promise" => Value::Promise {
                        promise_id: req_str(j, "__promiseId"),
                    },
                    "map" => Value::Map(
                        j.get("__entries")
                            .and_then(Json::items)
                            .expect("map __entries[]")
                            .iter()
                            .map(|pair| {
                                let kv = pair.items().expect("map entry is [key, value]");
                                (json_to_value(&kv[0]), json_to_value(&kv[1]))
                            })
                            .collect(),
                    ),
                    "set" => Value::Set(
                        j.get("__values")
                            .and_then(Json::items)
                            .expect("set __values[]")
                            .iter()
                            .map(json_to_value)
                            .collect(),
                    ),
                    other => panic!("unknown __type discriminator {other:?}"),
                }
            } else {
                Value::Object(
                    entries
                        .iter()
                        .map(|(k, v)| (k.as_str(), json_to_value(v)))
                        .collect(),
                )
            }
        }
    }
}

/// A required interned-string field of a `__type`-tagged matrix value.
fn req_str<'a>(j: &'a Json, key: &str) -> &'a str {
    j.get(key)
        .and_then(Json::as_str)
        .unwrap_or_else(|| panic!("matrix value missing string field {key}"))
}

/// A required numeric field (used for `Date.__epochMs`, which may be
/// negative, so `as_u64` is not enough).
fn req_f64(j: &Json, key: &str) -> f64 {
    match j.get(key) {
        Some(Json::Num(n)) => *n,
        _ => panic!("matrix value missing number field {key}"),
    }
}

fn json_props(j: &Json) -> Vec<(&str, Value<'_>)> {
    match j {
        Json::Obj(entries) => entries
            .iter()
            .map(|(k, v)| (k.as_str(), json_to_value(v)))
            .collect(),
        _ => Vec::new(),
    }
}

fn u32_field(op: &Json, key: &str) -> u32 {
    op.get(key)
        .and_then(Json::as_u64)
        .unwrap_or_else(|| panic!("golden op missing u32 field {key}")) as u32
}

/// Build an [`Op`] from one golden batch operation object.
fn json_to_op(op: &Json) -> Op<'_> {
    let kind = op.get("op").and_then(Json::as_str).expect("op.op");
    let id = u32_field(op, "id");
    match kind {
        "CREATE" => Op::Create {
            id,
            ty: op.get("type").and_then(Json::as_str).expect("CREATE.type"),
            props: op.get("props").map(json_props).unwrap_or_default(),
        },
        "UPDATE" => Op::Update {
            id,
            props: op.get("props").map(json_props).unwrap_or_default(),
            removed: op
                .get("removedProps")
                .and_then(Json::items)
                .map(|items| {
                    items
                        .iter()
                        .map(|s| s.as_str().expect("removedProps entry"))
                        .collect()
                })
                .unwrap_or_default(),
        },
        "DELETE" => Op::Delete { id },
        "APPEND" => Op::Append {
            id,
            parent_id: u32_field(op, "parentId"),
            child_id: u32_field(op, "childId"),
        },
        "INSERT" => Op::Insert {
            id,
            parent_id: u32_field(op, "parentId"),
            child_id: u32_field(op, "childId"),
            index: u32_field(op, "index") as u16,
        },
        "REMOVE" => Op::Remove {
            id,
            parent_id: u32_field(op, "parentId"),
            child_id: u32_field(op, "childId"),
        },
        "REORDER" => Op::Reorder {
            id,
            parent_id: u32_field(op, "parentId"),
            child_ids: op
                .get("childIds")
                .and_then(Json::items)
                .map(|items| items.iter().map(|c| c.as_u64().unwrap() as u32).collect())
                .unwrap_or_default(),
        },
        "TEXT" => Op::Text {
            id,
            text: op.get("text").and_then(Json::as_str).expect("TEXT.text"),
        },
        "REF_CALL" => Op::RefCall {
            id,
            method: op.get("method").and_then(Json::as_str).expect("method"),
            call_id: op.get("callId").and_then(Json::as_str).expect("callId"),
            args: op
                .get("args")
                .and_then(Json::items)
                .map(|items| items.iter().map(json_to_value).collect())
                .unwrap_or_default(),
        },
        other => panic!("unknown golden op kind {other}"),
    }
}

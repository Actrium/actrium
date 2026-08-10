//! Bundle identity tests for `export_bundle!`.
//!
//! This is a separate integration test binary so the `export_bundle!`
//! submission here stays isolated from `tests/integration.rs`'s process-global
//! inventory view (each `tests/*.rs` file compiles to its own binary, and
//! inventory submissions are per-binary).

actr_cabi::export_bundle!("cabi.test-bundle");

#[test]
fn bundle_id_is_registered() {
    assert_eq!(actr_cabi::bundle_id(), Some("cabi.test-bundle"));
}

#[test]
fn bundle_count_is_one() {
    assert_eq!(actr_cabi::bundle_count(), 1);
}

#[test]
fn bundle_id_query_via_c_abi() {
    // SAFETY: out_len is a writable local; the query writes the static string
    // length and returns a static pointer valid for the process lifetime.
    let mut len = 0usize;
    let ptr = unsafe { actr_cabi::actr_cabi_bundle_id(&mut len) };
    assert!(
        !ptr.is_null(),
        "actr_cabi_bundle_id must return the registered id"
    );
    // SAFETY: the pointer is valid for `len` bytes for the process lifetime.
    let bytes = unsafe { std::slice::from_raw_parts(ptr.cast::<u8>(), len) };
    assert_eq!(
        std::str::from_utf8(bytes).expect("bundle id must be valid UTF-8"),
        "cabi.test-bundle"
    );
    assert_eq!(actr_cabi::actr_cabi_bundle_count(), 1);
}

#[test]
fn abi_and_actr_version_still_queryable_alongside_bundle() {
    // The introspection queries coexist with a registered bundle in the same
    // binary — a host calls all of them during native validation.
    assert_eq!(actr_cabi::actr_cabi_abi_version(), actr_cabi::ABI_VERSION);
    // SAFETY: out_len is a writable local.
    let mut len = 0usize;
    let ptr = unsafe { actr_cabi::actr_cabi_actr_version(&mut len) };
    assert!(!ptr.is_null());
    // SAFETY: static pointer, valid for `len` bytes.
    let bytes = unsafe { std::slice::from_raw_parts(ptr.cast::<u8>(), len) };
    assert_eq!(std::str::from_utf8(bytes).unwrap(), actr_cabi::ACTR_VERSION);
}

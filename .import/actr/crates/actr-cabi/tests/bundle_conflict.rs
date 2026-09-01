//! Multi-bundle conflict test for `export_bundle!`.
//!
//! A separate integration test binary that registers TWO bundle IDs, so the
//! `bundle_count() > 1` conflict path (detected and rejected by the C# host
//! in `ActrCabiNode.ValidateNative`) is exercised at the Rust level too.
//! `bundle_id()` returns an arbitrary first entry when multiple are registered;
//! this test only asserts the count and that the returned id is one of the
//! two registered values.

actr_cabi::export_bundle!("cabi.conflict.a");
actr_cabi::export_bundle!("cabi.conflict.b");

#[test]
fn multiple_bundles_register_both() {
    assert_eq!(
        actr_cabi::bundle_count(),
        2,
        "both export_bundle! entries should register"
    );
}

#[test]
fn bundle_id_returns_one_of_the_registered_values() {
    let id = actr_cabi::bundle_id();
    assert!(
        matches!(id, Some("cabi.conflict.a") | Some("cabi.conflict.b")),
        "bundle_id() should return one of the registered ids, got {id:?}"
    );
}

#[test]
fn bundle_count_via_c_abi_matches_rust() {
    assert_eq!(actr_cabi::actr_cabi_bundle_count(), 2);
}

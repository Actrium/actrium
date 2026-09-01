use super::*;

#[test]
fn parse_semver_extracts_three_component_versions() {
    assert_eq!(parse_semver("1.2.3"), Some((1, 2, 3)));
    assert_eq!(parse_semver("0.5.22"), Some((0, 5, 22)));
    assert_eq!(parse_semver("1.0"), None);
    assert_eq!(parse_semver("abc"), None);
    assert_eq!(parse_semver(""), None);
    assert_eq!(parse_semver("1.2.3-beta"), Some((1, 2, 3)));
}

#[test]
fn extract_semver_finds_first_semver_in_text() {
    assert_eq!(extract_semver("wasm-component-ld 0.5.22"), Some((0, 5, 22)));
    assert_eq!(extract_semver("version 1.0.0 (abc 2.3.4)"), Some((1, 0, 0)));
    assert_eq!(extract_semver("no version here"), None);
}

#[test]
fn format_semver_joins_with_dots() {
    assert_eq!(format_semver((1, 2, 3)), "1.2.3");
    assert_eq!(format_semver((0, 5, 22)), "0.5.22");
}

#[test]
fn resolve_manifest_path_errors_when_file_absent() {
    let dir = tempfile::TempDir::new().unwrap();
    let p = dir.path().join("manifest.toml");
    assert!(resolve_manifest_path(&p).is_err());
}

#[test]
fn resolve_output_path_handles_absolute_relative_and_default() {
    let mf = std::path::Path::new("/proj/manifest.toml");
    assert_eq!(
        resolve_output_path(
            mf,
            "x86_64-linux",
            Some(&std::path::PathBuf::from("/abs/pkg.actr"))
        )
        .unwrap(),
        std::path::PathBuf::from("/abs/pkg.actr")
    );
    assert_eq!(
        resolve_output_path(
            mf,
            "x86_64-linux",
            Some(&std::path::PathBuf::from("rel/pkg.actr"))
        )
        .unwrap(),
        std::path::PathBuf::from("/proj/rel/pkg.actr")
    );
    // Without explicit output, defaults to dist/<name>-<target>.actr
    let err = resolve_output_path(mf, "x86_64-linux", None).unwrap_err();
    assert!(format!("{err}").contains("manifest"));
}

#[test]
fn web_package_authenticates_wasm_bindgen_glue() {
    let dir = tempfile::TempDir::new().unwrap();
    let binary = dir.path().join("echo_guest_bg.wasm");
    let glue = dir.path().join("echo_guest.js");
    std::fs::write(&binary, b"wasm").unwrap();
    std::fs::write(&glue, b"let wasm_bindgen = {};").unwrap();

    let resources =
        authenticated_wasm_bindgen_resources(&binary, "wasm32-unknown-unknown").unwrap();
    assert_eq!(resources, vec![("resources/glue.js".to_string(), glue)]);
}

#[test]
fn web_package_rejects_missing_or_misnamed_wasm_bindgen_artifacts() {
    let dir = tempfile::TempDir::new().unwrap();
    let misnamed = dir.path().join("echo_guest.wasm");
    std::fs::write(&misnamed, b"wasm").unwrap();
    assert!(authenticated_wasm_bindgen_resources(&misnamed, "wasm32-unknown-unknown").is_err());

    let binary = dir.path().join("echo_guest_bg.wasm");
    std::fs::write(&binary, b"wasm").unwrap();
    let error = authenticated_wasm_bindgen_resources(&binary, "wasm32-unknown-unknown")
        .unwrap_err()
        .to_string();
    assert!(error.contains("glue not found"));
}

#[test]
fn native_package_does_not_require_wasm_bindgen_resources() {
    let resources = authenticated_wasm_bindgen_resources(
        std::path::Path::new("component.wasm"),
        "wasm32-wasip2",
    )
    .unwrap();
    assert!(resources.is_empty());
}

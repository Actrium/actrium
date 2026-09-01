//! Integration coverage for the `actr registry publish` manufacturer
//! binding check (issue #425).
//!
//! Builds a real `.actr` package in-process via `actr_pack`, then drives the
//! `actr` binary to assert that a keychain bound to a different manufacturer is
//! rejected before any network call is attempted.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as B64;
use ed25519_dalek::SigningKey;
use tempfile::TempDir;

fn actr_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_actr"))
}

fn run_actr(args: &[&str], cwd: &std::path::Path, home: &std::path::Path) -> std::process::Output {
    Command::new(actr_bin())
        .args(args)
        .current_dir(cwd)
        .env("HOME", home)
        .env("NO_COLOR", "1")
        .env("RUST_LOG", "off")
        .output()
        .expect("failed to run actr binary")
}

/// Write a keychain JSON file bound to `manufacturer`, using `signing_key`'s
/// material so that its `key_id` matches any package signed by the same key.
fn write_keychain(path: &std::path::Path, signing_key: &SigningKey, manufacturer: &str) {
    let json = serde_json::json!({
        "private_key": B64.encode(signing_key.to_bytes()),
        "public_key": B64.encode(signing_key.verifying_key().to_bytes()),
        "manufacturer": manufacturer,
    });
    fs::write(path, json.to_string()).expect("write keychain");
}

/// Pack a minimal `.actr` package whose manifest declares `manufacturer`.
fn build_package(path: &std::path::Path, signing_key: &SigningKey, manufacturer: &str) {
    let manifest = actr_pack::PackageManifest {
        manufacturer: manufacturer.to_string(),
        name: "echo-service".to_string(),
        version: "0.1.0".to_string(),
        binary: actr_pack::BinaryEntry {
            path: "bin/actor.wasm".to_string(),
            target: "wasm32-wasip1".to_string(),
            hash: String::new(),
            size: None,
            kind: None,
        },
        signature_algorithm: "ed25519".to_string(),
        signing_key_id: Some(actr_pack::compute_key_id(
            &signing_key.verifying_key().to_bytes(),
        )),
        resources: vec![],
        proto_files: vec![],
        lock_file: None,
        metadata: actr_pack::ManifestMetadata {
            description: Some("echo".to_string()),
            license: None,
        },
    };
    let bytes = actr_pack::pack(&actr_pack::PackOptions {
        manifest,
        binary_bytes: b"fake-wasm".to_vec(),
        resources: vec![],
        proto_files: vec![],
        signing_key: signing_key.clone(),
        lock_file: None,
    })
    .expect("pack");
    fs::write(path, bytes).expect("write package");
}

#[test]
fn registry_publish_rejects_cross_manufacturer_keychain() {
    let tmp = TempDir::new().expect("tempdir");
    let home = tmp.path().join("home");
    fs::create_dir_all(&home).expect("create home");

    // One signing key signs the package; the keychain reuses the same key so
    // the `key_id` matches and the only failing check is the manufacturer
    // binding.
    let signing_key = SigningKey::generate(&mut rand::rngs::OsRng);

    let pkg_path = tmp.path().join("acme-echo-0.1.0-wasm32-wasip1.actr");
    build_package(&pkg_path, &signing_key, "acme");

    let keychain_path = tmp.path().join("rival-keychain.json");
    write_keychain(&keychain_path, &signing_key, "rival");

    let pkg_arg = pkg_path.to_string_lossy().into_owned();
    let kc_arg = keychain_path.to_string_lossy().into_owned();

    let out = run_actr(
        &[
            "registry",
            "publish",
            "--package",
            &pkg_arg,
            "--keychain",
            &kc_arg,
            "--endpoint",
            "http://localhost:1",
        ],
        tmp.path(),
        &home,
    );
    assert!(
        !out.status.success(),
        "publish should reject cross-manufacturer keychain, stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("Manufacturer mismatch"),
        "expected manufacturer mismatch, stderr:\n{err}"
    );
}

#[test]
fn registry_publish_accepts_matching_manufacturer_keychain() {
    let tmp = TempDir::new().expect("tempdir");
    let home = tmp.path().join("home");
    fs::create_dir_all(&home).expect("create home");

    let signing_key = SigningKey::generate(&mut rand::rngs::OsRng);
    let pkg_path = tmp.path().join("acme-echo-0.1.0-wasm32-wasip1.actr");
    build_package(&pkg_path, &signing_key, "acme");

    let keychain_path = tmp.path().join("acme-keychain.json");
    write_keychain(&keychain_path, &signing_key, "acme");

    let pkg_arg = pkg_path.to_string_lossy().into_owned();
    let kc_arg = keychain_path.to_string_lossy().into_owned();

    let out = run_actr(
        &[
            "registry",
            "publish",
            "--package",
            &pkg_arg,
            "--keychain",
            &kc_arg,
            // Unreachable endpoint: the manufacturer check must pass, but the
            // nonce request fails at the network layer (no server running).
            "--endpoint",
            "http://127.0.0.1:1",
        ],
        tmp.path(),
        &home,
    );
    assert!(
        !out.status.success(),
        "publish should fail at the network layer, not pass, stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
    // The manufacturer check passed — the failure must NOT mention mismatch.
    let err = String::from_utf8_lossy(&out.stderr);
    let combined = format!("{}{}", String::from_utf8_lossy(&out.stdout), err);
    assert!(
        !combined.contains("Manufacturer mismatch"),
        "matching manufacturer should not be rejected:\n{combined}"
    );
}

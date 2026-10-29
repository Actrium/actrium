//! Drive `actr realm trust --reset` through the real CLI: pinned-trust reset
//! semantics against the on-disk store (the fetch paths are covered by the
//! `realm_pins`/`realm_members` unit tests).

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use serde_json::Value;

fn run(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_actr"))
        .args(args)
        .current_dir(dir)
        .env("HOME", dir)
        .env("USERPROFILE", dir)
        .env("NO_COLOR", "1")
        .env("RUST_LOG", "off")
        .output()
        .expect("run actr")
}

fn succeeds(output: Output) -> Value {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("stdout is only JSON")
}

#[test]
fn trust_reset_reports_and_removes_pinned_trust() {
    let dir = tempfile::tempdir().unwrap();

    // Nothing pinned: reset is a reported no-op.
    let reply = succeeds(run(dir.path(), &["realm", "trust", "acme.com", "--reset"]));
    assert_eq!(reply["domain"], "acme.com");
    assert_eq!(reply["reset"], false);

    // A pinned file is removed and reported.
    let pins = dir.path().join(".actr").join("realms");
    fs::create_dir_all(&pins).unwrap();
    let file = pins.join("acme.com.json");
    fs::write(&file, "{}").unwrap();
    let reply = succeeds(run(dir.path(), &["realm", "trust", "acme.com", "--reset"]));
    assert_eq!(reply["reset"], true);
    assert!(!file.exists());
}

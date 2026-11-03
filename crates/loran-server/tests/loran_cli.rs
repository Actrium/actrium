use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

fn write_valid_config(dir: &Path, file_name: &str) -> PathBuf {
    let data_dir = dir.join("data");
    fs::create_dir_all(&data_dir).expect("create data dir");

    let config_path = dir.join(file_name);
    fs::write(
        &config_path,
        format!(
            r#"
name = "loran-cli-test"
enable = 16
env = "dev"
sqlite_path = "{sqlite}"
loran_shared_key = "0123456789abcdef0123456789abcdef"
location_tag = "local,test,cli"
pid = "{pid}"

[bind]
[bind.http]
domain_name = "localhost"
advertised_ip = "127.0.0.1"
ip = "127.0.0.1"
port = 39999

[bind.ice]
advertised_ip = "127.0.0.1"
advertised_port = 3478
ip = "127.0.0.1"
port = 0

[turn]
relay_port_range = "49152-65535"
realm = "loran.local"

[services.signer]
kek = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"

[control.admin_ui]
password_hash = "$argon2id$v=19$m=65536,t=2,p=1$YWN0cml4LXRlc3Qtc2FsdA$lfmjnm9CjoHQC9U79KOWXJkyWTUA7g5MhgG2DDPqe1E"

[recording]

"#,
            sqlite = data_dir.display(),
            pid = dir.join("loran.pid").display()
        ),
    )
    .expect("write valid config");

    config_path
}

fn write_warning_only_config(dir: &Path, file_name: &str) -> PathBuf {
    let data_dir = dir.join("data");
    fs::create_dir_all(&data_dir).expect("create data dir");

    let config_path = dir.join(file_name);
    fs::write(
        &config_path,
        format!(
            r#"
name = "loran-cli-warning-test"
enable = 16
env = "prod"
sqlite_path = "{sqlite}"
loran_shared_key = "0123456789abcdef0123456789abcdef"
location_tag = "local,test,cli-warning"
pid = "{pid}"

[bind]
[bind.http]
domain_name = "localhost"
advertised_ip = "127.0.0.1"
ip = "127.0.0.1"
port = 39998
cert = "/tmp/fake.crt"
key = "/tmp/fake.key"

[bind.ice]
advertised_ip = "127.0.0.1"
advertised_port = 3478
ip = "127.0.0.1"
port = 0

[monitoring]
allowed_ips = ["127.0.0.1/32"]

[turn]
relay_port_range = "49152-65535"
realm = "loran.local"

[services.signer]
kek = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"

[control.admin_ui]
password_hash = "$argon2id$v=19$m=65536,t=2,p=1$YWN0cml4LXRlc3Qtc2FsdA$lfmjnm9CjoHQC9U79KOWXJkyWTUA7g5MhgG2DDPqe1E"

[recording]

"#,
            sqlite = data_dir.display(),
            pid = dir.join("loran.pid").display()
        ),
    )
    .expect("write warning-only config");

    config_path
}

fn write_validation_error_config(dir: &Path, file_name: &str) -> PathBuf {
    let data_dir = dir.join("data");
    fs::create_dir_all(&data_dir).expect("create data dir");

    let config_path = dir.join(file_name);
    fs::write(
        &config_path,
        format!(
            r#"
name = "loran-cli-validation-error-test"
enable = 16
env = "dev"
sqlite_path = "{sqlite}"
loran_shared_key = "0123456789abcdef0123456789abcdef"
location_tag = "local,test,cli-validation-error"
pid = "{pid}"

[bind]
[bind.http]
domain_name = "localhost"
advertised_ip = "127.0.0.1"
ip = "127.0.0.1"
port = 39997

[bind.ice]
advertised_ip = "127.0.0.1"
advertised_port = 3478
ip = "127.0.0.1"
port = 0

[turn]
relay_port_range = "49152-65535"
realm = "loran.local"

# Intentionally no [services.signer] to trigger validation error for ENABLE_SIGNER.

[recording]

"#,
            sqlite = data_dir.display(),
            pid = dir.join("loran.pid").display()
        ),
    )
    .expect("write validation-error config");

    config_path
}

fn run_loran(args: &[&str], current_dir: Option<&Path>) -> Output {
    let mut cmd = Command::new(PathBuf::from(env!("CARGO_BIN_EXE_loran")));
    cmd.args(args);
    if let Some(dir) = current_dir {
        cmd.current_dir(dir);
    }
    cmd.output().expect("run loran command")
}

#[test]
fn loran_cli_reports_the_canonical_command_name() {
    let version = run_loran(&["--version"], None);
    assert!(version.status.success());
    assert_eq!(
        String::from_utf8_lossy(&version.stdout).trim(),
        format!("loran {}", env!("CARGO_PKG_VERSION"))
    );

    let help = run_loran(&["--help"], None);
    assert!(help.status.success());
    assert!(String::from_utf8_lossy(&help.stdout).contains("Usage: loran"));
}

#[test]
fn loran_test_command_accepts_explicit_valid_config() {
    let temp = tempfile::tempdir().expect("temp dir");
    let config_path = write_valid_config(temp.path(), "valid.toml");
    let output = run_loran(&["test", config_path.to_str().expect("utf8 path")], None);

    assert!(
        output.status.success(),
        "command should succeed, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn loran_test_command_finds_default_config_in_current_directory() {
    let temp = tempfile::tempdir().expect("temp dir");
    write_valid_config(temp.path(), "config.toml");
    let output = run_loran(&["test"], Some(temp.path()));

    assert!(
        output.status.success(),
        "command should succeed, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn loran_test_command_fails_for_missing_custom_config_path() {
    let temp = tempfile::tempdir().expect("temp dir");
    let missing_path = temp.path().join("missing.toml");
    let output = run_loran(&["test", missing_path.to_str().expect("utf8 path")], None);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(!output.status.success(), "command should fail");
    assert!(
        stderr.contains("Config file not found")
            || stderr.contains("configuration file")
            || stderr.contains("No configuration file found"),
        "unexpected stderr: {stderr}"
    );
}

#[test]
fn loran_test_command_fails_when_no_default_config_exists() {
    let temp = tempfile::tempdir().expect("temp dir");
    let output = run_loran(&["test"], Some(temp.path()));
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(!output.status.success(), "command should fail");
    assert!(
        stderr.contains("No configuration file found")
            || stderr.contains("Config file not found")
            || stderr.contains("configuration file")
            || stderr.contains("Permission denied"),
        "unexpected stderr: {stderr}"
    );
}

#[test]
fn loran_test_command_fails_for_invalid_config_content() {
    let temp = tempfile::tempdir().expect("temp dir");
    let bad_path = temp.path().join("bad.toml");
    fs::write(&bad_path, "name = \"broken\"\nenable = [\n").expect("write invalid toml");

    let output = run_loran(&["test", bad_path.to_str().expect("utf8 path")], None);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(!output.status.success(), "command should fail");
    assert!(
        stderr.contains("Configuration parse failed")
            || stderr.contains("parse")
            || stderr.contains("invalid"),
        "unexpected stderr: {stderr}"
    );
}

#[test]
fn loran_test_command_fails_for_validation_errors() {
    let temp = tempfile::tempdir().expect("temp dir");
    let config_path = write_validation_error_config(temp.path(), "validation-error.toml");
    let output = run_loran(&["test", config_path.to_str().expect("utf8 path")], None);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(!output.status.success(), "command should fail");
    assert!(
        stderr.contains("Configuration validation failed")
            || stderr.contains("ServiceValidation")
            || stderr.contains("services.signer"),
        "unexpected stderr: {stderr}"
    );
}

#[test]
fn loran_test_command_succeeds_with_warning_only_config() {
    let temp = tempfile::tempdir().expect("temp dir");
    let config_path = write_warning_only_config(temp.path(), "warning.toml");
    let output = run_loran(&["test", config_path.to_str().expect("utf8 path")], None);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "warning-only config should succeed, stderr: {stderr}"
    );
}

#[test]
fn loran_run_mode_fails_when_no_default_config_exists() {
    let temp = tempfile::tempdir().expect("temp dir");
    let output = run_loran(&[], Some(temp.path()));
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(!output.status.success(), "run mode should fail");
    assert!(
        stderr.contains("No configuration file found")
            || stderr.contains("Config file not found")
            || stderr.contains("configuration file")
            || stderr.contains("Permission denied"),
        "unexpected stderr: {stderr}"
    );
}

#[test]
fn loran_run_mode_fails_for_missing_custom_config_flag() {
    let temp = tempfile::tempdir().expect("temp dir");
    let missing_path = temp.path().join("missing-run.toml");
    let output = run_loran(
        &["--config", missing_path.to_str().expect("utf8 path")],
        None,
    );
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(!output.status.success(), "run mode should fail");
    assert!(
        stderr.contains("Config file not found")
            || stderr.contains("No configuration file found")
            || stderr.contains("configuration file"),
        "unexpected stderr: {stderr}"
    );
}

#[test]
fn loran_run_mode_fails_for_invalid_config_content() {
    let temp = tempfile::tempdir().expect("temp dir");
    let bad_path = temp.path().join("bad-run.toml");
    fs::write(&bad_path, "name = \"broken\"\nenable = [\n").expect("write invalid toml");
    let output = run_loran(&["--config", bad_path.to_str().expect("utf8 path")], None);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(!output.status.success(), "run mode should fail");
    assert!(
        stderr.contains("Configuration load failed")
            || stderr.contains("Configuration parse failed")
            || stderr.contains("parse")
            || stderr.contains("invalid"),
        "unexpected stderr: {stderr}"
    );
}

#[test]
fn loran_run_mode_fails_for_validation_errors() {
    let temp = tempfile::tempdir().expect("temp dir");
    let config_path = write_validation_error_config(temp.path(), "run-validation-error.toml");
    let output = run_loran(
        &["--config", config_path.to_str().expect("utf8 path")],
        None,
    );
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(!output.status.success(), "run mode should fail");
    assert!(
        stderr.contains("Configuration validation failed")
            || stderr.contains("services.signer")
            || stderr.contains("validation"),
        "unexpected stderr: {stderr}"
    );
}

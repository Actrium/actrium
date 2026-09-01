use super::*;

#[test]
fn test_create_service_filter() {
    let cmd = DiscoveryCommand::new(Some("user-*".to_string()), false, false);
    let filter = cmd.create_service_filter();

    assert!(filter.is_some());
    let filter = filter.unwrap();
    assert_eq!(filter.name_pattern, Some("user-*".to_string()));
}

#[test]
fn test_create_service_filter_none() {
    let cmd = DiscoveryCommand::new(None, false, false);
    let filter = cmd.create_service_filter();

    assert!(filter.is_none());
}

#[test]
fn test_required_components() {
    let cmd = DiscoveryCommand::default();
    let components = cmd.required_components();

    // Discovery requires only ServiceDiscovery + UserInterface up front.
    // ConfigManager and validators are obtained lazily when needed.
    assert!(components.contains(&ComponentType::ServiceDiscovery));
    assert!(components.contains(&ComponentType::UserInterface));
    assert!(!components.contains(&ComponentType::ConfigManager));
    assert!(!components.contains(&ComponentType::DependencyResolver));
    assert!(!components.contains(&ComponentType::NetworkValidator));
    assert!(!components.contains(&ComponentType::FingerprintValidator));
}

#[test]
fn export_proto_rejects_remote_path_traversal_without_writing_outside_root() {
    let dir = tempfile::tempdir().unwrap();
    let output_dir = prepare_export_directory(dir.path(), "EchoService").unwrap();
    let outside = dir.path().join("outside.proto");

    for name in [
        "../outside.proto",
        "..\\outside.proto",
        "/outside.proto",
        ".",
        "..",
    ] {
        let error = write_exported_proto(&output_dir, name, "malicious")
            .unwrap_err()
            .to_string();
        assert!(error.contains("not a safe path component"), "{error}");
    }
    assert!(!outside.exists());
}

#[cfg(unix)]
#[test]
fn export_proto_rejects_existing_symlink_destination() {
    use std::os::unix::fs::symlink;

    let dir = tempfile::tempdir().unwrap();
    let output_dir = prepare_export_directory(dir.path(), "EchoService").unwrap();
    let outside = dir.path().join("outside.proto");
    std::fs::write(&outside, "original").unwrap();
    symlink(&outside, output_dir.join("echo.proto")).unwrap();

    let error = write_exported_proto(&output_dir, "echo.proto", "replacement")
        .unwrap_err()
        .to_string();
    assert!(error.contains("symbolic link"), "{error}");
    assert_eq!(std::fs::read_to_string(outside).unwrap(), "original");
}

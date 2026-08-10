use super::{ENTRY_PLACEHOLDER_MARKER, ENTRY_SCAFFOLD_MARKER, RustGenerator};
use crate::commands::codegen::scaffold::{ScaffoldCatalog, ScaffoldMethod, ScaffoldService};
use crate::commands::codegen::{
    ActrGenMetadata, DispatchSpec, GenContext, LanguageGenerator, ProtoModel, SupportedLanguage,
    TypeRef, write_metadata,
};
use crate::templates::{
    DEFAULT_MANUFACTURER, ProjectTemplate, ProjectTemplateName, TemplateContext,
};
use actr_config::ConfigParser;
use std::path::PathBuf;
use tempfile::TempDir;

fn scaffold_service() -> ScaffoldService {
    ScaffoldService {
        name: "EmptyShell".to_string(),
        package: "demo.shell".to_string(),
        proto_file: PathBuf::from("bridge.proto"),
        handler_interface: Some("EmptyShellHandler".to_string()),
        workload_type: Some("EmptyShellWorkload".to_string()),
        dispatcher_type: Some("EmptyShellDispatcher".to_string()),
        client_type: None,
        dependency_alias: None,
        actr_type: None,
        methods: vec![ScaffoldMethod {
            name: "Ping".to_string(),
            snake_name: "ping".to_string(),
            input_type: "PingRequest".to_string(),
            output_type: "PingResponse".to_string(),
            route_key: "demo.shell.EmptyShell/Ping".to_string(),
            input_ref: TypeRef {
                proto_type: "demo.shell.PingRequest".to_string(),
                type_name: "PingRequest".to_string(),
                proto_package: "demo.shell".to_string(),
                proto_file: "bridge.proto".to_string(),
                generated_type: None,
            },
            output_ref: TypeRef {
                proto_type: "demo.shell.PingResponse".to_string(),
                type_name: "PingResponse".to_string(),
                proto_package: "demo.shell".to_string(),
                proto_file: "bridge.proto".to_string(),
                generated_type: None,
            },
        }],
    }
}

fn scaffold_catalog() -> ScaffoldCatalog {
    ScaffoldCatalog {
        local_services: vec![scaffold_service()],
        remote_services: vec![],
    }
}

#[test]
fn generated_entry_placeholder_is_overwritten_and_regeneration_is_idempotent() {
    let tmp = TempDir::new().unwrap();
    let output = tmp.path().join("src/generated");
    let path = tmp.path().join("src/lib.rs");

    let template = ProjectTemplate::new(ProjectTemplateName::Empty, SupportedLanguage::Rust);
    let context = TemplateContext::new(
        "empty-shell",
        "ws://localhost:8080",
        DEFAULT_MANUFACTURER,
        "empty-service",
        true,
    );
    template.generate(tmp.path(), &context).unwrap();

    let placeholder = std::fs::read_to_string(&path).unwrap();
    assert!(placeholder.contains(ENTRY_PLACEHOLDER_MARKER));

    let generator = RustGenerator;
    let scaffold = generator.generate_entry_scaffold_content(&scaffold_catalog());
    std::fs::create_dir_all(&output).unwrap();

    tokio_test::block_on(generator.generate_entry_scaffold(&scaffold_catalog(), &output, false))
        .unwrap();

    assert_eq!(std::fs::read_to_string(&path).unwrap(), scaffold);

    tokio_test::block_on(generator.generate_entry_scaffold(&scaffold_catalog(), &output, false))
        .unwrap();

    assert_eq!(std::fs::read_to_string(path).unwrap(), scaffold);
}

#[test]
fn legacy_generated_entry_placeholder_is_overwritten_with_full_scaffold() {
    let tmp = TempDir::new().unwrap();
    let output = tmp.path().join("src/generated");
    let path = tmp.path().join("src/lib.rs");

    let template = ProjectTemplate::new(ProjectTemplateName::Empty, SupportedLanguage::Rust);
    let context = TemplateContext::new(
        "legacy-empty-shell",
        "ws://localhost:8080",
        DEFAULT_MANUFACTURER,
        "empty-service",
        true,
    );
    template.generate(tmp.path(), &context).unwrap();

    let placeholder = std::fs::read_to_string(&path).unwrap().replacen(
        ENTRY_PLACEHOLDER_MARKER,
        ENTRY_SCAFFOLD_MARKER,
        1,
    );
    std::fs::write(&path, placeholder).unwrap();

    let generator = RustGenerator;
    let scaffold = generator.generate_entry_scaffold_content(&scaffold_catalog());
    tokio_test::block_on(generator.generate_entry_scaffold(&scaffold_catalog(), &output, false))
        .unwrap();

    assert_eq!(std::fs::read_to_string(path).unwrap(), scaffold);
}

#[test]
fn user_modified_entry_placeholder_is_preserved() {
    let tmp = TempDir::new().unwrap();
    let output = tmp.path().join("src/generated");
    let path = tmp.path().join("src/lib.rs");

    let template = ProjectTemplate::new(ProjectTemplateName::Empty, SupportedLanguage::Rust);
    let context = TemplateContext::new(
        "custom-empty-shell",
        "ws://localhost:8080",
        DEFAULT_MANUFACTURER,
        "empty-service",
        true,
    );
    template.generate(tmp.path(), &context).unwrap();

    let mut user_content = std::fs::read_to_string(&path).unwrap();
    user_content.push_str("\npub fn user_customization() {}\n");
    std::fs::write(&path, &user_content).unwrap();

    tokio_test::block_on(RustGenerator.generate_entry_scaffold(
        &scaffold_catalog(),
        &output,
        false,
    ))
    .unwrap();

    assert_eq!(std::fs::read_to_string(path).unwrap(), user_content);
}

#[test]
fn modified_generated_entry_scaffold_is_preserved() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("lib.rs");
    let generator = RustGenerator;
    let scaffold = generator.generate_entry_scaffold_content(&scaffold_catalog());
    let modified = format!("{scaffold}\n// User customization.\n");
    std::fs::write(&path, modified).unwrap();

    assert!(
        !generator
            .should_overwrite_entry_scaffold(&path, &scaffold)
            .unwrap()
    );
}

#[test]
fn user_modified_entry_without_actr_marker_is_preserved() {
    let tmp = TempDir::new().unwrap();
    let output = tmp.path().join("src/generated");
    let path = tmp.path().join("src/lib.rs");
    std::fs::create_dir_all(&output).unwrap();

    let generator = RustGenerator;
    let user_content = "pub mod generated;\n\npub fn user_customization() {}\n";
    std::fs::write(&path, user_content).unwrap();

    tokio_test::block_on(generator.generate_entry_scaffold(&scaffold_catalog(), &output, false))
        .unwrap();

    assert_eq!(std::fs::read_to_string(path).unwrap(), user_content);
}

#[test]
fn scaffold_empty_service_uses_service_metadata_and_writes_entry() {
    let tmp = TempDir::new().unwrap();
    let src_dir = tmp.path().join("src");
    let proto_root = tmp.path().join("protos");
    std::fs::create_dir_all(src_dir.join("generated")).unwrap();
    std::fs::create_dir_all(&proto_root).unwrap();

    let proto_file = proto_root.join("bridge.proto");
    std::fs::write(
        &proto_file,
        "syntax = \"proto3\";\npackage demo.shell;\nservice EmptyShell {}\n",
    )
    .unwrap();

    let config_path = tmp.path().join("manifest.toml");
    std::fs::write(
        &config_path,
        r#"edition = 1
exports = []

[package]
name = "Demo"
manufacturer = "acme"
version = "0.1.0"

[system.signaling]
url = "ws://127.0.0.1:8080"

[system.ais_endpoint]
url = "http://127.0.0.1:8080/ais"

[system.deployment]
realm_id = 1001
"#,
    )
    .unwrap();

    let config = ConfigParser::from_manifest_file(&config_path).unwrap();
    let proto_files = vec![proto_file];
    let proto_model = ProtoModel::parse(&proto_files, &proto_root, &config).unwrap();
    let context = GenContext {
        proto_files,
        proto_model,
        input_path: proto_root,
        output: src_dir.join("generated"),
        config_path,
        config,
        no_scaffold: false,
        overwrite_user_code: false,
        no_format: false,
        debug: false,
        skip_validation: false,
        dispatch_spec: DispatchSpec::default(),
        dispatch_spec_path: PathBuf::new(),
        dispatch_spec_hash: String::new(),
    };
    let metadata =
        ActrGenMetadata::from_proto_model(SupportedLanguage::Rust, &context.proto_model).unwrap();
    write_metadata(&context.output, &metadata).unwrap();
    let catalog = ScaffoldCatalog::from_metadata(&metadata);

    tokio_test::block_on(RustGenerator.generate_scaffold(&context, &catalog)).unwrap();

    let handler_path = src_dir.join("actr_handler.rs");
    assert!(
        handler_path.exists(),
        "unified handler file should be generated"
    );
    assert!(
        !src_dir.join("bridge_service.rs").exists(),
        "proto file stem should not drive the scaffold handler path"
    );

    let handler = std::fs::read_to_string(&handler_path).unwrap();
    assert!(handler.contains("use crate::generated::actr_dispatch::ActrDispatchHandler;"));
    assert!(handler.contains("pub struct ActrHandler;"));
    assert!(handler.contains("impl ActrDispatchHandler for ActrHandler"));

    let lib = std::fs::read_to_string(src_dir.join("lib.rs")).unwrap();
    assert!(lib.contains("pub mod generated;"));
    assert!(lib.contains("pub mod actr_handler;"));
    assert!(lib.contains("use generated::actr_dispatch::ActrWorkload;"));
    assert!(lib.contains("pub use crate::actr_handler::ActrHandler;"));
    assert!(lib.contains("entry!("));
    assert!(lib.contains("ActrWorkload<ActrHandler>"));

    std::fs::write(&handler_path, "pub struct UserImplemented;\n").unwrap();
    tokio_test::block_on(RustGenerator.generate_scaffold(&context, &catalog)).unwrap();
    assert_eq!(
        std::fs::read_to_string(&handler_path).unwrap(),
        "pub struct UserImplemented;\n",
        "implemented handler files must be preserved without overwrite_user_code"
    );
}

#[test]
fn unified_remote_scaffold_uses_unit_handler_without_actr_type() {
    let tmp = TempDir::new().unwrap();
    let proto_root = tmp.path().join("protos");
    let remote_dir = proto_root.join("remote/echo");
    std::fs::create_dir_all(&remote_dir).unwrap();

    let remote_proto = remote_dir.join("echo.proto");
    std::fs::write(
        &remote_proto,
        "syntax = \"proto3\";\npackage demo;\nservice EchoService {}\n",
    )
    .unwrap();

    let config_path = tmp.path().join("manifest.toml");
    std::fs::write(
        &config_path,
        r#"edition = 1
exports = []

[package]
name = "Demo"
manufacturer = "acme"
version = "0.1.0"

[dependencies]
echo = { actr_type = "remote:EchoService:0.1.0" }

[system.signaling]
url = "ws://127.0.0.1:8080"

[system.ais_endpoint]
url = "http://127.0.0.1:8080/ais"

[system.deployment]
realm_id = 1001
"#,
    )
    .unwrap();

    let config = ConfigParser::from_manifest_file(&config_path).unwrap();
    let proto_files = vec![remote_proto];
    let proto_model = ProtoModel::parse(&proto_files, &proto_root, &config).unwrap();

    let metadata =
        ActrGenMetadata::from_proto_model(SupportedLanguage::Rust, &proto_model).unwrap();
    let catalog = ScaffoldCatalog::from_metadata(&metadata);
    let entry = RustGenerator.generate_entry_scaffold_content(&catalog);
    assert!(entry.contains("ActrWorkload<()>"));
    assert!(entry.contains("ActrWorkload::new(())"));
    assert!(!entry.contains("ActrType"));
}

#[test]
fn pure_helpers_produce_correct_output() {
    let svc = scaffold_service();
    assert_eq!(
        super::rust_scaffold_qualified_type(&svc.methods[0].input_ref, "Fallback"),
        "crate::generated::demo_shell::PingRequest"
    );
    // is_default_cargo_lib_rs returns false for non-default lib.rs.
    assert!(!super::is_default_cargo_lib_rs("custom content"));
}

#[test]
fn qualified_scaffold_types_use_nested_parent_modules() {
    let mut svc = scaffold_service();
    svc.methods[0].input_ref = TypeRef {
        proto_type: "ask.Outer.InnerRequest".to_string(),
        type_name: "InnerRequest".to_string(),
        proto_package: "ask".to_string(),
        proto_file: "remote/ask/ask.proto".to_string(),
        generated_type: None,
    };
    svc.methods[0].output_ref = TypeRef {
        proto_type: "ask.Outer.InnerResponse".to_string(),
        type_name: "InnerResponse".to_string(),
        proto_package: "ask".to_string(),
        proto_file: "remote/ask/ask.proto".to_string(),
        generated_type: None,
    };

    assert_eq!(
        super::rust_scaffold_qualified_type(&svc.methods[0].input_ref, "Fallback"),
        "crate::generated::ask::outer::InnerRequest"
    );
    assert_eq!(
        super::rust_scaffold_qualified_type(&svc.methods[0].output_ref, "Fallback"),
        "crate::generated::ask::outer::InnerResponse"
    );
}

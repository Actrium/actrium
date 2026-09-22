use super::{ENTRY_PLACEHOLDER_MARKER, ENTRY_SCAFFOLD_MARKER, RustGenerator};
use crate::commands::codegen::scaffold::{ScaffoldCatalog, ScaffoldMethod, ScaffoldService};
use crate::commands::codegen::{
    ActrGenMetadata, DispatchSpec, GenContext, LanguageGenerator, ProtoModel, SupportedLanguage,
    TypeRef, write_metadata,
};
use crate::templates::{DEFAULT_PUBLISHER, ProjectTemplate, ProjectTemplateName, TemplateContext};
use actr_config::ConfigParser;
use std::path::PathBuf;
use tempfile::TempDir;

fn scaffold_service() -> ScaffoldService {
    ScaffoldService {
        name: "EmptyShell".to_string(),
        package: "demo.shell".to_string(),
        proto_file: PathBuf::from("bridge.proto"),
        handler_interface: Some("EmptyShellHandler".to_string()),
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
            reply_mode: crate::commands::codegen::ReplyMode::Return,
        }],
    }
}

fn scaffold_catalog() -> ScaffoldCatalog {
    ScaffoldCatalog {
        local_services: vec![scaffold_service()],
        remote_services: vec![],
    }
}

fn empty_scaffold_service(name: &str) -> ScaffoldService {
    let mut service = scaffold_service();
    service.name = name.to_string();
    service.handler_interface = Some(format!("{name}Handler"));
    service.methods.clear();
    service
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
        DEFAULT_PUBLISHER,
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
        DEFAULT_PUBLISHER,
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
        DEFAULT_PUBLISHER,
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
publisher = "acme"
version = "0.1.0"

[system.signaling]
url = "ws://127.0.0.1:8080"

[system.issuer_endpoint]
url = "http://127.0.0.1:8080/issuer"

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

    // The unified business handler scaffold is gone: the aggregate
    // ActrHandler now lives in generated code, and business logic returns to
    // per-service src/<service>.rs files.
    assert!(
        !src_dir.join("actr_handler.rs").exists(),
        "no user-facing actr_handler.rs scaffold may be generated"
    );

    let service_path = src_dir.join("empty_shell.rs");
    assert!(
        service_path.exists(),
        "per-service business scaffold should be generated"
    );
    let service = std::fs::read_to_string(&service_path).unwrap();
    assert!(
        service.contains("use crate::generated::bridge_actor::EmptyShellHandler;"),
        "business code implements the per-service Handler trait:\n{service}"
    );
    assert!(service.contains("pub struct EmptyShellImpl;"));
    assert!(service.contains("impl EmptyShellHandler for EmptyShellImpl"));
    assert!(
        !service.contains("ActrDispatchHandler"),
        "business code must not touch the sealed dispatch trait:\n{service}"
    );

    let lib = std::fs::read_to_string(src_dir.join("lib.rs")).unwrap();
    assert!(lib.contains("pub mod generated;"));
    assert!(lib.contains("use generated::actr_dispatch::ActrWorkload;"));
    assert!(lib.contains("entry!("));
    assert!(lib.contains("ActrWorkload<()>"));
    assert!(lib.contains("ActrWorkload::new(())"));
    assert!(
        !lib.contains("actr_handler::ActrHandler"),
        "empty-only services must not require generated actr_handler.rs:\n{lib}"
    );
    assert!(
        !lib.contains("EmptyShellImpl"),
        "empty-only services must not be composed into the entry:\n{lib}"
    );

    // Existing business files are user code — never overwritten implicitly.
    std::fs::write(&service_path, "pub struct UserImplemented;\n").unwrap();
    tokio_test::block_on(RustGenerator.generate_scaffold(&context, &catalog)).unwrap();
    assert_eq!(
        std::fs::read_to_string(&service_path).unwrap(),
        "pub struct UserImplemented;\n",
        "implemented service files must be preserved without overwrite_user_code"
    );

    // Existing lib.rs entries are user code too — never rewritten.
    let lib_path = src_dir.join("lib.rs");
    std::fs::write(&lib_path, "// user lib\n").unwrap();
    tokio_test::block_on(RustGenerator.generate_scaffold(&context, &catalog)).unwrap();
    assert_eq!(
        std::fs::read_to_string(&lib_path).unwrap(),
        "// user lib\n",
        "existing lib.rs must be preserved"
    );
}

#[test]
fn entry_scaffold_uses_unit_handler_for_empty_only_local_services() {
    let catalog = ScaffoldCatalog {
        local_services: vec![empty_scaffold_service("EmptyShell")],
        remote_services: vec![],
    };

    let entry = RustGenerator.generate_entry_scaffold_content(&catalog);
    assert!(entry.contains("ActrWorkload<()>"), "entry:\n{entry}");
    assert!(entry.contains("ActrWorkload::new(())"), "entry:\n{entry}");
    assert!(
        !entry.contains("actr_handler::ActrHandler"),
        "entry must not import an adapter the plugin does not generate:\n{entry}"
    );
    assert!(
        !entry.contains("EmptyShellImpl"),
        "empty service must not be a handler generic or constructor arg:\n{entry}"
    );
}

#[test]
fn entry_scaffold_ignores_empty_local_services_when_composing_actr_handler() {
    let mut nonempty = scaffold_service();
    nonempty.name = "EchoService".to_string();
    nonempty.handler_interface = Some("EchoServiceHandler".to_string());
    let catalog = ScaffoldCatalog {
        local_services: vec![empty_scaffold_service("EmptyShell"), nonempty],
        remote_services: vec![],
    };

    let entry = RustGenerator.generate_entry_scaffold_content(&catalog);
    assert!(
        entry.contains("ActrWorkload<ActrHandler<EchoServiceImpl>>"),
        "entry:\n{entry}"
    );
    assert!(
        entry.contains("ActrWorkload::new(ActrHandler::new(EchoServiceImpl::new()))"),
        "entry:\n{entry}"
    );
    assert!(
        !entry.contains("EmptyShellImpl"),
        "empty service must not affect ActrHandler generics or constructor args:\n{entry}"
    );
}

#[test]
fn multi_service_entry_composes_generated_actr_handler() {
    let mut ask = scaffold_service();
    ask.name = "AskService".to_string();
    ask.package = "ask".to_string();
    ask.proto_file = PathBuf::from("local/ask.proto");
    ask.handler_interface = Some("AskServiceHandler".to_string());
    let mut chat = scaffold_service();
    chat.name = "ChatService".to_string();
    chat.package = "chat".to_string();
    chat.proto_file = PathBuf::from("local/chat.proto");
    chat.handler_interface = Some("ChatServiceHandler".to_string());

    let catalog = ScaffoldCatalog {
        local_services: vec![ask, chat],
        remote_services: vec![],
    };

    let entry = RustGenerator.generate_entry_scaffold_content(&catalog);
    assert!(entry.contains("pub mod ask_service;"), "entry: \n{entry}");
    assert!(entry.contains("pub mod chat_service;"), "entry: \n{entry}");
    assert!(
        entry.contains("use ask_service::AskServiceImpl;"),
        "entry: \n{entry}"
    );
    assert!(
        entry.contains("use chat_service::ChatServiceImpl;"),
        "entry: \n{entry}"
    );
    assert!(
        entry.contains("ActrWorkload<ActrHandler<AskServiceImpl, ChatServiceImpl>>"),
        "entry: \n{entry}"
    );
    assert!(
        entry.contains(
            "ActrWorkload::new(ActrHandler::new(AskServiceImpl::new(), ChatServiceImpl::new()))"
        ),
        "entry: \n{entry}"
    );

    // Per-service scaffolds keep the protobuf RPC names (both services
    // define Ping here) and reference their own actor module.
    let ask_scaffold = RustGenerator.generate_service_scaffold_content(&catalog.local_services[0]);
    let chat_scaffold = RustGenerator.generate_service_scaffold_content(&catalog.local_services[1]);
    assert!(ask_scaffold.contains("use crate::generated::ask_actor::AskServiceHandler;"));
    assert!(chat_scaffold.contains("use crate::generated::chat_actor::ChatServiceHandler;"));
    assert!(ask_scaffold.contains("async fn ping<C: Context>"));
    assert!(chat_scaffold.contains("async fn ping<C: Context>"));
}

#[test]
fn explicit_method_scaffold_takes_reply_handle() {
    let mut service = scaffold_service();
    service.methods.push(ScaffoldMethod {
        name: "Transcode".to_string(),
        snake_name: "transcode".to_string(),
        input_type: "PingRequest".to_string(),
        output_type: "PingResponse".to_string(),
        route_key: "demo.shell.EmptyShell.Transcode".to_string(),
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
        reply_mode: crate::commands::codegen::ReplyMode::Explicit,
    });

    let scaffold = RustGenerator.generate_service_scaffold_content(&service);
    // RETURN method keeps the 0.6 plain-return signature.
    assert!(
        scaffold.contains(") -> ActorResult<crate::generated::demo_shell::PingResponse>"),
        "RETURN signature:\n{scaffold}"
    );
    // EXPLICIT method receives a Reply<T> and returns ActorResult<()>.
    assert!(
        scaffold.contains("reply: Reply<crate::generated::demo_shell::PingResponse>,"),
        "EXPLICIT signature:\n{scaffold}"
    );
    assert!(
        scaffold.contains(") -> ActorResult<()>"),
        "EXPLICIT signature:\n{scaffold}"
    );
    assert!(
        scaffold.contains("use actr_framework::Reply;"),
        "Reply import:\n{scaffold}"
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
publisher = "acme"
version = "0.1.0"

[dependencies]
echo = { actr_type = "remote:EchoService:0.1.0" }

[system.signaling]
url = "ws://127.0.0.1:8080"

[system.issuer_endpoint]
url = "http://127.0.0.1:8080/issuer"

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
fn stale_plugin_artifacts_are_removed_absent_from_run_metadata() {
    let tmp = TempDir::new().unwrap();
    let output = tmp.path().join("generated");
    std::fs::create_dir_all(&output).unwrap();

    // Simulate a previous local-services run: adapter + actor file, plus the
    // artifacts this (remote-only) run also owns.
    std::fs::write(output.join("actr_handler.rs"), "// stale adapter").unwrap();
    std::fs::write(output.join("chat_actor.rs"), "// stale actor").unwrap();
    std::fs::write(output.join("ask_client.rs"), "// current client").unwrap();
    std::fs::write(output.join("actr_dispatch.rs"), "// current dispatch").unwrap();
    let metadata = ActrGenMetadata {
        plugin_version: "test".into(),
        language: "rust".into(),
        generated_files: vec!["ask_client.rs".into(), "actr_dispatch.rs".into()],
        ..Default::default()
    };
    write_metadata(&output, &metadata).unwrap();

    RustGenerator
        .remove_stale_plugin_artifacts(&output)
        .unwrap();

    assert!(
        !output.join("actr_handler.rs").exists(),
        "stale adapter must be removed"
    );
    assert!(!output.join("chat_actor.rs").exists());
    assert!(output.join("ask_client.rs").exists());
    assert!(output.join("actr_dispatch.rs").exists());
}

#[test]
fn current_plugin_artifacts_survive_cleanup() {
    let tmp = TempDir::new().unwrap();
    let output = tmp.path().join("generated");
    std::fs::create_dir_all(&output).unwrap();

    std::fs::write(output.join("actr_handler.rs"), "// current adapter").unwrap();
    // A prost message module is not plugin-owned and must never be touched.
    std::fs::write(output.join("ask.rs"), "// prost module").unwrap();
    let metadata = ActrGenMetadata {
        plugin_version: "test".into(),
        language: "rust".into(),
        generated_files: vec![
            "actr_dispatch.rs".into(),
            "actr_handler.rs".into(),
            "ask_actor.rs".into(),
        ],
        ..Default::default()
    };
    write_metadata(&output, &metadata).unwrap();

    RustGenerator
        .remove_stale_plugin_artifacts(&output)
        .unwrap();

    assert!(output.join("actr_handler.rs").exists());
    assert!(output.join("ask.rs").exists());
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

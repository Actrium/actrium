use super::*;
use tempfile::TempDir;

#[test]
fn stale_workload_cleanup_requires_the_typescript_generator_banner() {
    let tmp = TempDir::new().unwrap();
    let output = tmp.path().join("generated");
    let workload = output.join("echo_workload.ts");
    let legacy_modules = std::collections::BTreeSet::from(["echo_workload".to_string()]);

    std::fs::create_dir_all(&output).unwrap();
    std::fs::write(&workload, "// user-owned module\n").unwrap();
    super::remove_stale_legacy_workloads(&output, &legacy_modules).unwrap();
    assert!(workload.exists(), "same-named user module must be retained");

    std::fs::write(
        &workload,
        format!("// DO NOT EDIT.\n{LEGACY_TYPESCRIPT_WORKLOAD_HEADER}\n"),
    )
    .unwrap();
    super::remove_stale_legacy_workloads(&output, &legacy_modules).unwrap();
    assert!(
        !workload.exists(),
        "legacy generated module must be removed"
    );
}

#[test]
fn inspects_generated_client_exports() {
    let generator = TypeScriptGenerator;
    let temp_dir = TempDir::new().unwrap();
    let path = temp_dir.path().join("echo_client.ts");
    std::fs::write(
        &path,
        r#"
            export const EchoRequest = {
                routeKey: "demo.echo.Echo",
            } as const;
            "#,
    )
    .unwrap();

    let api = generator.inspect_generated_client_api(&path).unwrap();
    assert!(api.exported_consts.contains("EchoRequest"));
}

#[test]
fn implemented_marker_prevents_overwrite() {
    let generator = TypeScriptGenerator;
    let temp_dir = TempDir::new().unwrap();
    let path = temp_dir.path().join("actr_service.ts");
    std::fs::write(
            &path,
            format!(
                "{IMPLEMENTED_MARKER}\n{UNIMPLEMENTED_MARKER}\nexport default defineWorkload({{ dispatch() {{ throw new Error('custom'); }} }});\n"
            ),
        )
        .unwrap();
    assert!(!generator.should_overwrite_scaffold(&path).unwrap());
}

#[test]
fn recognizes_minimal_unimplemented_scaffold() {
    let generator = TypeScriptGenerator;
    let temp_dir = TempDir::new().unwrap();
    let path = temp_dir.path().join("actr_service.ts");
    std::fs::write(
            &path,
            format!(
                "{UNIMPLEMENTED_MARKER}\n{SCAFFOLD_HINT}\nexport default defineWorkload({{ dispatch() {{ throw new Error('TODO'); }} }});\n"
            ),
        )
        .unwrap();
    assert!(generator.should_overwrite_scaffold(&path).unwrap());
}

#[test]
fn unimplemented_marker_with_scaffold_hint_is_overwritten() {
    let generator = TypeScriptGenerator;
    let temp_dir = TempDir::new().unwrap();
    let path = temp_dir.path().join("actr_service.ts");
    std::fs::write(
        &path,
        format!("{UNIMPLEMENTED_MARKER}\n{SCAFFOLD_HINT}\nconsole.log('custom quick-start');\n"),
    )
    .unwrap();
    assert!(generator.should_overwrite_scaffold(&path).unwrap());

    std::fs::write(&path, "console.log('user code');\n").unwrap();
    assert!(!generator.should_overwrite_scaffold(&path).unwrap());
}

#[test]
fn does_not_treat_echo_templates_as_generated_scaffold() {
    let generator = TypeScriptGenerator;
    let temp_dir = TempDir::new().unwrap();
    let path = temp_dir.path().join("actr_service.ts");

    std::fs::write(
            &path,
            format!(
                "{IMPLEMENTED_MARKER}\nexport default defineWorkload({{ dispatch() {{ throw new Error('template'); }} }});\n"
            ),
        )
        .unwrap();
    assert!(!generator.should_overwrite_scaffold(&path).unwrap());

    std::fs::write(
            &path,
            format!(
                "{IMPLEMENTED_MARKER}\nimport {{ ActrNode }} from '@actrium/actr';\nconsole.log('template');\n"
            ),
        )
        .unwrap();
    assert!(!generator.should_overwrite_scaffold(&path).unwrap());
}

#[test]
fn generates_scaffold_with_local_and_remote_sections() {
    let generator = TypeScriptGenerator;
    let scaffold = generator.generate_scaffold_content(&[
        BoundMethodInfo {
            generated_client_import: "./generated/echo_client".to_string(),
            service_name: "EchoService".to_string(),
            method_name: "Echo".to_string(),
            input_type: "EchoRequest".to_string(),
            output_type: "EchoResponse".to_string(),
            input_type_short: "EchoRequest".to_string(),
            output_type_short: "EchoResponse".to_string(),
            input_pb_import: "./generated/echo_pb.js".to_string(),
            output_pb_import: "./generated/echo_pb.js".to_string(),
            request_companion: Some("EchoRequest".to_string()),
            is_local: true,
            reply_mode: ReplyMode::Return,
        },
        BoundMethodInfo {
            generated_client_import: "./generated/demo/remote_client".to_string(),
            service_name: "RemoteService".to_string(),
            method_name: "Ping".to_string(),
            input_type: "PingRequest".to_string(),
            output_type: "PingResponse".to_string(),
            input_type_short: "PingRequest".to_string(),
            output_type_short: "PingResponse".to_string(),
            input_pb_import: "./generated/demo/remote_pb.js".to_string(),
            output_pb_import: "./generated/demo/remote_pb.js".to_string(),
            request_companion: Some("PingRequest".to_string()),
            is_local: false,
            reply_mode: ReplyMode::Return,
        },
    ]);

    assert!(
        scaffold.contains("import { ActrContext, defineWorkload } from '@actrium/actr-workload';")
    );
    assert!(scaffold.contains("export default defineWorkload({"));
    assert!(
        scaffold.contains("const { actrDispatch } = await import('./generated/actr_dispatch.js');")
    );
    assert!(scaffold.contains("class ActrHandler {"));
    assert!(scaffold.contains("const handler = new ActrHandler();"));
    assert!(scaffold.contains("const actrCtx = new ActrContext(ctx!);"));
    assert!(scaffold.contains("actrDispatch(actrCtx, handler, envelope.routeKey ?? envelope.method, envelope.payload ?? new Uint8Array(), envelope.timeoutMs)"));
    assert!(!scaffold.contains("Implement this workload with @actrium/actr-workload"));
    assert!(!scaffold.contains("// - EchoService.Echo (EchoRequest -> EchoResponse)"));
    assert!(scaffold.contains("Remote RPC quick-start examples"));
    assert!(scaffold.contains("PingRequest.encode"));
    assert!(scaffold.contains("PingRequest.routeKey"));
    assert!(scaffold.contains("PingRequest.response.decode"));
    assert!(scaffold.contains(UNIMPLEMENTED_MARKER));
}

#[test]
fn explicit_scaffold_handler_is_async() {
    let generator = TypeScriptGenerator;
    let scaffold = generator.generate_scaffold_content(&[BoundMethodInfo {
        generated_client_import: "./generated/echo_client".to_string(),
        service_name: "EchoService".to_string(),
        method_name: "Echo".to_string(),
        input_type: "EchoRequest".to_string(),
        output_type: "EchoResponse".to_string(),
        input_type_short: "EchoRequest".to_string(),
        output_type_short: "EchoResponse".to_string(),
        input_pb_import: "./generated/echo_pb.js".to_string(),
        output_pb_import: "./generated/echo_pb.js".to_string(),
        request_companion: Some("EchoRequest".to_string()),
        is_local: true,
        reply_mode: ReplyMode::Explicit,
    }]);

    assert!(
        scaffold.contains("async echoServiceEcho(_req: EchoRequest, _ctx: ActrContext, reply: Reply<EchoResponse>): Promise<void> {")
    );
    assert!(scaffold.contains("await reply.send(create(EchoResponseSchema, {}));"));
}

#[test]
fn user_scaffold_imports_imported_types_from_owner_module() {
    // The user-facing `actr_service.ts` scaffold must import an imported RPC
    // type from its declaring `_pb.js` module (owner-resolved), not from the
    // local service's proto module.
    let generator = TypeScriptGenerator;
    let scaffold = generator.generate_scaffold_content(&[BoundMethodInfo {
        generated_client_import: String::new(),
        service_name: "DataStreamAppService".to_string(),
        method_name: "ContinuePromptResultStreams".to_string(),
        input_type: "ContinuePromptResultStreamsRequest".to_string(),
        output_type: "ContinuePromptResultStreamsResponse".to_string(),
        input_type_short: "ContinuePromptResultStreamsRequest".to_string(),
        output_type_short: "ContinuePromptResultStreamsResponse".to_string(),
        input_pb_import: "./generated/ask-service/ask_pb.js".to_string(),
        output_pb_import: "./generated/ask-service/ask_pb.js".to_string(),
        request_companion: None,
        is_local: true,
        reply_mode: ReplyMode::Return,
    }]);

    assert!(
        scaffold.contains("from './generated/ask-service/ask_pb.js'"),
        "expected owner module import in user scaffold, got:\n{scaffold}"
    );
    assert!(
        !scaffold.contains("from './generated/data_stream_app_pb.js'"),
        "user scaffold must not pin imported types to the local service module:\n{scaffold}"
    );
}

#[test]
fn user_scaffold_imports_nested_local_types_from_owner_module() {
    assert_eq!(
        scaffold_proto_import_for("local/foo/bar/payload.proto"),
        "./generated/foo/bar/payload_pb.js"
    );
    assert_eq!(
        scaffold_proto_import_for("local/payload.proto"),
        "./generated/payload_pb.js"
    );

    let generator = TypeScriptGenerator;
    let scaffold = generator.generate_scaffold_content(&[BoundMethodInfo {
        generated_client_import: String::new(),
        service_name: "AppService".to_string(),
        method_name: "UsePayload".to_string(),
        input_type: "PayloadRequest".to_string(),
        output_type: "PayloadResponse".to_string(),
        input_type_short: "PayloadRequest".to_string(),
        output_type_short: "PayloadResponse".to_string(),
        input_pb_import: scaffold_proto_import_for("local/foo/bar/payload.proto"),
        output_pb_import: scaffold_proto_import_for("local/foo/bar/payload.proto"),
        request_companion: None,
        is_local: true,
        reply_mode: ReplyMode::Return,
    }]);

    assert!(
        scaffold.contains("from './generated/foo/bar/payload_pb.js'"),
        "expected nested owner module import in user scaffold, got:\n{scaffold}"
    );
    assert!(
        !scaffold.contains("from './generated/payload_pb.js'"),
        "user scaffold must not flatten nested local owner modules:\n{scaffold}"
    );
}

#[test]
fn unified_handler_name_preserves_acronym_boundaries() {
    assert_eq!(
        typescript_dispatch_handler_method_name("HTTPService", "ParseURL"),
        "httpServiceParseUrl"
    );
}

#[test]
fn typescript_plugin_extract_command_uses_destination_flag() {
    // Regression guard: `unzip` selects its destination directory with `-d`.
    // `-C` controls case-insensitive member matching, not the destination; a
    // `-C` here makes the extract path a member pattern and breaks extraction.
    let archive = Path::new("/tmp/actr-plugin.zip");
    let extract_dir = Path::new("/tmp/actr-extract");
    let args = unzip_extract_args(archive, extract_dir);

    let arg_strs: Vec<&str> = args.iter().filter_map(|a| a.to_str()).collect();

    assert!(
        arg_strs.contains(&"-d"),
        "unzip must select the destination with -d, got: {arg_strs:?}"
    );
    assert!(
        !arg_strs.contains(&"-C"),
        "unzip -C controls case-insensitive matching, not the destination, got: {arg_strs:?}"
    );
    assert!(arg_strs.contains(&"/tmp/actr-plugin.zip"));
    assert!(arg_strs.contains(&"/tmp/actr-extract"));
}

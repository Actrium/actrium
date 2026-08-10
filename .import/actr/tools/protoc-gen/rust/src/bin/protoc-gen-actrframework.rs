use anyhow::{Context as _, Result, anyhow};
use bytes::Bytes;
use heck::ToSnakeCase;
use prost::Message;
use prost_types::{
    DescriptorProto, FileDescriptorProto, ServiceDescriptorProto,
    compiler::{CodeGeneratorRequest, CodeGeneratorResponse, code_generator_response::File},
};
use serde::Serialize;
use std::collections::{BTreeSet, HashMap};
use std::io::{self, Read, Write};

use actr_framework_protoc_codegen::{
    DispatchSpec, GeneratorRole, ModernGenerator, RemoteServiceInfo, UnifiedDispatcherGenerator,
};
use actr_protocol::{PackageName, ServiceName};

/// Type-owner entry: maps a fully-qualified proto message name to the proto
/// package that declares it.
#[derive(Debug, Clone)]
struct TypeOwner {
    proto_package: String,
    proto_file: String,
    type_name: String,
}

#[derive(Serialize)]
struct ActrGenMetadata {
    schema_version: String,
    plugin_version: String,
    language: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    dispatch_spec_hash: Option<String>,
    generated_files: Vec<String>,
    generated_local_routes: Vec<String>,
    generated_remote_routes: Vec<String>,
    local_services: Vec<LocalServiceMetadata>,
    remote_services: Vec<RemoteServiceMetadata>,
}

#[derive(Serialize)]
struct LocalServiceMetadata {
    name: String,
    package: String,
    proto_file: String,
    handler_interface: String,
    workload_type: String,
    dispatcher_type: String,
    methods: Vec<MethodMetadata>,
}

#[derive(Serialize)]
struct RemoteServiceMetadata {
    name: String,
    package: String,
    proto_file: String,
    /// Manifest dependency alias used by `DispatchSpec` remote routes to
    /// resolve this service's `ActrType` at runtime via
    /// `Context::resolve_dependency`. The unified dispatcher reads this
    /// alias from the DispatchSpec, not from an embedded `ActrType` string.
    dependency_alias: String,
    client_type: String,
    methods: Vec<MethodMetadata>,
}

#[derive(Serialize)]
struct MethodMetadata {
    name: String,
    snake_name: String,
    route_key: String,
    input_ref: TypeRef,
    output_ref: TypeRef,
}

#[derive(Serialize)]
struct TypeRef {
    proto_type: String,
    type_name: String,
    proto_package: String,
    proto_file: String,
}

fn main() -> Result<()> {
    // Support --version and --help arguments
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 {
        match args[1].as_str() {
            "--version" | "-V" => {
                println!("protoc-gen-actrframework {}", env!("CARGO_PKG_VERSION"));
                println!(
                    "actr-framework-protoc-codegen library version: {}",
                    env!("CARGO_PKG_VERSION")
                );
                return Ok(());
            }
            "--help" | "-h" => {
                println!("protoc-gen-actrframework - Protobuf plugin for Actrium ACTR framework");
                println!();
                println!("USAGE:");
                println!(
                    "    As protoc plugin: protoc --plugin=protoc-gen-actrframework=PATH --actrframework_out=OUT_DIR input.proto"
                );
                println!("    Version info:     protoc-gen-actrframework --version");
                println!();
                println!("VERSION:");
                println!("    {}", env!("CARGO_PKG_VERSION"));
                return Ok(());
            }
            _ => {}
        }
    }

    // Read CodeGeneratorRequest from stdin
    let mut stdin = io::stdin();
    let mut buf = Vec::new();
    stdin
        .read_to_end(&mut buf)
        .context("Failed to read from stdin")?;

    let request = CodeGeneratorRequest::decode(Bytes::from(buf))
        .context("Failed to decode CodeGeneratorRequest")?;

    // Generate code
    let response = generate_code(request)?;

    // Write CodeGeneratorResponse to stdout
    let mut out_buf = Vec::new();
    response
        .encode(&mut out_buf)
        .context("Failed to encode CodeGeneratorResponse")?;

    io::stdout()
        .write_all(&out_buf)
        .context("Failed to write to stdout")?;

    Ok(())
}

/// Parse parameters from protoc --actrframework_opt
/// Format: key1=value1,key2=value2
fn parse_parameters(param_str: &str) -> HashMap<String, String> {
    let mut params = HashMap::new();
    for pair in param_str.split(',') {
        if let Some((key, value)) = pair.split_once('=') {
            params.insert(key.trim().to_string(), value.trim().to_string());
        }
    }
    params
}

fn generate_code(request: CodeGeneratorRequest) -> Result<CodeGeneratorResponse> {
    // Set supported features if available (FEATURE_PROTO3_OPTIONAL = 1)
    let mut response = CodeGeneratorResponse {
        supported_features: Some(1u64),
        ..Default::default()
    };

    // Parse parameters from --actrframework_opt
    let params = parse_parameters(request.parameter.as_deref().unwrap_or(""));

    // Load the DispatchSpec written by the CLI at
    // `<output>/.actr/codegen/actr-dispatch-spec.v1.json`. The spec is the
    // single source of truth for local/remote classification, dependency
    // aliases, and route metadata — the plugin MUST NOT parse
    // `manifest.toml`/`manifest.lock.toml`, judge local/remote from file
    // paths, or embed `ActrType` strings.
    let spec = load_dispatch_spec(&params)?;
    let dispatch_spec_hash = params
        .get("dispatch_spec_hash")
        .cloned()
        .unwrap_or_default();

    // Build fully-qualified message name -> declaring proto package map so
    // imported RPC message types resolve to their declaring module instead of
    // the current service's package.
    let mut type_owners: HashMap<String, TypeOwner> = HashMap::new();
    for file in &request.proto_file {
        collect_type_owners(file, &mut type_owners, file.package());
    }

    // Build a lookup from route_key -> MethodDescriptorProto so the unified
    // dispatcher can find the method descriptor for each spec route.
    let mut methods_by_route: HashMap<String, prost_types::MethodDescriptorProto> = HashMap::new();
    for file in &request.proto_file {
        let package = file.package().to_string();
        for service in &file.service {
            let service_name = service.name().to_string();
            for method in &service.method {
                let route_key = if package.is_empty() {
                    format!("{}.{}", service_name, method.name())
                } else {
                    format!("{}.{}.{}", package, service_name, method.name())
                };
                methods_by_route.insert(route_key, method.clone());
            }
        }
    }

    let mut metadata = ActrGenMetadata {
        schema_version: "v1".to_string(),
        plugin_version: env!("CARGO_PKG_VERSION").to_string(),
        language: "rust",
        dispatch_spec_hash: (!dispatch_spec_hash.is_empty()).then(|| dispatch_spec_hash.clone()),
        generated_files: Vec::new(),
        generated_local_routes: spec
            .local_routes()
            .iter()
            .map(|route| route.route_key.clone())
            .collect(),
        generated_remote_routes: spec
            .remote_routes()
            .iter()
            .map(|route| route.route_key.clone())
            .collect(),
        local_services: Vec::new(),
        remote_services: Vec::new(),
    };

    // Process services for each file to generate. Local/remote is sourced
    // from the DispatchSpec: a service is local if any of its routes have
    // `dependency_alias: None`, remote if they have `Some(_)`.
    for file_name in &request.file_to_generate {
        if let Some(file) = request.proto_file.iter().find(|f| f.name() == file_name) {
            if file.service.len() > 1 {
                return Err(anyhow!(
                    "{}: defines {} services, but only one service per .proto file is supported. \
                     Split each service into its own .proto file.",
                    file_name,
                    file.service.len()
                ));
            }
            for service in &file.service {
                let generated_file = generate_service_code(
                    file,
                    service,
                    &type_owners,
                    &spec,
                    &methods_by_route,
                    &mut metadata,
                )?;
                let name = generated_file.name.clone().unwrap_or_default();
                if !name.is_empty() {
                    metadata.generated_files.push(name);
                    response.file.push(generated_file);
                }
            }
        }
    }

    // Generate the unified workload-level `actr_dispatch.rs` from the
    // DispatchSpec. One file per protoc invocation aggregates all local
    // services and remote routes.
    let generator_type_owners = type_owners
        .iter()
        .map(|(full_name, owner)| (full_name.clone(), owner.proto_package.clone()))
        .collect();
    let methods_refs: HashMap<String, &prost_types::MethodDescriptorProto> = methods_by_route
        .iter()
        .map(|(k, v)| (k.clone(), v))
        .collect();
    let unified_generator = UnifiedDispatcherGenerator::new(&spec, &generator_type_owners);
    let unified_code = unified_generator.generate(&methods_refs)?;
    let unified_file_name = "actr_dispatch.rs".to_string();
    metadata.generated_files.push(unified_file_name.clone());
    response.file.push(File {
        name: Some(unified_file_name),
        content: Some(unified_code),
        ..Default::default()
    });

    // Sort metadata route lists for deterministic output.
    metadata.generated_local_routes.sort();
    metadata.generated_remote_routes.sort();

    metadata
        .generated_files
        .push("actr-gen-meta.json".to_string());
    response.file.push(File {
        name: Some("actr-gen-meta.json".to_string()),
        content: Some(serde_json::to_string_pretty(&metadata)?),
        ..Default::default()
    });

    Ok(response)
}

/// Load the DispatchSpec from the path in `--actr_opt=dispatch_spec=<path>`
/// and verify its hash against `--actr_opt=dispatch_spec_hash=<hash>`.
fn load_dispatch_spec(params: &HashMap<String, String>) -> Result<DispatchSpec> {
    let spec_path = params.get("dispatch_spec").ok_or_else(|| {
        anyhow!(
            "dispatch_spec=<path> was not passed to the protoc plugin. \
             The CLI must write the DispatchSpec and pass both `dispatch_spec` \
             and `dispatch_spec_hash` via --actrframework_opt."
        )
    })?;
    let spec_hash = params
        .get("dispatch_spec_hash")
        .map(|s| s.as_str())
        .unwrap_or("");
    let spec = DispatchSpec::from_path(std::path::Path::new(spec_path))?;
    spec.verify_hash(spec_hash)?;
    Ok(spec)
}

fn collect_type_owners(
    file: &FileDescriptorProto,
    owners: &mut HashMap<String, TypeOwner>,
    package: &str,
) {
    for message in &file.message_type {
        collect_message_type_owner(message, owners, package, file.name(), "");
    }
}

fn collect_message_type_owner(
    message: &DescriptorProto,
    owners: &mut HashMap<String, TypeOwner>,
    package: &str,
    proto_file: &str,
    parent_type: &str,
) {
    let type_name = if parent_type.is_empty() {
        message.name().to_string()
    } else {
        format!("{}.{}", parent_type, message.name())
    };
    insert_type_owner(owners, package, proto_file, &type_name);

    for nested in &message.nested_type {
        collect_message_type_owner(nested, owners, package, proto_file, &type_name);
    }
}

fn insert_type_owner(
    owners: &mut HashMap<String, TypeOwner>,
    package: &str,
    proto_file: &str,
    type_name: &str,
) {
    let full_name = if package.is_empty() {
        type_name.to_string()
    } else {
        format!("{package}.{type_name}")
    };
    owners.insert(
        full_name,
        TypeOwner {
            proto_package: package.to_string(),
            proto_file: normalize_proto_path(proto_file),
            type_name: type_name.to_string(),
        },
    );
}

fn generate_service_code(
    file: &FileDescriptorProto,
    service: &ServiceDescriptorProto,
    type_owners: &HashMap<String, TypeOwner>,
    spec: &DispatchSpec,
    _methods_by_route: &HashMap<String, prost_types::MethodDescriptorProto>,
    metadata: &mut ActrGenMetadata,
) -> Result<File> {
    let service_name = service.name();
    let package_name = file.package();

    // Validate proto package name early to surface clear errors
    PackageName::new(package_name.to_string())
        .map_err(|e| anyhow!("Invalid proto package name '{}': {}", package_name, e))?;
    // Validate proto service name early
    ServiceName::new(service_name.to_string())
        .map_err(|e| anyhow!("Invalid proto service name '{}': {}", service_name, e))?;

    // Determine local/remote from the DispatchSpec: look up routes that
    // belong to this service. A service is local if its routes have
    // `dependency_alias: None`, remote if `Some(_)`.
    let proto_service = if package_name.is_empty() {
        service_name.to_string()
    } else {
        format!("{}.{}", package_name, service_name)
    };
    let service_routes: Vec<_> = spec
        .routes
        .iter()
        .filter(|route| route.proto_service == proto_service)
        .collect();
    if service_routes.is_empty() && !service.method.is_empty() {
        return Err(anyhow!(
            "dispatch spec has no routes for proto service `{proto_service}`"
        ));
    }
    let has_local_routes = service_routes
        .iter()
        .any(|route| route.dependency_alias.is_none());
    let has_remote_routes = service_routes
        .iter()
        .any(|route| route.dependency_alias.is_some());
    if has_local_routes && has_remote_routes {
        return Err(anyhow!(
            "dispatch spec mixes local and remote routes for proto service `{proto_service}`"
        ));
    }
    // A service without methods has no DispatchSpec routes; retain the
    // conventional local role so empty-service scaffolds remain valid.
    let is_local = !has_remote_routes;

    // Extract the proto file stem (e.g., "echo.proto" -> "echo")
    let file_stem = std::path::Path::new(file.name())
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(service_name)
        .to_snake_case();

    // Use the modern code generator
    let role = if is_local {
        GeneratorRole::ServerSide
    } else {
        GeneratorRole::ClientSide
    };

    let generator_type_owners = type_owners
        .iter()
        .map(|(full_name, owner)| (full_name.clone(), owner.proto_package.clone()))
        .collect();
    let generator = ModernGenerator::new(package_name, service_name, role, generator_type_owners);

    // Pass an empty `remote_services` slice — the per-service Dispatcher no
    // longer embeds remote forwarding arms with `ActrType` strings. Remote
    // forwarding is handled by the unified `actr_dispatch.rs`.
    let final_code = if role == GeneratorRole::ServerSide {
        generator.generate_with_remotes(&service.method, &[])?
    } else {
        generator.generate(&service.method)?
    };

    // Generate different file suffixes based on role
    let file_suffix = match role {
        GeneratorRole::ServerSide => "_actor",
        GeneratorRole::ClientSide => "_client",
    };

    // Build metadata for this service.
    if is_local {
        metadata
            .local_services
            .push(build_local_service_metadata(file, service, type_owners)?);
    } else {
        metadata.remote_services.push(build_remote_service_metadata(
            file,
            service,
            spec,
            type_owners,
        )?);
    }

    Ok(File {
        name: Some(format!("{}{}.rs", file_stem, file_suffix)),
        content: Some(final_code),
        insertion_point: None,
        generated_code_info: None,
    })
}

fn build_local_service_metadata(
    file: &FileDescriptorProto,
    service: &ServiceDescriptorProto,
    type_owners: &HashMap<String, TypeOwner>,
) -> Result<LocalServiceMetadata> {
    Ok(LocalServiceMetadata {
        name: service.name().to_string(),
        package: file.package().to_string(),
        proto_file: normalize_proto_path(file.name()),
        handler_interface: format!("{}Handler", service.name()),
        workload_type: format!("{}Workload", service.name()),
        dispatcher_type: format!("{}Dispatcher", service.name()),
        methods: service
            .method
            .iter()
            .map(|method| build_method_metadata(file, service, method, type_owners))
            .collect::<Result<Vec<_>>>()?,
    })
}

fn build_remote_service_metadata(
    file: &FileDescriptorProto,
    service: &ServiceDescriptorProto,
    spec: &DispatchSpec,
    type_owners: &HashMap<String, TypeOwner>,
) -> Result<RemoteServiceMetadata> {
    // Derive the dependency alias from the DispatchSpec routes that belong
    // to this service. All routes for a remote service share the same alias.
    let package_name = file.package();
    let proto_service = if package_name.is_empty() {
        service.name().to_string()
    } else {
        format!("{}.{}", package_name, service.name())
    };
    let dependency_aliases = spec
        .routes
        .iter()
        .filter(|route| route.proto_service == proto_service)
        .map(|route| {
            route
                .dependency_alias
                .as_deref()
                .filter(|alias| !alias.trim().is_empty())
                .ok_or_else(|| {
                    anyhow!(
                        "remote proto service `{proto_service}` has a route without a non-empty dependency_alias"
                    )
                })
        })
        .collect::<Result<BTreeSet<_>>>()?;
    let dependency_alias = if dependency_aliases.len() == 1 {
        dependency_aliases
            .first()
            .expect("length checked")
            .to_string()
    } else if dependency_aliases.is_empty() {
        return Err(anyhow!(
            "remote proto service `{proto_service}` has no dependency_alias"
        ));
    } else {
        return Err(anyhow!(
            "remote proto service `{proto_service}` uses multiple dependency aliases: {}",
            dependency_aliases
                .into_iter()
                .collect::<Vec<_>>()
                .join(", ")
        ));
    };

    Ok(RemoteServiceMetadata {
        name: service.name().to_string(),
        package: package_name.to_string(),
        proto_file: normalize_proto_path(file.name()),
        dependency_alias,
        client_type: format!("{}Client", service.name()),
        methods: service
            .method
            .iter()
            .map(|method| build_method_metadata(file, service, method, type_owners))
            .collect::<Result<Vec<_>>>()?,
    })
}

fn build_method_metadata(
    file: &FileDescriptorProto,
    service: &ServiceDescriptorProto,
    method: &prost_types::MethodDescriptorProto,
    type_owners: &HashMap<String, TypeOwner>,
) -> Result<MethodMetadata> {
    let package = file.package();
    let route_key = if package.is_empty() {
        format!("{}.{}", service.name(), method.name())
    } else {
        format!("{}.{}.{}", package, service.name(), method.name())
    };
    let input_ref = build_type_ref(
        file,
        service,
        method,
        "input",
        method.input_type(),
        type_owners,
    )?;
    let output_ref = build_type_ref(
        file,
        service,
        method,
        "output",
        method.output_type(),
        type_owners,
    )?;

    Ok(MethodMetadata {
        name: method.name().to_string(),
        snake_name: method.name().to_snake_case(),
        route_key,
        input_ref,
        output_ref,
    })
}

fn build_type_ref(
    file: &FileDescriptorProto,
    service: &ServiceDescriptorProto,
    method: &prost_types::MethodDescriptorProto,
    kind: &str,
    raw: &str,
    type_owners: &HashMap<String, TypeOwner>,
) -> Result<TypeRef> {
    let proto_type = normalize_type_name(raw);
    if let Some(owner) = type_owners.get(&proto_type) {
        return Ok(TypeRef {
            proto_type,
            type_name: owner.type_name.clone(),
            proto_package: owner.proto_package.clone(),
            proto_file: owner.proto_file.clone(),
        });
    }

    Err(anyhow!(
        "Cannot resolve {} type `{}` for {}.{} in {}: RPC types must be declared in one of the parsed proto files",
        kind,
        proto_type,
        service.name(),
        method.name(),
        normalize_proto_path(file.name())
    ))
}

fn normalize_type_name(raw: &str) -> String {
    raw.trim_start_matches('.').to_string()
}

fn normalize_proto_path(raw: &str) -> String {
    let normalized = raw.replace('\\', "/");
    let mut path = normalized.as_str();
    while let Some(stripped) = path.strip_prefix("./") {
        path = stripped;
    }
    if path.ends_with(".proto") {
        path.to_string()
    } else {
        format!("{path}.proto")
    }
}

// ---------------------------------------------------------------------------
// Legacy helpers retained for backward compat with tests that still
// exercise the old `ProtoSource::from_proto_file` + `RemoteFileActrTypes`
// path. The unified-dispatch code path no longer uses them, but they are
// kept so existing tests don't break during the migration window.
// ---------------------------------------------------------------------------

/// Proto source type enum — retained for backward compat with tests.
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq)]
pub enum ProtoSource {
    Local,
    Remote,
}

#[allow(dead_code)]
impl ProtoSource {
    pub fn from_proto_file(
        file: &FileDescriptorProto,
        params: &HashMap<String, String>,
    ) -> Result<Self> {
        let file_name = normalize_proto_path(file.name());
        let path_ends_with = |path: &str, suffix: &str| {
            path == suffix
                || path
                    .strip_suffix(suffix)
                    .is_some_and(|prefix| prefix.ends_with('/'))
        };

        let matches = |list_str: &str| {
            list_str.split(':').filter(|p| !p.is_empty()).any(|p| {
                let candidate = normalize_proto_path(p);
                path_ends_with(&file_name, &candidate) || path_ends_with(&candidate, &file_name)
            })
        };

        let in_remote = params.get("RemoteFiles").is_some_and(|s| matches(s));
        let in_local = params.get("LocalFiles").is_some_and(|s| matches(s));

        if in_remote && in_local {
            return Err(anyhow!(
                "{}: appears in both RemoteFiles and LocalFiles; a file must belong to exactly one side.",
                file_name
            ));
        }

        if in_remote {
            return Ok(Self::Remote);
        }
        if in_local {
            return Ok(Self::Local);
        }

        // Fallback: Assume Local if it has services, otherwise Remote
        let has_services = !file.service.is_empty();
        Ok(if has_services {
            Self::Local
        } else {
            Self::Remote
        })
    }
}

// Allow the legacy RemoteServiceInfo struct to be re-exported from the
// library crate without warnings — it is referenced by the re-export at
// the top of the file but no longer constructed by the new code path.
#[allow(dead_code)]
fn _silence_unused_warning() -> RemoteServiceInfo {
    unreachable!("legacy helper — never called")
}

#[cfg(test)]
mod tests {
    use super::*;
    use prost_types::MethodDescriptorProto;

    #[test]
    fn generate_code_metadata_preserves_imported_rpc_type_owner() {
        // Build a minimal DispatchSpec with one local route.
        let spec = DispatchSpec {
            schema_version: "v1".to_string(),
            routes: vec![actr_framework_protoc_codegen::DispatchRoute {
                route_key: "data_stream_app.DataStreamAppService.ContinuePromptResultStreams"
                    .to_string(),
                proto_file: "local/data_stream_app.proto".to_string(),
                proto_service: "data_stream_app.DataStreamAppService".to_string(),
                method: "ContinuePromptResultStreams".to_string(),
                input_type: "ask.ContinuePromptResultStreamsRequest".to_string(),
                output_type: "ask.ContinuePromptResultStreamsResponse".to_string(),
                payload_type: "rpcReliable".to_string(),
                dependency_alias: None,
            }],
            spec_hash: None,
        };

        // Build a CodeGeneratorRequest with one local file and one remote
        // imported file.
        let request = CodeGeneratorRequest {
            file_to_generate: vec!["local/data_stream_app.proto".to_string()],
            parameter: Some("dispatch_spec=ignored,dispatch_spec_hash=".to_string()),
            proto_file: vec![
                FileDescriptorProto {
                    name: Some("local/data_stream_app.proto".to_string()),
                    package: Some("data_stream_app".to_string()),
                    service: vec![ServiceDescriptorProto {
                        name: Some("DataStreamAppService".to_string()),
                        method: vec![MethodDescriptorProto {
                            name: Some("ContinuePromptResultStreams".to_string()),
                            input_type: Some(".ask.ContinuePromptResultStreamsRequest".to_string()),
                            output_type: Some(
                                ".ask.ContinuePromptResultStreamsResponse".to_string(),
                            ),
                            ..Default::default()
                        }],
                        ..Default::default()
                    }],
                    ..Default::default()
                },
                FileDescriptorProto {
                    name: Some("remote/ask-service/ask.proto".to_string()),
                    package: Some("ask".to_string()),
                    message_type: vec![
                        DescriptorProto {
                            name: Some("ContinuePromptResultStreamsRequest".to_string()),
                            ..Default::default()
                        },
                        DescriptorProto {
                            name: Some("ContinuePromptResultStreamsResponse".to_string()),
                            ..Default::default()
                        },
                    ],
                    ..Default::default()
                },
            ],
            ..Default::default()
        };

        // Bypass the DispatchSpec file load by intercepting the path.
        // We can't easily stub the file read, so instead verify the metadata
        // shape through a direct call to the metadata builder helpers.
        let mut type_owners: HashMap<String, TypeOwner> = HashMap::new();
        for file in &request.proto_file {
            collect_type_owners(file, &mut type_owners, file.package());
        }

        let mut methods_by_route: HashMap<String, prost_types::MethodDescriptorProto> =
            HashMap::new();
        for file in &request.proto_file {
            let package = file.package().to_string();
            for service in &file.service {
                let service_name = service.name().to_string();
                for method in &service.method {
                    let route_key = if package.is_empty() {
                        format!("{}.{}", service_name, method.name())
                    } else {
                        format!("{}.{}.{}", package, service_name, method.name())
                    };
                    methods_by_route.insert(route_key, method.clone());
                }
            }
        }

        let mut metadata = ActrGenMetadata {
            schema_version: "v1".to_string(),
            plugin_version: "test".to_string(),
            language: "rust",
            dispatch_spec_hash: None,
            generated_files: Vec::new(),
            generated_local_routes: spec
                .local_routes()
                .iter()
                .map(|route| route.route_key.clone())
                .collect(),
            generated_remote_routes: spec
                .remote_routes()
                .iter()
                .map(|route| route.route_key.clone())
                .collect(),
            local_services: Vec::new(),
            remote_services: Vec::new(),
        };

        let local_file = &request.proto_file[0];
        let local_service = &local_file.service[0];
        let _generated = generate_service_code(
            local_file,
            local_service,
            &type_owners,
            &spec,
            &methods_by_route,
            &mut metadata,
        )
        .unwrap();

        let metadata_json = serde_json::to_string_pretty(&metadata).unwrap();
        let metadata_value: serde_json::Value = serde_json::from_str(&metadata_json).unwrap();
        let method = &metadata_value["local_services"][0]["methods"][0];
        assert_eq!(
            method["input_ref"]["type_name"],
            "ContinuePromptResultStreamsRequest"
        );
        assert_eq!(
            method["input_ref"]["proto_type"],
            "ask.ContinuePromptResultStreamsRequest"
        );
        assert_eq!(method["input_ref"]["proto_package"], "ask");
        assert_eq!(
            method["input_ref"]["proto_file"],
            "remote/ask-service/ask.proto"
        );
    }

    #[test]
    fn collect_type_owners_only_indexes_rpc_message_types() {
        let file = FileDescriptorProto {
            name: Some("ask.proto".to_string()),
            package: Some("ask".to_string()),
            message_type: vec![DescriptorProto {
                name: Some("Request".to_string()),
                enum_type: vec![prost_types::EnumDescriptorProto {
                    name: Some("State".to_string()),
                    ..Default::default()
                }],
                ..Default::default()
            }],
            enum_type: vec![prost_types::EnumDescriptorProto {
                name: Some("Status".to_string()),
                ..Default::default()
            }],
            ..Default::default()
        };
        let mut owners = HashMap::new();
        collect_type_owners(&file, &mut owners, "ask");
        assert!(owners.contains_key("ask.Request"));
        assert!(!owners.contains_key("ask.Status"));
        assert!(!owners.contains_key("ask.Request.State"));
    }

    #[test]
    fn normalize_proto_path_is_cross_platform_and_stable() {
        assert_eq!(normalize_proto_path(r".\remote\ask"), "remote/ask.proto");
        assert_eq!(
            normalize_proto_path("./remote/ask.proto"),
            "remote/ask.proto"
        );
    }
}

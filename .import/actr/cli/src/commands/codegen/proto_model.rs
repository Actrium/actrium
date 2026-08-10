use crate::error::{ActrCliError, Result};
use crate::utils::to_snake_case;
use actr_config::ManifestConfig;
use prost::Message as _;
use prost_types::{
    DescriptorProto, FileDescriptorProto, FileDescriptorSet, MethodDescriptorProto,
    ServiceDescriptorProto,
};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProtoSide {
    Local,
    Remote,
}

#[derive(Debug, Clone)]
pub struct ProtoModel {
    pub files: Vec<ProtoFileModel>,
    pub local_services: Vec<ServiceModel>,
    pub remote_services: Vec<ServiceModel>,
}

#[derive(Debug, Clone)]
pub struct ProtoFileModel {
    pub proto_file: PathBuf,
    pub relative_path: PathBuf,
    pub package: String,
    pub side: ProtoSide,
    pub declared_type_names: Vec<String>,
    pub services: Vec<ServiceModel>,
}

#[derive(Debug, Clone)]
pub struct ServiceModel {
    pub name: String,
    pub package: String,
    pub proto_file: PathBuf,
    pub relative_path: PathBuf,
    pub side: ProtoSide,
    pub methods: Vec<MethodModel>,
    pub actr_type: Option<String>,
}

#[derive(Debug, Clone)]
pub struct MethodModel {
    pub name: String,
    pub snake_name: String,
    pub input_type: String,
    pub output_type: String,
    /// Lower-camel `actr.payload_type` value for this RPC method.
    pub payload_type: String,
    pub route_key: String,
}

/// Declaring owner of a proto message/enum type.
///
/// `proto_file` is the path relative to the proto root (matching
/// `ProtoFileModel::relative_path`), so it stays stable across the metadata,
/// scaffold catalog, and language generators.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeOwner {
    pub proto_package: String,
    pub proto_file: String,
    pub type_name: String,
}

/// Index mapping declared message/enum/service type names to the proto file
/// that declares them, built from a fully parsed [`ProtoModel`].
///
/// Resolution rules (see `resolve`):
/// - fully-qualified `package.Type` → exact match on the declaring file;
/// - unqualified `Type` → prefer the current proto file, then a unique
///   declaring file;
/// - an unqualified name declared in several non-current files is ambiguous
///   and surfaces as an error so the codegen does not silently pick the wrong
///   owner;
/// - types that cannot be resolved at all return `Ok(None)`; callers may
///   choose a local fallback for unqualified legacy references, but should not
///   silently rewrite unresolved qualified types to the current service owner.
#[derive(Debug, Clone, Default)]
pub struct TypeOwnerIndex {
    qualified: HashMap<String, TypeOwner>,
    bare: HashMap<String, Vec<TypeOwner>>,
}

impl TypeOwnerIndex {
    pub fn from_files(files: &[ProtoFileModel]) -> Self {
        let mut index = Self::default();
        for file in files {
            for declared in &file.declared_type_names {
                let owner = TypeOwner {
                    proto_package: file.package.clone(),
                    proto_file: file.relative_path.to_string_lossy().to_string(),
                    type_name: declared.clone(),
                };
                let qualified_name = if file.package.is_empty() {
                    declared.clone()
                } else {
                    format!("{}.{}", file.package, declared)
                };
                index.qualified.insert(qualified_name, owner.clone());
                index
                    .bare
                    .entry(declared.rsplit('.').next().unwrap_or(declared).to_string())
                    .or_default()
                    .push(owner);
            }
        }
        index
    }

    /// Resolve `referenced` (a proto type string as written in an RPC
    /// signature, e.g. `ask.ContinuePromptResultStreamsRequest` or
    /// `EchoRequest`) against the index.
    ///
    /// Returns `Ok(Some(owner))` when the declaring file is known, `Ok(None)`
    /// when the type cannot be resolved, or `Err(candidates)` when an
    /// unqualified type is ambiguous.
    pub fn resolve(
        &self,
        referenced: &str,
        current: &ProtoFileModel,
    ) -> std::result::Result<Option<TypeOwner>, Vec<TypeOwner>> {
        let normalized = normalize_proto_type(referenced);
        if normalized.is_empty() {
            return Ok(None);
        }

        if let Some(owner) = self.qualified.get(&normalized) {
            return Ok(Some(owner.clone()));
        }

        if normalized.contains('.') {
            if !current.package.is_empty()
                && let Some(owner) = self
                    .qualified
                    .get(&format!("{}.{}", current.package, normalized))
            {
                return Ok(Some(owner.clone()));
            }
            return Ok(None);
        }

        let type_name = normalized
            .rsplit('.')
            .next()
            .unwrap_or(&normalized)
            .to_string();

        if let Some(declared) = current
            .declared_type_names
            .iter()
            .find(|declared| declared.rsplit('.').next() == Some(type_name.as_str()))
        {
            return Ok(Some(TypeOwner {
                proto_package: current.package.clone(),
                proto_file: current.relative_path.to_string_lossy().to_string(),
                type_name: declared.clone(),
            }));
        }

        match self.bare.get(&type_name) {
            None => Ok(None),
            Some(candidates) if candidates.len() == 1 => Ok(Some(candidates[0].clone())),
            Some(candidates) => Err(candidates.clone()),
        }
    }
}

impl ProtoModel {
    /// Parse proto files using protoc descriptors.
    ///
    /// Runs `protoc --descriptor_set_out=<tmp>` over the input proto files,
    /// decodes the resulting `FileDescriptorSet` with `prost_types`, and
    /// builds the model from the descriptor (not text parsing).
    pub fn parse(
        proto_files: &[PathBuf],
        input_path: &Path,
        config: &ManifestConfig,
    ) -> Result<Self> {
        Self::from_descriptors(proto_files, input_path, config)
    }

    /// Build a [`ProtoModel`] from protoc-generated descriptors.
    ///
    /// This invokes `protoc --descriptor_set_out=<tmp>` with the same include
    /// paths the language generators use (the proto root directory), decodes
    /// the `FileDescriptorSet`, and populates the existing struct shapes from
    /// the descriptor.
    pub fn from_descriptors(
        proto_files: &[PathBuf],
        input_path: &Path,
        config: &ManifestConfig,
    ) -> Result<Self> {
        let proto_root = if input_path.is_file() {
            input_path.parent().unwrap_or_else(|| Path::new("."))
        } else {
            input_path
        };

        let dependency_actr_types: HashMap<String, String> = config
            .dependencies
            .iter()
            .filter_map(|dependency| {
                dependency
                    .actr_type
                    .as_ref()
                    .map(|actr_type| (dependency.alias.clone(), actr_type.to_string_repr()))
            })
            .collect();

        // Pre-check: reject paths that escape the proto root before invoking
        // protoc, so traversal attempts surface as config errors rather than
        // protoc I/O failures.
        for proto_file in proto_files {
            let relative_path = proto_file
                .strip_prefix(proto_root)
                .unwrap_or(proto_file)
                .to_path_buf();
            if relative_path
                .components()
                .any(|component| matches!(component, std::path::Component::ParentDir))
            {
                return Err(ActrCliError::config_error(format!(
                    "proto file path escapes the proto root: {}",
                    proto_file.display()
                )));
            }
        }

        // Run protoc --descriptor_set_out to get a binary FileDescriptorSet.
        let descriptor_bytes = run_protoc_descriptor_set(proto_files, proto_root)?;
        // Custom options are unknown fields to `prost_types`, so prost drops
        // `(actr.payload_type)` while decoding `MethodOptions`. Scan the raw
        // descriptor bytes before decoding and retain the value by descriptor
        // position (file -> service -> method).
        let raw_payload_types = extract_payload_types_from_descriptor_set(&descriptor_bytes)?;

        let descriptor_set =
            FileDescriptorSet::decode(descriptor_bytes.as_slice()).map_err(|e| {
                ActrCliError::config_error(format!("Failed to decode FileDescriptorSet: {e}"))
            })?;

        if raw_payload_types.len() != descriptor_set.file.len() {
            return Err(ActrCliError::config_error(format!(
                "raw descriptor scan found {} file(s), but prost decoded {}",
                raw_payload_types.len(),
                descriptor_set.file.len()
            )));
        }

        // Build a lookup from proto file name (relative to proto root) →
        // FileDescriptorProto so we can match input proto file paths to
        // descriptors.
        let mut files_by_name: HashMap<String, (usize, &FileDescriptorProto)> = HashMap::new();
        for (file_index, file_proto) in descriptor_set.file.iter().enumerate() {
            if let Some(name) = &file_proto.name {
                files_by_name.insert(name.clone(), (file_index, file_proto));
            }
        }

        let mut files = Vec::new();
        let mut local_services = Vec::new();
        let mut remote_services = Vec::new();

        for proto_file in proto_files {
            let relative_path = proto_file
                .strip_prefix(proto_root)
                .unwrap_or(proto_file)
                .to_path_buf();

            // Reject paths that escape the proto root.
            if relative_path
                .components()
                .any(|component| matches!(component, std::path::Component::ParentDir))
            {
                return Err(ActrCliError::config_error(format!(
                    "proto file path escapes the proto root: {}",
                    proto_file.display()
                )));
            }

            let side = classify_proto_side(&relative_path);

            // Look up the FileDescriptorProto by its name (relative path
            // using forward slashes, as protoc normalizes).
            let relative_str = relative_path.to_string_lossy().replace('\\', "/");
            let &(file_index, file_proto) = files_by_name.get(&relative_str).ok_or_else(|| {
                ActrCliError::config_error(format!(
                    "proto file '{}' not found in descriptor set (available: {:?})",
                    relative_str,
                    files_by_name.keys().collect::<Vec<_>>()
                ))
            })?;
            let file_payload_types = &raw_payload_types[file_index];
            if file_payload_types.len() != file_proto.service.len() {
                return Err(ActrCliError::config_error(format!(
                    "raw descriptor scan found {} service(s) in `{relative_str}`, but prost decoded {}",
                    file_payload_types.len(),
                    file_proto.service.len()
                )));
            }

            let package = file_proto.package.clone().unwrap_or_default();

            // Collect declared type names from the descriptor.
            let declared_type_names = collect_declared_types_from_descriptor(file_proto);

            // Build services from the descriptor.
            let remote_actr_type = if side == ProtoSide::Remote {
                let alias = remote_dependency_alias(&relative_path).ok_or_else(|| {
                    ActrCliError::config_error(format!(
                        "remote proto `{relative_str}` must be located under remote/<dependency-alias>/"
                    ))
                })?;
                let actr_type = dependency_actr_types.get(alias).cloned().ok_or_else(|| {
                    ActrCliError::config_error(format!(
                        "remote proto `{relative_str}` references dependency alias `{alias}`, but manifest.toml does not declare it with a complete actr_type"
                    ))
                })?;
                Some(actr_type)
            } else {
                None
            };

            let services: Vec<ServiceModel> = file_proto
                .service
                .iter()
                .enumerate()
                .map(|(service_index, service_desc)| {
                    let service_model = build_service_model(
                        service_desc,
                        &package,
                        proto_file,
                        &relative_path,
                        side,
                        remote_actr_type.clone(),
                        &file_payload_types[service_index],
                    )?;

                    if side == ProtoSide::Local {
                        local_services.push(service_model.clone());
                    } else {
                        remote_services.push(service_model.clone());
                    }

                    Ok(service_model)
                })
                .collect::<Result<Vec<_>>>()?;

            files.push(ProtoFileModel {
                proto_file: proto_file.clone(),
                relative_path,
                package,
                side,
                declared_type_names,
                services,
            });
        }

        Ok(Self {
            files,
            local_services,
            remote_services,
        })
    }
}

#[allow(dead_code)]
#[derive(Debug)]
struct ParsedProtoFile {
    package: String,
    declared_type_names: Vec<String>,
    services: Vec<ParsedService>,
}

#[derive(Debug)]
struct ParsedService {
    name: String,
    methods: Vec<MethodModel>,
}

fn classify_proto_side(relative_path: &Path) -> ProtoSide {
    let first_component = relative_path
        .components()
        .next()
        .and_then(|component| component.as_os_str().to_str());

    if first_component == Some("remote") {
        ProtoSide::Remote
    } else {
        ProtoSide::Local
    }
}

fn remote_dependency_alias(relative_path: &Path) -> Option<&str> {
    let mut components = relative_path.components();
    if components.next()?.as_os_str() != "remote" {
        return None;
    }
    let alias = components.next()?.as_os_str().to_str()?;
    components.next().map(|_| alias)
}

#[allow(dead_code)]
fn parse_proto_file(proto_file: &Path) -> Result<ParsedProtoFile> {
    let content = std::fs::read_to_string(proto_file).map_err(|e| {
        ActrCliError::config_error(format!(
            "Failed to read proto file {}: {e}",
            proto_file.display()
        ))
    })?;

    let mut package = String::new();
    let mut declared_type_names = collect_declared_type_names(&content);
    let mut current_service: Option<ParsedService> = None;
    let mut services = Vec::new();

    for raw_line in content.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with("//") {
            continue;
        }

        if let Some(rest) = line.strip_prefix("package ") {
            package = rest
                .trim_end_matches(';')
                .split_whitespace()
                .next()
                .unwrap_or_default()
                .to_string();
            continue;
        }

        if let Some(rest) = line.strip_prefix("service ") {
            if let Some(service) = current_service.take() {
                services.push(service);
            }

            let name = rest
                .split(|character: char| character.is_whitespace() || character == '{')
                .find(|segment| !segment.is_empty())
                .unwrap_or_default()
                .to_string();

            if !name.is_empty() {
                declared_type_names.push(name.clone());
                current_service = Some(ParsedService {
                    name,
                    methods: Vec::new(),
                });
            }
            continue;
        }

        if extract_declared_type_name(line, "message ").is_some() {
            continue;
        }

        if extract_declared_type_name(line, "enum ").is_some() {
            continue;
        }

        if let Some(rest) = line.strip_prefix("rpc ")
            && let Some(service) = current_service.as_mut()
        {
            if let Some(method) = parse_rpc_method(rest, &package, &service.name) {
                service.methods.push(method);
            }
            continue;
        }

        if line.starts_with('}')
            && let Some(service) = current_service.take()
        {
            services.push(service);
        }
    }

    if let Some(service) = current_service.take() {
        services.push(service);
    }

    Ok(ParsedProtoFile {
        package,
        declared_type_names,
        services,
    })
}

#[allow(dead_code)]
fn parse_rpc_method(rest: &str, package: &str, service_name: &str) -> Option<MethodModel> {
    let input_start = rest.find('(')?;
    let method_name = rest[..input_start]
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .to_string();
    if method_name.is_empty() {
        return None;
    }

    let after_input_start = &rest[input_start + 1..];
    let input_end = after_input_start.find(')')?;
    let input_type = normalize_proto_type(&after_input_start[..input_end]);

    let returns_pos = after_input_start.find("returns")?;
    let after_returns = &after_input_start[returns_pos + "returns".len()..];
    let output_start = after_returns.find('(')?;
    let output_end = after_returns[output_start + 1..].find(')')?;
    let output_type =
        normalize_proto_type(&after_returns[output_start + 1..output_start + 1 + output_end]);

    let route_key = if package.is_empty() {
        format!("{service_name}.{method_name}")
    } else {
        format!("{package}.{service_name}.{method_name}")
    };

    Some(MethodModel {
        snake_name: to_snake_case(&method_name),
        name: method_name,
        input_type,
        output_type,
        payload_type: "rpcReliable".to_string(),
        route_key,
    })
}

#[allow(dead_code)]
fn normalize_proto_type(raw_type: &str) -> String {
    raw_type.trim().trim_start_matches('.').to_string()
}

#[allow(dead_code)]
fn collect_declared_type_names(content: &str) -> Vec<String> {
    let tokens = tokenize_proto_declarations(content);
    let mut declared = Vec::new();
    let mut type_scopes: Vec<Option<String>> = Vec::new();
    let mut pending_scope: Option<Option<String>> = None;
    let mut index = 0;

    while index < tokens.len() {
        match tokens[index].as_str() {
            "message" | "enum" => {
                if let Some(name) = tokens.get(index + 1).filter(|token| is_proto_ident(token)) {
                    let mut parts = type_scopes
                        .iter()
                        .filter_map(|scope| scope.as_deref())
                        .map(str::to_string)
                        .collect::<Vec<_>>();
                    parts.push(name.clone());
                    let full_name = parts.join(".");
                    declared.push(full_name.clone());
                    pending_scope = Some(Some(full_name));
                    index += 2;
                    continue;
                }
            }
            "{" => {
                type_scopes.push(pending_scope.take().unwrap_or(None));
            }
            "}" => {
                type_scopes.pop();
                pending_scope = None;
            }
            _ => {}
        }
        index += 1;
    }

    declared
}

#[allow(dead_code)]
fn tokenize_proto_declarations(content: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    for line in content.lines() {
        let line = line.split_once("//").map_or(line, |(before, _)| before);
        let mut current = String::new();
        for character in line.chars() {
            if character == '{' || character == '}' {
                if !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                }
                tokens.push(character.to_string());
            } else if character.is_alphanumeric() || character == '_' {
                current.push(character);
            } else if !current.is_empty() {
                tokens.push(std::mem::take(&mut current));
            }
        }
        if !current.is_empty() {
            tokens.push(current);
        }
    }
    tokens
}

#[allow(dead_code)]
fn is_proto_ident(token: &str) -> bool {
    token
        .chars()
        .next()
        .is_some_and(|character| character.is_alphabetic() || character == '_')
}

#[allow(dead_code)]
fn extract_declared_type_name(line: &str, prefix: &str) -> Option<String> {
    let rest = line.strip_prefix(prefix)?;
    let name = rest
        .split(|character: char| character.is_whitespace() || character == '{')
        .find(|segment| !segment.is_empty())?;
    Some(name.to_string())
}

// ---------------------------------------------------------------------------
// Descriptor-based model building helpers
// ---------------------------------------------------------------------------

/// Run `protoc --descriptor_set_out=<tmp>` and return the raw bytes of the
/// `FileDescriptorSet`.
fn run_protoc_descriptor_set(proto_files: &[PathBuf], proto_root: &Path) -> Result<Vec<u8>> {
    let tmp_dir = tempfile::TempDir::new().map_err(|e| {
        ActrCliError::config_error(format!("Failed to create temp dir for descriptor set: {e}"))
    })?;
    let descriptor_path = tmp_dir.path().join("descriptor_set.bin");

    let mut cmd = std::process::Command::new("protoc");
    cmd.arg(format!("--proto_path={}", proto_root.display()))
        .arg(format!(
            "--descriptor_set_out={}",
            descriptor_path.display()
        ))
        .arg("--include_imports")
        .arg("--include_source_info");

    for proto_file in proto_files {
        cmd.arg(proto_file);
    }

    let output = cmd.output().map_err(|e| {
        ActrCliError::command_error(format!("Failed to execute protoc (descriptor_set): {e}"))
    })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(ActrCliError::command_error(format!(
            "protoc (descriptor_set) execution failed: {stderr}"
        )));
    }

    std::fs::read(&descriptor_path).map_err(|e| {
        ActrCliError::config_error(format!("Failed to read descriptor set output: {e}"))
    })
}

/// Build a [`ServiceModel`] from a [`ServiceDescriptorProto`].
fn build_service_model(
    service_desc: &ServiceDescriptorProto,
    package: &str,
    proto_file: &Path,
    relative_path: &Path,
    side: ProtoSide,
    actr_type: Option<String>,
    method_payload_types: &[Option<PayloadTypeValue>],
) -> Result<ServiceModel> {
    let name = service_desc.name.clone().unwrap_or_default();

    if method_payload_types.len() != service_desc.method.len() {
        return Err(ActrCliError::config_error(format!(
            "raw descriptor scan found {} method(s) for service `{name}`, but prost decoded {}",
            method_payload_types.len(),
            service_desc.method.len()
        )));
    }

    let methods: Vec<MethodModel> = service_desc
        .method
        .iter()
        .zip(method_payload_types.iter().copied())
        .map(|(method_desc, payload_type)| {
            build_method_model(method_desc, package, &name, payload_type)
        })
        .collect::<Result<Vec<_>>>()?;

    Ok(ServiceModel {
        name,
        package: package.to_string(),
        proto_file: proto_file.to_path_buf(),
        relative_path: relative_path.to_path_buf(),
        side,
        methods,
        actr_type,
    })
}

/// Build a [`MethodModel`] from a [`MethodDescriptorProto`].
///
/// Returns an error if the method has streaming semantics (v1 dispatch spec
/// does not support streaming RPC).
fn build_method_model(
    method_desc: &MethodDescriptorProto,
    package: &str,
    service_name: &str,
    payload_type: Option<PayloadTypeValue>,
) -> Result<MethodModel> {
    let method_name = method_desc.name.clone().unwrap_or_default();
    if method_name.is_empty() {
        return Err(ActrCliError::config_error(
            "encountered a method with empty name in proto descriptor",
        ));
    }

    let input_type = method_desc
        .input_type
        .as_deref()
        .unwrap_or("")
        .trim_start_matches('.')
        .to_string();

    let output_type = method_desc
        .output_type
        .as_deref()
        .unwrap_or("")
        .trim_start_matches('.')
        .to_string();

    let route_key = if package.is_empty() {
        format!("{service_name}.{method_name}")
    } else {
        format!("{package}.{service_name}.{method_name}")
    };

    // v1 dispatch spec does not support streaming RPC.
    if method_desc.client_streaming.unwrap_or(false)
        || method_desc.server_streaming.unwrap_or(false)
    {
        return Err(ActrCliError::config_error(format!(
            "v1 dispatch spec does not support streaming RPC: {route_key}"
        )));
    }

    Ok(MethodModel {
        snake_name: to_snake_case(&method_name),
        name: method_name,
        input_type,
        output_type,
        payload_type: payload_type
            .unwrap_or(PayloadTypeValue::RpcReliable)
            .as_lower_camel()
            .to_string(),
        route_key,
    })
}

/// Collect declared type names (messages and enums, including nested) from a
/// [`FileDescriptorProto`].
///
/// Returns names *relative to the file's package* (without the package
/// prefix), matching the behavior of the legacy text-based
/// `collect_declared_type_names`. The `TypeOwnerIndex::from_files` builder
/// prepends the package itself, so we must not include it here.
fn collect_declared_types_from_descriptor(file_proto: &FileDescriptorProto) -> Vec<String> {
    let mut declared = Vec::new();

    for message in &file_proto.message_type {
        collect_message_types(message, "", &mut declared);
    }

    for enum_desc in &file_proto.enum_type {
        if let Some(name) = &enum_desc.name {
            declared.push(name.clone());
        }
    }

    declared
}

/// Recursively collect message type names (relative to the file's package,
/// i.e. *without* the package prefix), including nested messages and nested
/// enums.
fn collect_message_types(message: &DescriptorProto, prefix: &str, declared: &mut Vec<String>) {
    let name = message.name.as_deref().unwrap_or("");
    let relative_name = if prefix.is_empty() {
        name.to_string()
    } else {
        format!("{prefix}.{name}")
    };

    if !relative_name.is_empty() {
        declared.push(relative_name.clone());
    }

    for nested_message in &message.nested_type {
        collect_message_types(nested_message, &relative_name, declared);
    }

    for nested_enum in &message.enum_type {
        if let Some(enum_name) = &nested_enum.name {
            let enum_full = format!("{relative_name}.{enum_name}");
            declared.push(enum_full);
        }
    }
}

/// Wire-format scanner for the `(actr.payload_type)` extension (field 50001).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PayloadTypeValue {
    RpcReliable,
    RpcSignal,
    StreamReliable,
    StreamLatencyFirst,
    MediaRtp,
}

impl PayloadTypeValue {
    fn from_u64(value: u64) -> Option<Self> {
        match value {
            0 => Some(Self::RpcReliable),
            1 => Some(Self::RpcSignal),
            2 => Some(Self::StreamReliable),
            3 => Some(Self::StreamLatencyFirst),
            4 => Some(Self::MediaRtp),
            _ => None,
        }
    }

    fn as_lower_camel(&self) -> &'static str {
        match self {
            Self::RpcReliable => "rpcReliable",
            Self::RpcSignal => "rpcSignal",
            Self::StreamReliable => "streamReliable",
            Self::StreamLatencyFirst => "streamLatencyFirst",
            Self::MediaRtp => "mediaRtp",
        }
    }
}

type WireResult<T> = std::result::Result<T, String>;

enum WireValue<'a> {
    Varint(u64),
    LengthDelimited(&'a [u8]),
    Other,
}

struct WireField<'a> {
    number: u64,
    value: WireValue<'a>,
}

/// Read one base-128 varint, rejecting truncated and overflowing encodings.
fn read_varint(buf: &[u8], pos: &mut usize) -> WireResult<u64> {
    let mut result = 0_u64;
    for byte_index in 0..10 {
        let byte = *buf
            .get(*pos)
            .ok_or_else(|| "truncated protobuf varint".to_string())?;
        *pos += 1;
        if byte_index == 9 && byte > 1 {
            return Err("protobuf varint overflows u64".to_string());
        }
        result |= u64::from(byte & 0x7f) << (byte_index * 7);
        if byte & 0x80 == 0 {
            return Ok(result);
        }
    }
    Err("protobuf varint exceeds ten bytes".to_string())
}

fn advance(buf: &[u8], pos: &mut usize, amount: usize) -> WireResult<()> {
    let end = (*pos)
        .checked_add(amount)
        .filter(|end| *end <= buf.len())
        .ok_or_else(|| "protobuf field extends past the descriptor buffer".to_string())?;
    *pos = end;
    Ok(())
}

fn read_wire_value<'a>(
    buf: &'a [u8],
    pos: &mut usize,
    field_number: u64,
    wire_type: u8,
) -> WireResult<WireValue<'a>> {
    match wire_type {
        0 => Ok(WireValue::Varint(read_varint(buf, pos)?)),
        1 => {
            advance(buf, pos, 8)?;
            Ok(WireValue::Other)
        }
        2 => {
            let length = usize::try_from(read_varint(buf, pos)?)
                .map_err(|_| "length-delimited protobuf field is too large".to_string())?;
            let start = *pos;
            advance(buf, pos, length)?;
            Ok(WireValue::LengthDelimited(&buf[start..*pos]))
        }
        3 => {
            skip_group(buf, pos, field_number)?;
            Ok(WireValue::Other)
        }
        4 => Err(format!(
            "unexpected end-group marker for protobuf field {field_number}"
        )),
        5 => {
            advance(buf, pos, 4)?;
            Ok(WireValue::Other)
        }
        _ => Err(format!("invalid protobuf wire type {wire_type}")),
    }
}

fn skip_group(buf: &[u8], pos: &mut usize, start_field_number: u64) -> WireResult<()> {
    loop {
        if *pos >= buf.len() {
            return Err(format!(
                "unterminated protobuf group for field {start_field_number}"
            ));
        }
        let key = read_varint(buf, pos)?;
        let field_number = key >> 3;
        let wire_type = (key & 0x07) as u8;
        if field_number == 0 {
            return Err("protobuf field number zero is invalid".to_string());
        }
        if wire_type == 4 {
            return if field_number == start_field_number {
                Ok(())
            } else {
                Err(format!(
                    "protobuf group for field {start_field_number} ended with field {field_number}"
                ))
            };
        }
        read_wire_value(buf, pos, field_number, wire_type)?;
    }
}

fn next_wire_field<'a>(buf: &'a [u8], pos: &mut usize) -> WireResult<Option<WireField<'a>>> {
    if *pos == buf.len() {
        return Ok(None);
    }
    let key = read_varint(buf, pos)?;
    let field_number = key >> 3;
    if field_number == 0 {
        return Err("protobuf field number zero is invalid".to_string());
    }
    let wire_type = (key & 0x07) as u8;
    let value = read_wire_value(buf, pos, field_number, wire_type)?;
    Ok(Some(WireField {
        number: field_number,
        value,
    }))
}

fn length_delimited_fields(buf: &[u8], target: u64) -> WireResult<Vec<&[u8]>> {
    let mut fields = Vec::new();
    let mut pos = 0;
    while let Some(field) = next_wire_field(buf, &mut pos)? {
        if field.number == target {
            match field.value {
                WireValue::LengthDelimited(value) => fields.push(value),
                _ => {
                    return Err(format!(
                        "protobuf field {target} has the wrong wire type; expected length-delimited"
                    ));
                }
            }
        }
    }
    Ok(fields)
}

/// Scan `FileDescriptorSet.file[*].service[*].method[*].options` directly in
/// the raw wire format so custom extension field 50001 survives prost decode.
fn extract_payload_types_from_descriptor_set(
    descriptor_bytes: &[u8],
) -> Result<Vec<Vec<Vec<Option<PayloadTypeValue>>>>> {
    let raw_files = length_delimited_fields(descriptor_bytes, 1).map_err(|error| {
        ActrCliError::config_error(format!("failed to scan raw FileDescriptorSet: {error}"))
    })?;
    raw_files
        .into_iter()
        .map(|raw_file| {
            let raw_services = length_delimited_fields(raw_file, 6).map_err(|error| {
                ActrCliError::config_error(format!(
                    "failed to scan services in raw FileDescriptorProto: {error}"
                ))
            })?;
            raw_services
                .into_iter()
                .map(|raw_service| {
                    let raw_methods = length_delimited_fields(raw_service, 2).map_err(|error| {
                        ActrCliError::config_error(format!(
                            "failed to scan methods in raw ServiceDescriptorProto: {error}"
                        ))
                    })?;
                    raw_methods
                        .into_iter()
                        .map(extract_payload_type_from_raw_method)
                        .collect::<Result<Vec<_>>>()
                })
                .collect::<Result<Vec<_>>>()
        })
        .collect()
}

fn extract_payload_type_from_raw_method(raw_method: &[u8]) -> Result<Option<PayloadTypeValue>> {
    const METHOD_OPTIONS_FIELD_NUMBER: u64 = 4;
    const PAYLOAD_TYPE_EXTENSION_FIELD_NUMBER: u64 = 50001;

    let options =
        length_delimited_fields(raw_method, METHOD_OPTIONS_FIELD_NUMBER).map_err(|error| {
            ActrCliError::config_error(format!(
                "failed to scan raw MethodDescriptorProto options: {error}"
            ))
        })?;
    let mut payload_type = None;
    for raw_options in options {
        let mut pos = 0;
        while let Some(field) = next_wire_field(raw_options, &mut pos).map_err(|error| {
            ActrCliError::config_error(format!("failed to scan raw MethodOptions: {error}"))
        })? {
            if field.number != PAYLOAD_TYPE_EXTENSION_FIELD_NUMBER {
                continue;
            }
            let WireValue::Varint(value) = field.value else {
                return Err(ActrCliError::config_error(
                    "(actr.payload_type) field 50001 must use protobuf varint encoding",
                ));
            };
            let value = PayloadTypeValue::from_u64(value).ok_or_else(|| {
                ActrCliError::config_error(format!(
                    "(actr.payload_type) field 50001 has unknown enum value {value}"
                ))
            })?;
            if payload_type.replace(value).is_some() {
                return Err(ActrCliError::config_error(
                    "(actr.payload_type) field 50001 is encoded more than once",
                ));
            }
        }
    }
    Ok(payload_type)
}

#[cfg(test)]
#[path = "proto_model_tests.rs"]
mod tests;

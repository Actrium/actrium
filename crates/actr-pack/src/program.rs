//! Complete signed Program Releases, layered on the existing v2 archive.
//!
//! Verification grants no permissions. An embedding Host must intersect the
//! requests below with its grants/platform/session and enforce every quota.
use std::collections::{BTreeMap, BTreeSet};

use ed25519_dalek::VerifyingKey;
use semver::{Version, VersionReq};
use serde::{Deserialize, Serialize};

use crate::util::sha256_hex;
use crate::{ActorPackageManifest, InspectedActorPackage, PackError};

pub const SERVICE_COMPONENT_PROFILE: &str = "actrium.service-component/1";
pub const SERVICE_ABI: &str = "actr:workload/workload@0.8.0";
pub const SERVICE_HOST: &str = "actr:workload/host@0.8.0";

macro_rules! declaration {
    ($name:ident { $($field:ident: $ty:ty),* $(,)? }) => {
        #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
        #[serde(deny_unknown_fields)]
        pub struct $name { $(pub $field: $ty),* }
    }
}

declaration!(ProgramDeclaration {
    schema_version: u32,
    min_actrium_version: String,
    front_host_api: String,
    service_host_api: String,
    front: FrontDeclaration,
    service: ServiceDeclaration,
    capabilities: CapabilityRequests,
    quotas: ProgramQuotas,
    session: SessionDeclaration,
});
declaration!(FrontDeclaration {
    guest_runtime: String,
    guest_entry: String,
    keel_api: String,
    contract_manifest: String,
    contract_digest: String,
});
declaration!(ServiceDeclaration {
    abi: String,
    contract: String,
    protocol_fingerprint: String,
    workspace: WorkspaceDeclaration,
});
declaration!(WorkspaceDeclaration {
    requirement: WorkspaceRequirement,
    provisioning: WorkspaceProvisioning,
    access: WorkspaceAccess,
    providers: Vec<String>,
    lease: WorkspaceLease,
});
macro_rules! choice {
    ($name:ident { $($variant:ident),* }) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
        #[serde(rename_all = "kebab-case")]
        pub enum $name { $($variant),* }
    }
}
choice!(WorkspaceRequirement {
    None,
    Optional,
    Required
});
choice!(WorkspaceProvisioning {
    None,
    UserSelected,
    Managed
});
choice!(WorkspaceAccess {
    None,
    ReadOnly,
    ReadWrite
});
choice!(WorkspaceLease {
    None,
    SharedReader,
    ExclusiveWriter
});
choice!(AttachmentPolicy { Exclusive, Shared });
declaration!(SessionDeclaration {
    attachment_policy: AttachmentPolicy
});
declaration!(CapabilityRequests {
    front: Vec<String>, service: Vec<String>, components: Vec<String>, outbound_domains: Vec<String>,
});
declaration!(ProgramQuotas {
    guest_heap_bytes: u64,
    guest_execution_ms: u64,
    render_ops: u64,
    service_memory_bytes: u64,
    service_fuel: u64,
    service_execution_ms: u64,
    table_elements: u32,
    streams: u32,
    io_bytes: u64,
    storage_bytes: u64,
    network_bytes: u64,
});
// A signed binding between Front capability requests and the Service proto.
declaration!(FrontContract {
    schema: String, service_contract: String, service_contract_digest: String,
    capabilities: Vec<String>, components: Vec<String>,
});

/// Versions are supplied by the controlled Host, never taken from the package.
#[derive(Debug, Clone)]
pub struct ProgramHostPolicy {
    pub actrium_version: Version,
    pub front_host_api: Version,
    pub service_host_api: Version,
    pub keel_api: Version,
}
impl Default for ProgramHostPolicy {
    fn default() -> Self {
        Self {
            actrium_version: Version::new(1, 0, 0),
            front_host_api: Version::new(1, 0, 0),
            service_host_api: Version::new(1, 0, 0),
            keel_api: Version::new(1, 0, 0),
        }
    }
}

/// Immutable result: metadata and bytes are obtained from one authenticated
/// archive. No path/URL is accepted when retrieving the Guest entry.
#[derive(Debug, Clone)]
pub struct VerifiedProgramRelease {
    manifest: ActorPackageManifest,
    declaration: ProgramDeclaration,
    digest: String,
    payloads: BTreeMap<String, Vec<u8>>,
}
impl VerifiedProgramRelease {
    pub fn manifest(&self) -> &ActorPackageManifest {
        &self.manifest
    }
    pub fn declaration(&self) -> &ProgramDeclaration {
        &self.declaration
    }
    pub fn digest(&self) -> &str {
        &self.digest
    }
    pub fn guest_bytes(&self) -> &[u8] {
        &self.payloads[&self.declaration.front.guest_entry]
    }
    pub fn service_bytes(&self) -> &[u8] {
        &self.payloads[&self.manifest.workload.entry]
    }
    pub fn resource(&self, path: &str) -> Option<&[u8]> {
        self.payloads.get(path).map(Vec::as_slice)
    }
}

fn invalid(message: impl Into<String>) -> PackError {
    PackError::InvalidPackage(message.into())
}
fn require(ok: bool, message: &str) -> Result<(), PackError> {
    if ok { Ok(()) } else { Err(invalid(message)) }
}
fn program(manifest: &ActorPackageManifest) -> Result<&ProgramDeclaration, PackError> {
    manifest
        .extensions
        .actrium
        .as_ref()
        .and_then(|x| x.program.as_ref())
        .ok_or_else(|| invalid("not a complete Actrium Program Release"))
}
fn file<'a>(
    manifest: &'a ActorPackageManifest,
    path: &str,
    role: &str,
    media: &str,
) -> Result<&'a crate::PackageFileEntry, PackError> {
    let file = manifest
        .files
        .iter()
        .find(|f| f.path == path)
        .ok_or_else(|| invalid(format!("missing declared {role} entry `{path}`")))?;
    require(
        file.role == role && file.media_type == media,
        "entry role/media-type mismatch",
    )?;
    Ok(file)
}
fn unique(values: &[String]) -> bool {
    values.iter().all(|x| !x.is_empty())
        && values.iter().collect::<BTreeSet<_>>().len() == values.len()
}

pub(crate) fn validate_manifest(
    m: &ActorPackageManifest,
    p: &ProgramDeclaration,
) -> Result<(), PackError> {
    require(
        p.schema_version == 1,
        "unsupported Actrium Program schema_version",
    )?;
    let identity = actr_protocol::ActrType {
        publisher: m.actor.publisher.clone(),
        name: m.actor.name.clone(),
        version: m.actor.version.clone(),
    };
    identity.validate().map_err(|e| invalid(e.to_string()))?;
    require(
        canonical_domain(&m.actor.publisher),
        "Program publisher must be a canonical DNS domain",
    )?;
    require(
        m.workload.profile == SERVICE_COMPONENT_PROFILE && m.workload.keel.is_none(),
        "formal Program workload must be a Service Component; Front Host/native plugins are not entries",
    )?;
    require(
        m.host.profile == "actrium.service-host/1",
        "unsupported Service Host profile",
    )?;
    require(
        m.host.api == p.service_host_api,
        "host.api and service_host_api disagree",
    )?;
    require(
        m.host.required_capabilities == p.capabilities.service
            && m.host.optional_capabilities.is_empty(),
        "Service capabilities must match Host requirements",
    )?;
    require(
        p.front.guest_runtime == "keel-js" && p.service.abi == SERVICE_ABI,
        "unsupported Guest runtime or Service ABI",
    )?;
    file(m, &m.workload.entry, "workload", "application/wasm")?;
    file(m, &p.front.guest_entry, "guest", "text/javascript")?;
    let front_contract = file(
        m,
        &p.front.contract_manifest,
        "front-contract",
        "application/json",
    )?;
    let service_contract = file(
        m,
        &p.service.contract,
        "service-contract",
        "text/x-protobuf",
    )?;
    require(
        p.front.contract_digest == format!("sha256:{}", front_contract.sha256)
            && p.service.protocol_fingerprint == format!("sha256:{}", service_contract.sha256),
        "contract digest is not bound to the declared contract bytes",
    )?;
    if let Some(icon) = m.extensions.actrium.as_ref().and_then(|x| x.icon.as_ref()) {
        require(
            m.files.iter().any(|f| &f.path == icon && f.role == "asset"),
            "icon must be a signed asset",
        )?;
    }
    // Closed roles prevent a second executable from masquerading as a Front
    // Host, native module or unlisted JS entry. Assets have a non-executable
    // media allowlist; magic checks below do not trust the extension alone.
    for f in &m.files {
        let bound = f.path == m.workload.entry
            || f.path == p.front.guest_entry
            || f.path == p.front.contract_manifest
            || f.path == p.service.contract;
        require(
            bound
                || (f.role == "asset"
                    && matches!(
                        f.media_type.as_str(),
                        "image/png"
                            | "image/jpeg"
                            | "image/webp"
                            | "text/plain"
                            | "application/json"
                    ))
                || (f.role == "lock"
                    && f.path == "manifest.lock.toml"
                    && f.media_type == "application/toml"),
            "Program contains an unbound executable, contract, or unsupported resource",
        )?;
    }
    for values in [
        &p.capabilities.front,
        &p.capabilities.service,
        &p.capabilities.components,
        &p.capabilities.outbound_domains,
    ] {
        require(
            unique(values),
            "capability requests must be nonempty and unique",
        )?;
    }
    const FRONT: &[&str] = &[
        "ui.render",
        "actor.call",
        "camera",
        "microphone",
        "clipboard",
        "notification",
        "location",
    ];
    const SERVICE: &[&str] = &[
        "actor.call",
        "actor.discover",
        "actor.stream",
        "log",
        "workspace.read",
        "workspace.write",
        "network.http",
    ];
    require(
        p.capabilities
            .front
            .iter()
            .all(|x| FRONT.contains(&x.as_str()))
            && p.capabilities
                .service
                .iter()
                .all(|x| SERVICE.contains(&x.as_str())),
        "unsupported capability declaration",
    )?;
    require(
        p.capabilities
            .outbound_domains
            .iter()
            .all(|x| canonical_domain(x)),
        "outbound domains must be canonical DNS names",
    )?;
    require(
        p.capabilities.outbound_domains.is_empty()
            || p.capabilities.service.iter().any(|x| x == "network.http"),
        "outbound domains require network.http",
    )?;
    let w = &p.service.workspace;
    if w.requirement == WorkspaceRequirement::None {
        require(
            w.provisioning == WorkspaceProvisioning::None
                && w.access == WorkspaceAccess::None
                && w.providers.is_empty()
                && w.lease == WorkspaceLease::None
                && !p
                    .capabilities
                    .service
                    .iter()
                    .any(|x| x.starts_with("workspace.")),
            "workspace=none cannot request workspace access",
        )?;
    } else {
        require(
            w.provisioning != WorkspaceProvisioning::None
                && !w.providers.is_empty()
                && unique(&w.providers)
                && w.providers.iter().all(|x| {
                    ["managed-store", "local-folder", "cloud-drive"].contains(&x.as_str())
                }),
            "invalid Workspace provisioning/providers",
        )?;
        let (lease, capability) = match w.access {
            WorkspaceAccess::ReadWrite => (WorkspaceLease::ExclusiveWriter, "workspace.write"),
            WorkspaceAccess::ReadOnly => (WorkspaceLease::SharedReader, "workspace.read"),
            WorkspaceAccess::None => return Err(invalid("Workspace requires access mode")),
        };
        require(
            w.lease == lease && p.capabilities.service.iter().any(|x| x == capability),
            "Workspace access, capability and lease disagree",
        )?;
    }
    let q = &p.quotas;
    require(
        q.guest_heap_bytes > 0
            && q.guest_heap_bytes <= 256 * 1024 * 1024
            && q.guest_execution_ms > 0
            && q.guest_execution_ms <= 5000
            && q.render_ops > 0
            && q.render_ops <= 100_000
            && q.service_memory_bytes > 0
            && q.service_memory_bytes <= 256 * 1024 * 1024
            && q.service_fuel > 0
            && q.service_fuel <= 100_000_000
            && q.service_execution_ms > 0
            && q.service_execution_ms <= 30_000
            && q.table_elements > 0
            && q.table_elements <= 100_000
            && q.streams <= 1024
            && q.io_bytes <= 64 * 1024 * 1024
            && q.storage_bytes <= 1024 * 1024 * 1024
            && q.network_bytes <= 64 * 1024 * 1024,
        "Program quotas must be finite and inside v1 ceilings",
    )?;
    require(
        w.requirement != WorkspaceRequirement::None || q.storage_bytes == 0,
        "storage quota requires Workspace",
    )?;
    Ok(())
}

fn canonical_domain(value: &str) -> bool {
    value.contains('.')
        && actr_protocol::uri::ActrUri::from_domain(value).is_ok_and(|uri| uri.domain() == value)
}
fn compatible(range: &str, version: &Version) -> Result<(), PackError> {
    require(
        VersionReq::parse(range)
            .map_err(|e| invalid(e.to_string()))?
            .matches(version),
        "Host API/Keel API is incompatible with Program",
    )
}

/// Pre-sign validation for the official pack/build workflow. File hashes must
/// already be populated from these exact payloads (pack_actor_package does so).
pub fn validate_payloads(
    m: &ActorPackageManifest,
    payloads: &BTreeMap<String, Vec<u8>>,
    policy: &ProgramHostPolicy,
) -> Result<ProgramDeclaration, PackError> {
    m.validate()?;
    let p = program(m)?;
    require(
        Version::parse(&p.min_actrium_version).map_err(|e| invalid(e.to_string()))?
            <= policy.actrium_version,
        "Actrium version is too old",
    )?;
    compatible(&p.front_host_api, &policy.front_host_api)?;
    compatible(&p.service_host_api, &policy.service_host_api)?;
    compatible(&p.front.keel_api, &policy.keel_api)?;
    require(payloads.len() == m.files.len(), "payload set mismatch")?;
    let mut total = 0usize;
    for f in &m.files {
        let bytes = payloads
            .get(&f.path)
            .ok_or_else(|| invalid("missing payload"))?;
        total = total
            .checked_add(bytes.len())
            .ok_or_else(|| invalid("payload size overflow"))?;
        require(
            total <= crate::DEFAULT_MAX_VERIFIED_ENTRY_BYTES,
            "Program exceeds aggregate payload limit",
        )?;
        require(
            f.size == bytes.len() as u64 && f.sha256 == sha256_hex(bytes),
            "payload hash/size mismatch",
        )?;
        if f.role != "workload" {
            require(
                !executable_magic(bytes),
                "native binary or Wasm disguised as a Program resource",
            )?;
        }
    }
    let guest = std::str::from_utf8(&payloads[&p.front.guest_entry])
        .map_err(|_| invalid("Guest must be UTF-8 JavaScript"))?;
    require(
        !guest.trim().is_empty() && !guest.contains('\0'),
        "Guest entry is empty or invalid",
    )?;
    validate_guest(guest)?;
    let contract: FrontContract = serde_json::from_slice(&payloads[&p.front.contract_manifest])
        .map_err(|e| invalid(format!("invalid Front contract: {e}")))?;
    require(
        contract.schema == "actrium.front-contract/v1"
            && contract.service_contract == p.service.contract
            && contract.service_contract_digest == p.service.protocol_fingerprint
            && contract.capabilities == p.capabilities.front
            && contract.components == p.capabilities.components,
        "Front contract and Program declarations disagree",
    )?;
    let proto = std::str::from_utf8(&payloads[&p.service.contract])
        .map_err(|_| invalid("Service proto is not UTF-8"))?;
    actr_service_compat::Fingerprint::calculate_proto_semantic_fingerprint(proto)
        .map_err(|e| invalid(format!("invalid Service contract: {e}")))?;
    validate_component(&payloads[&m.workload.entry], &p.capabilities.service)?;
    Ok(p.clone())
}

/// Parse a single closed JS bundle. Static and dynamic module loading are
/// forbidden; only the versioned Guest SDK shims may remain as require calls.
/// This preflight complements, and does not replace, Keel's sealed sandbox.
fn validate_guest(source: &str) -> Result<(), PackError> {
    use oxc_ast::{
        AstKind,
        ast::{Argument, Expression},
    };
    use oxc_ast_visit::Visit;
    require(
        source.len() <= 4 * 1024 * 1024,
        "Guest exceeds v1 source limit",
    )?;
    let allocator = oxc_allocator::Allocator::default();
    let parsed = oxc_parser::Parser::new(&allocator, source, oxc_span::SourceType::cjs()).parse();
    require(
        parsed.errors.is_empty() && !parsed.panicked,
        "Guest JavaScript has syntax errors",
    )?;
    #[derive(Default)]
    struct ClosedBundle {
        denied: bool,
    }
    impl<'a> Visit<'a> for ClosedBundle {
        fn enter_node(&mut self, node: AstKind<'a>) {
            match node {
                AstKind::ImportDeclaration(_)
                | AstKind::ImportExpression(_)
                | AstKind::ExportAllDeclaration(_)
                | AstKind::ExportNamedDeclaration(_) => self.denied = true,
                AstKind::CallExpression(call) => {
                    if let Expression::Identifier(id) = &call.callee {
                        if ["eval", "Function", "importScripts"].contains(&id.name.as_str()) {
                            self.denied = true;
                        }
                        if id.name == "require" {
                            self.denied |= call.arguments.len() != 1
                                || !matches!(call.arguments.first(),
                                Some(Argument::StringLiteral(s)) if ["react", "react/jsx-runtime", "react-native", "keel/guest"].contains(&s.value.as_str()));
                        }
                    }
                }
                AstKind::NewExpression(call) => {
                    if matches!(&call.callee, Expression::Identifier(id) if id.name == "Function") {
                        self.denied = true;
                    }
                }
                _ => {}
            }
        }
    }
    let mut visitor = ClosedBundle::default();
    visitor.visit_program(&parsed.program);
    require(
        !visitor.denied,
        "Guest must be a closed bundle using only supported SDK shims",
    )
}
fn executable_magic(bytes: &[u8]) -> bool {
    [
        b"\0asm".as_slice(),
        b"\x7fELF",
        b"MZ",
        b"\xcf\xfa\xed\xfe",
        b"\xfe\xed\xfa\xcf",
        b"\xca\xfe\xba\xbe",
        b"#!",
    ]
    .iter()
    .any(|prefix| bytes.starts_with(prefix))
}

/// Inspect the actual Component interface graph, not producer custom sections.
/// No raw sockets, filesystem or HTTP WASI imports are supported. WASI cli
/// imports receive an empty environment, closed stdin and sink output in the
/// Program Host. External operations require a scoped Host broker.
pub fn validate_component(bytes: &[u8], capabilities: &[String]) -> Result<(), PackError> {
    wasmparser::Validator::new_with_features(wasmparser::WasmFeatures::all())
        .validate_all(bytes)
        .map_err(|e| invalid(format!("invalid Service Component: {e}")))?;
    let wit_parser::decoding::DecodedWasm::Component(resolve, world) =
        wit_parser::decoding::decode(bytes)
            .map_err(|e| invalid(format!("cannot decode Component ABI: {e}")))?
    else {
        return Err(invalid("Service must be a concrete Component"));
    };
    let world = &resolve.worlds[world];
    let mut expected = wit_parser::Resolve::default();
    let package = expected
        .push_source(
            "program-workload.wit",
            include_str!("../wit/program-workload.wit"),
        )
        .map_err(|e| invalid(e.to_string()))?;
    let expected_world = expected.packages[package].worlds["actr-workload-guest-v2"];
    let expected_world = &expected.worlds[expected_world];
    let (_, actual_export) = world
        .exports
        .iter()
        .find(|(key, _)| resolve.name_world_key(key) == SERVICE_ABI)
        .ok_or_else(|| invalid("Service does not export the supported workload ABI"))?;
    require(
        world.exports.iter().all(|(key, _)| {
            let name = resolve.name_world_key(key);
            name == SERVICE_ABI || name == "actr:workload/types@0.8.0"
        }),
        "unsupported Service export",
    )?;
    let (_, expected_export) = expected_world
        .exports
        .iter()
        .find(|(key, _)| expected.name_world_key(key) == SERVICE_ABI)
        .expect("bundled WIT exports workload");
    check_interface(&resolve, actual_export, &expected, expected_export, true)?;
    for (key, item) in &world.imports {
        let name = resolve.name_world_key(key);
        if name == SERVICE_HOST {
            let (_, expected_import) = expected_world
                .imports
                .iter()
                .find(|(key, _)| expected.name_world_key(key) == SERVICE_HOST)
                .expect("bundled WIT imports host");
            check_interface(&resolve, item, &expected, expected_import, false)?;
            if let wit_parser::WorldItem::Interface { id, .. } = item {
                for function in resolve.interfaces[*id].functions.keys() {
                    let cap = match function.as_str() {
                        "call" | "tell" | "call-raw" => Some("actor.call"),
                        "discover" => Some("actor.discover"),
                        "send-data-chunk" | "register-stream" | "unregister-stream" => {
                            Some("actor.stream")
                        }
                        "log-message" => Some("log"),
                        _ => None,
                    };
                    if let Some(cap) = cap {
                        require(
                            capabilities.iter().any(|c| c == cap),
                            "undeclared Service capability import",
                        )?;
                    }
                }
            }
        } else {
            require(
                name == "actr:workload/types@0.8.0" || wasi_allowed(&name),
                &format!("unsupported Service import `{name}`"),
            )?;
        }
    }
    Ok(())
}
fn wasi_allowed(name: &str) -> bool {
    // WASI 0.2 through the 0.2.12 interfaces shipped by wasmtime-wasi 48.0.7.
    // The controlled Program runtime never inherits environment or stdio.
    [
        "wasi:io/error@0.2.",
        "wasi:io/poll@0.2.",
        "wasi:io/streams@0.2.",
        "wasi:cli/stdin@0.2.",
        "wasi:cli/stdout@0.2.",
        "wasi:cli/stderr@0.2.",
        "wasi:cli/exit@0.2.",
        "wasi:cli/environment@0.2.",
        "wasi:cli/terminal-input@0.2.",
        "wasi:cli/terminal-output@0.2.",
        "wasi:cli/terminal-stdin@0.2.",
        "wasi:cli/terminal-stdout@0.2.",
        "wasi:cli/terminal-stderr@0.2.",
        "wasi:clocks/monotonic-clock@0.2.",
        "wasi:clocks/wall-clock@0.2.",
        "wasi:random/random@0.2.",
        "wasi:random/insecure-seed@0.2.",
    ]
    .iter()
    .any(|prefix| {
        name.strip_prefix(prefix).is_some_and(|patch| {
            patch
                .parse::<u8>()
                .is_ok_and(|n| n <= 12 && n.to_string() == patch)
        })
    })
}
fn check_interface(
    a: &wit_parser::Resolve,
    ai: &wit_parser::WorldItem,
    e: &wit_parser::Resolve,
    ei: &wit_parser::WorldItem,
    complete: bool,
) -> Result<(), PackError> {
    let (
        wit_parser::WorldItem::Interface { id: ai, .. },
        wit_parser::WorldItem::Interface { id: ei, .. },
    ) = (ai, ei)
    else {
        return Err(invalid("ABI must be an interface"));
    };
    let (af, ef) = (&a.interfaces[*ai].functions, &e.interfaces[*ei].functions);
    require(
        !complete || af.len() == ef.len(),
        "Service ABI exports are incomplete",
    )?;
    for (name, f) in af {
        let expected = ef
            .get(name)
            .ok_or_else(|| invalid(format!("unsupported ABI function `{name}`")))?;
        require(
            function_shape(a, f)? == function_shape(e, expected)?,
            "Component function signature differs from supported WIT",
        )?;
    }
    Ok(())
}
fn function_shape(r: &wit_parser::Resolve, f: &wit_parser::Function) -> Result<String, PackError> {
    let params = f
        .params
        .iter()
        .map(|p| type_shape(r, &p.ty, 0))
        .collect::<Result<Vec<_>, _>>()?;
    let result = f.result.as_ref().map(|t| type_shape(r, t, 0)).transpose()?;
    Ok(format!("{:?}:{params:?}:{result:?}", f.kind))
}
fn type_shape(
    r: &wit_parser::Resolve,
    t: &wit_parser::Type,
    depth: usize,
) -> Result<String, PackError> {
    use wit_parser::{Type, TypeDefKind as K};
    require(depth < 64, "recursive/excessively deep ABI type")?;
    let Type::Id(id) = t else {
        return Ok(format!("{t:?}"));
    };
    let next = |t| type_shape(r, t, depth + 1);
    Ok(match &r.types[*id].kind {
        K::Type(t) => next(t)?,
        K::List(t) => format!("list({})", next(t)?),
        K::Option(t) => format!("option({})", next(t)?),
        K::Record(v) => format!(
            "record{:?}",
            v.fields
                .iter()
                .map(|f| Ok((f.name.clone(), next(&f.ty)?)))
                .collect::<Result<Vec<_>, PackError>>()?
        ),
        K::Variant(v) => format!(
            "variant{:?}",
            v.cases
                .iter()
                .map(|f| Ok((f.name.clone(), f.ty.as_ref().map(next).transpose()?)))
                .collect::<Result<Vec<_>, PackError>>()?
        ),
        K::Enum(v) => format!(
            "enum{:?}",
            v.cases.iter().map(|c| &c.name).collect::<Vec<_>>()
        ),
        K::Flags(v) => format!(
            "flags{:?}",
            v.flags.iter().map(|c| &c.name).collect::<Vec<_>>()
        ),
        K::Tuple(v) => format!(
            "tuple{:?}",
            v.types.iter().map(next).collect::<Result<Vec<_>, _>>()?
        ),
        K::Result(v) => format!(
            "result({:?},{:?})",
            v.ok.as_ref().map(next).transpose()?,
            v.err.as_ref().map(next).transpose()?
        ),
        _ => return Err(invalid("unsupported Actr ABI type")),
    })
}

pub(crate) fn read_payloads(
    bytes: &[u8],
    inspected: &InspectedActorPackage,
) -> Result<BTreeMap<String, Vec<u8>>, PackError> {
    inspected
        .manifest
        .files
        .iter()
        .map(|f| {
            Ok((
                f.path.clone(),
                crate::read_actor_package_file(
                    bytes,
                    inspected,
                    &f.path,
                    crate::DEFAULT_MAX_VERIFIED_ENTRY_BYTES,
                )?,
            ))
        })
        .collect()
}

pub fn verify_program_release(
    bytes: &[u8],
    public_key: &VerifyingKey,
    expected_digest: &str,
    policy: &ProgramHostPolicy,
) -> Result<VerifiedProgramRelease, PackError> {
    let digest = format!("sha256:{}", sha256_hex(bytes));
    require(expected_digest == digest, "Program package digest mismatch")?;
    let inspected = crate::verify_actor_package(bytes, public_key)?;
    let payloads = read_payloads(bytes, &inspected)?;
    let declaration = validate_payloads(&inspected.manifest, &payloads, policy)?;
    Ok(VerifiedProgramRelease {
        manifest: inspected.manifest,
        declaration,
        digest,
        payloads,
    })
}

/// Adapter for existing Service Host/Issuer APIs. The original signed manifest
/// bytes are retained by verification; this is never re-signed or forwarded as
/// if it were the Publisher's manifest.
pub fn runtime_manifest(m: &ActorPackageManifest) -> Result<crate::PackageManifest, PackError> {
    m.validate()?;
    let p = program(m)?;
    let binary = m.workload_file()?;
    Ok(crate::PackageManifest {
        publisher: m.actor.publisher.clone(),
        name: m.actor.name.clone(),
        version: m.actor.version.clone(),
        binary: crate::BinaryEntry {
            path: binary.path.clone(),
            target: "wasm32-wasip2".into(),
            hash: binary.sha256.clone(),
            size: Some(binary.size),
            kind: Some(crate::BinaryKind::Component),
        },
        signature_algorithm: m.signature.algorithm.clone(),
        signing_key_id: Some(m.signature.key_id.clone()),
        resources: m
            .files
            .iter()
            .filter(|f| f.path != binary.path && f.path != p.service.contract && f.role != "lock")
            .map(|f| crate::ResourceEntry {
                path: f.path.clone(),
                hash: f.sha256.clone(),
            })
            .collect(),
        proto_files: m
            .files
            .iter()
            .filter(|f| f.path == p.service.contract)
            .map(|f| crate::ProtoFileEntry {
                name: f.path.rsplit('/').next().unwrap_or(&f.path).into(),
                path: f.path.clone(),
                hash: f.sha256.clone(),
            })
            .collect(),
        lock_file: m
            .files
            .iter()
            .find(|f| f.role == "lock")
            .map(|f| crate::LockFileEntry {
                path: f.path.clone(),
                hash: f.sha256.clone(),
            }),
        metadata: crate::ManifestMetadata {
            description: m
                .extensions
                .actrium
                .as_ref()
                .and_then(|x| x.description.clone()),
            license: None,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn guest_dependencies_are_parsed_not_matched_as_text() {
        assert!(
            validate_guest("const text = 'import(\"https://example.test\")'; // require('fs')")
                .is_ok()
        );
        for source in [
            "import('https://example.test/guest.js')",
            "require('fs')",
            "require(name)",
            "eval('1')",
            "new Function('return 1')",
            "const = ;",
            "import x from './other.js'",
        ] {
            assert!(validate_guest(source).is_err(), "accepted {source}");
        }
        assert!(validate_guest("require('react');require('keel/guest');").is_ok());
    }
    #[test]
    fn formal_program_wit_tracks_the_runtime_contract() {
        assert_eq!(
            include_str!("../wit/program-workload.wit"),
            include_str!("../../actr-framework/wit-v2/actr-workload.wit")
        );
    }
    #[test]
    fn core_module_and_undeclared_abi_are_rejected() {
        assert!(validate_component(b"\0asm\x01\0\0\0", &[]).is_err());
        assert!(validate_component(b"\0asm\x0d\0\x01\0", &[]).is_err());
    }
}

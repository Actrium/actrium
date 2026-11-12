//! Payload checks for supported Workload Profiles.

use semver::{Version, VersionReq};
use serde::Deserialize;
use wasmparser::{
    Validator, WasmFeatures,
    component_types::{ComponentAnyTypeId, ComponentEntityType},
};

use crate::{ActorPackageManifest, KEEL_GUEST_PROFILE_V1, KeelGuestDescriptor, PackError};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct KeelCapabilityManifest {
    schema_version: u32,
    keel_api: String,
    guest_entry: String,
    imports: Vec<String>,
}

pub(crate) fn validate_workload(
    manifest: &ActorPackageManifest,
    read: impl Fn(&str) -> Result<Vec<u8>, PackError>,
) -> Result<(), PackError> {
    let workload = &manifest.workload;
    match workload.profile.as_str() {
        "actr.component/1" => validate_component(&read(&workload.entry)?),
        KEEL_GUEST_PROFILE_V1 => {
            let keel = workload.keel.as_ref().ok_or_else(|| {
                PackError::InvalidPackage("keel.guest/1 requires [workload.keel]".into())
            })?;
            if keel.runtime == "js" {
                validate_single_entry(manifest)?;
                validate_js_guest(manifest, keel, read)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

fn validate_single_entry(manifest: &ActorPackageManifest) -> Result<(), PackError> {
    require(
        manifest
            .files
            .iter()
            .filter(|file| file.role == "workload")
            .count()
            == 1,
        format!(
            "{} requires exactly one [[files]] entry with role = \"workload\"",
            manifest.workload.profile
        ),
    )
}

fn validate_js_guest(
    manifest: &ActorPackageManifest,
    keel: &KeelGuestDescriptor,
    read: impl Fn(&str) -> Result<Vec<u8>, PackError>,
) -> Result<(), PackError> {
    let entry = &manifest.workload.entry;
    let guest = read(entry)?;
    require(
        std::str::from_utf8(&guest).is_ok_and(|js| !js.trim().is_empty()),
        "the Keel Guest must be a nonempty UTF-8 bundle",
    )?;
    if let Some(dependency) = manifest.dependencies.get("service") {
        let contract_path = dependency.contract.as_deref().ok_or_else(|| {
            PackError::InvalidPackage(
                "[dependencies.service] must declare its Proto contract".into(),
            )
        })?;
        require(
            dependency.required
                && !dependency.actor.trim().is_empty()
                && contract_path.ends_with(".proto"),
            "[dependencies.service] must identify a required Service and its Proto contract",
        )?;
        VersionReq::parse(&dependency.version)
            .map_err(|error| invalid("dependencies.service.version", error))?;
        let contract = read(contract_path)?;
        let source =
            std::str::from_utf8(&contract).map_err(|error| invalid("Service Proto", error))?;
        actr_service_compat::validate_self_contained_rpc_proto(source)
            .map_err(|error| invalid(&format!("Service Proto `{contract_path}`"), error))?;
    }
    let Some(path) = &keel.capabilities else {
        return Ok(());
    };
    let capabilities: KeelCapabilityManifest = serde_json::from_slice(&read(path)?)
        .map_err(|error| invalid("Keel capability manifest", error))?;
    require(
        capabilities.schema_version == 1 && capabilities.guest_entry == *entry,
        "the Keel capability manifest must use schemaVersion 1 and match the Workload entry",
    )?;
    Version::parse(&capabilities.keel_api)
        .or_else(|_| Version::parse(&format!("{}.0", capabilities.keel_api)))
        .map_err(|error| invalid("capability manifest keelApi", error))?;
    if capabilities
        .imports
        .iter()
        .any(|name| name == "host:service")
    {
        require(
            manifest.dependencies.contains_key("service"),
            "host:service requires [dependencies.service]",
        )?;
    }
    Ok(())
}

fn require(condition: bool, message: impl Into<String>) -> Result<(), PackError> {
    if condition {
        Ok(())
    } else {
        Err(PackError::InvalidPackage(message.into()))
    }
}

fn validate_component(bytes: &[u8]) -> Result<(), PackError> {
    const WORLD: &str = "actr-workload-guest-v2";
    const WORKLOAD: &str = "actr:workload/workload@0.8.0";

    let mut resolve = wit_parser::Resolve::default();
    let package = resolve
        .push_str(
            "actr-workload.wit",
            actr_framework::guest::WASM_WORKLOAD_WIT,
        )
        .map_err(|error| invalid("SDK workload WIT", error))?;
    let world = resolve
        .select_world(&[package], Some(WORLD))
        .map_err(|error| invalid("SDK workload world", error))?;
    // Compare the exported interface type. WASI imports are checked
    // separately by the runtime when it links the actual component.
    let contract =
        wit_component::metadata::encode(&resolve, world, wit_component::StringEncoding::UTF8, None)
            .map_err(|error| invalid("SDK workload type", error))?;
    let mut validator = Validator::new_with_features(WasmFeatures::all());
    let expected = validator
        .validate_all(&contract)
        .map_err(|error| invalid("SDK workload type", error))?;
    let types = expected.as_ref();
    let Some(ComponentEntityType::Type {
        referenced: ComponentAnyTypeId::Component(outer),
        ..
    }) = types.component_item_for_export(WORLD).map(|item| item.ty)
    else {
        return Err(PackError::InvalidPackage(
            "missing SDK workload world type".into(),
        ));
    };
    let Some(ComponentEntityType::Component(world_type)) = types[outer]
        .exports
        .get(&resolve.id_of_name(package, WORLD))
        .map(|item| item.ty)
    else {
        return Err(PackError::InvalidPackage(
            "missing SDK workload component type".into(),
        ));
    };
    let required = types[world_type]
        .exports
        .get(WORKLOAD)
        .ok_or_else(|| PackError::InvalidPackage("missing SDK workload interface type".into()))?;

    validator.reset();
    let actual = validator
        .validate_all(bytes)
        .map_err(|error| invalid("Service WASM", error))?;
    let exported = actual.component_item_for_export(WORKLOAD).ok_or_else(|| {
        PackError::InvalidPackage(format!("the Service component must export {WORKLOAD}"))
    })?;
    require(
        ComponentEntityType::is_subtype_of(&exported.ty, actual.as_ref(), &required.ty, types),
        format!("the Service export {WORKLOAD} does not implement the SDK workload ABI"),
    )
}

fn invalid(field: &str, error: impl std::fmt::Display) -> PackError {
    PackError::InvalidPackage(format!("invalid {field}: {error}"))
}

//! Verify the self-contained OCI image graph against the signed package.

use std::collections::BTreeSet;
use std::fs::File;
use std::io::Read;
use std::path::Path;

use flate2::read::MultiGzDecoder;
use serde::Deserialize;

use super::{OCI_AMD64_PLATFORM, OCI_ARM64_PLATFORM, invalid, parse_digest, require};
use crate::{ActorPackageManifest, PackError, egress};

const INDEX: &str = "application/vnd.oci.image.index.v1+json";
const MANIFEST: &str = "application/vnd.oci.image.manifest.v1+json";
const CONFIG: &str = "application/vnd.oci.image.config.v1+json";
const TAR: &str = "application/vnd.oci.image.layer.v1.tar";
const GZIP: &str = "application/vnd.oci.image.layer.v1.tar+gzip";
const MAX_METADATA: u64 = 1024 * 1024;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Index {
    schema_version: u32,
    media_type: Option<String>,
    manifests: Vec<Descriptor>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Descriptor {
    media_type: String,
    digest: String,
    size: u64,
    #[serde(default)]
    urls: Vec<String>,
    data: Option<String>,
    platform: Option<Platform>,
}

#[derive(Deserialize)]
struct Layout {
    #[serde(rename = "imageLayoutVersion")]
    version: String,
}

#[derive(Deserialize)]
struct Platform {
    architecture: String,
    os: String,
    variant: Option<String>,
}

impl Platform {
    fn matches(&self, platform: &str) -> bool {
        if self.os != "linux" {
            return false;
        }
        let variant = self.variant.as_deref().unwrap_or_default();
        match (platform, self.architecture.as_str()) {
            (OCI_AMD64_PLATFORM, "amd64") => variant.is_empty(),
            (OCI_ARM64_PLATFORM, "arm64") => matches!(variant, "" | "v8"),
            _ => false,
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ImageManifest {
    schema_version: u32,
    media_type: Option<String>,
    config: Descriptor,
    layers: Vec<Descriptor>,
}

#[derive(Deserialize)]
struct ImageConfig {
    #[serde(flatten)]
    platform: Platform,
    rootfs: RootFs,
    #[serde(default)]
    config: ProcessConfig,
}

#[derive(Default, Deserialize)]
struct ProcessConfig {
    #[serde(rename = "User", default)]
    user: String,
}

#[derive(Deserialize)]
struct RootFs {
    #[serde(rename = "type")]
    kind: String,
    diff_ids: Vec<String>,
}

pub(super) fn verify(
    manifest: &ActorPackageManifest,
    folder: &Path,
    unpacked_limit: u64,
) -> Result<(), PackError> {
    let workload = manifest
        .workload
        .oci
        .as_ref()
        .ok_or_else(|| invalid("missing OCI metadata"))?;
    let layout_dir = manifest.workload.entry.strip_suffix("index.json").unwrap();
    let layout_path = format!("{layout_dir}oci-layout");
    let layout: Layout = json(folder, &layout_path)?;
    require(layout.version == "1.0.0", "unsupported OCI layout version")?;

    let index: Index = json(folder, &manifest.workload.entry)?;
    require(
        index.schema_version == 2
            && index.media_type.as_deref().is_none_or(|kind| kind == INDEX)
            && index.manifests.len() == 1,
        "OCI index must contain exactly one image manifest",
    )?;
    let descriptor = &index.manifests[0];
    require(
        descriptor.media_type == MANIFEST
            && descriptor.digest == workload.digest
            && descriptor
                .platform
                .as_ref()
                .is_none_or(|platform| platform.matches(&workload.platform)),
        "OCI index descriptor does not match the signed manifest digest and platform",
    )?;
    let mut image_files = BTreeSet::from([layout_path]);
    let image_path = blob(descriptor, manifest, layout_dir, &mut image_files)?;
    let image: ImageManifest = json(folder, &image_path)?;
    require(
        image.schema_version == 2
            && image
                .media_type
                .as_deref()
                .is_none_or(|kind| kind == MANIFEST)
            && image.config.media_type == CONFIG,
        "invalid OCI image manifest or config media type",
    )?;
    let config_path = blob(&image.config, manifest, layout_dir, &mut image_files)?;
    let config: ImageConfig = json(folder, &config_path)?;
    require(
        config.platform.matches(&workload.platform),
        "OCI config platform must match the signed workload platform",
    )?;
    require(
        matches!(config.config.user.as_str(), "" | "0" | "0:0"),
        "OCI image user must be unset, 0 or 0:0",
    )?;
    require(
        config.rootfs.kind == "layers" && config.rootfs.diff_ids.len() == image.layers.len(),
        "OCI config must provide one DiffID per image layer",
    )?;
    let mut remaining = unpacked_limit;
    for (descriptor, diff_id) in image.layers.iter().zip(&config.rootfs.diff_ids) {
        let layer_path = blob(descriptor, manifest, layout_dir, &mut image_files)?;
        let file = File::open(folder.join(layer_path))?;
        let reader: Box<dyn Read> = match descriptor.media_type.as_str() {
            TAR => Box::new(file),
            GZIP => Box::new(MultiGzDecoder::new(file)),
            _ => return Err(invalid("unsupported OCI layer media type")),
        };
        let (actual, size) = super::archive::hash_and_copy(reader, std::io::sink(), remaining)?;
        require(
            hex::encode(actual) == parse_digest(diff_id)?,
            "OCI layer DiffID mismatch",
        )?;
        remaining -= size;
    }

    for declared in &manifest.files {
        require(
            (declared.role == "image") == image_files.contains(&declared.path),
            "OCI image files must match the complete image descriptor graph",
        )?;
    }
    require(
        manifest
            .files
            .iter()
            .any(|file| file.path == format!("{layout_dir}oci-layout") && file.role == "image"),
        "oci-layout must be declared as an image file",
    )?;

    egress::validate_privacy_contract(manifest, |path| read_metadata(folder, path))?;
    if let Some(path) = manifest.rpc_contract()? {
        let bytes = read_metadata(folder, path)?;
        let source = std::str::from_utf8(&bytes)
            .map_err(|error| invalid(format!("invalid OCI Proto at {path}: {error}")))?;
        actr_service_compat::spec_builder::validate_oci_rpc_routes(source)
            .map_err(|error| invalid(error.to_string()))?;
    }
    Ok(())
}

fn blob(
    descriptor: &Descriptor,
    manifest: &ActorPackageManifest,
    layout_dir: &str,
    image_files: &mut BTreeSet<String>,
) -> Result<String, PackError> {
    require(
        descriptor.urls.is_empty() && descriptor.data.is_none(),
        "OCI blobs must be provided by the archive",
    )?;
    let hash = parse_digest(&descriptor.digest)?;
    let path = format!("{layout_dir}blobs/sha256/{hash}");
    require(
        manifest.files.iter().any(|entry| {
            entry.path == path
                && entry.role == "image"
                && entry.size == descriptor.size
                && entry.sha256 == hash
                && entry.media_type == descriptor.media_type
        }),
        "OCI descriptor does not match a declared blob",
    )?;
    image_files.insert(path.clone());
    Ok(path)
}

fn json<T: serde::de::DeserializeOwned>(folder: &Path, path: &str) -> Result<T, PackError> {
    serde_json::from_slice(&read_metadata(folder, path)?)
        .map_err(|error| invalid(format!("invalid OCI JSON at {path}: {error}")))
}

pub(super) fn read_metadata(folder: &Path, path: &str) -> Result<Vec<u8>, PackError> {
    let mut bytes = Vec::new();
    File::open(folder.join(path))?
        .take(MAX_METADATA + 1)
        .read_to_end(&mut bytes)?;
    require(
        bytes.len() as u64 <= MAX_METADATA,
        "OCI metadata exceeds 1 MiB",
    )?;
    Ok(bytes)
}

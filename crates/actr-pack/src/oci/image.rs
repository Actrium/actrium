//! Verify the self-contained OCI image graph against the signed package.

use std::collections::BTreeSet;
use std::fs::File;
use std::io::Read;
use std::path::Path;

use flate2::read::MultiGzDecoder;
use serde::Deserialize;

use super::{
    OCI_AMD64_PLATFORM, OCI_ARM64_PLATFORM, invalid, layout::ImageLayout, parse_digest, require,
};
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
) -> Result<ImageLayout, PackError> {
    let workload = manifest
        .workload
        .oci
        .as_ref()
        .ok_or_else(|| invalid("missing OCI metadata"))?;
    let layout = ImageLayout::index(folder.join(&manifest.workload.entry))?;
    let metadata: Layout = json(&layout, "oci-layout")?;
    require(
        metadata.version == "1.0.0",
        "unsupported OCI layout version",
    )?;

    let index: Index = json(&layout, "index.json")?;
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
    let mut image_files = BTreeSet::from(["index.json".to_owned(), "oci-layout".to_owned()]);
    let (image_path, _) = blob(descriptor, &layout, &mut image_files)?;
    let image: ImageManifest = json(&layout, &image_path)?;
    require(
        image.schema_version == 2
            && image
                .media_type
                .as_deref()
                .is_none_or(|kind| kind == MANIFEST)
            && image.config.media_type == CONFIG,
        "invalid OCI image manifest or config media type",
    )?;
    let (config_path, _) = blob(&image.config, &layout, &mut image_files)?;
    let config: ImageConfig = json(&layout, &config_path)?;
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
        let (layer_path, layer_hash) = blob(descriptor, &layout, &mut image_files)?;
        let (actual, size) = match descriptor.media_type.as_str() {
            TAR => (layer_hash, descriptor.size),
            GZIP => super::archive::hash_and_copy(
                MultiGzDecoder::new(layout.open(&layer_path)?),
                std::io::sink(),
                remaining,
            )?,
            _ => return Err(invalid("unsupported OCI layer media type")),
        };
        require(size <= remaining, "OCI unpacked image exceeds size limit")?;
        require(
            hex::encode(actual) == parse_digest(diff_id)?,
            "OCI layer DiffID mismatch",
        )?;
        remaining -= size;
    }

    require(
        layout.files().all(|name| image_files.contains(name)),
        "OCI image tar files must match the complete image descriptor graph",
    )?;

    egress::validate_privacy_contract(manifest, |path| read_metadata(folder, path))?;
    if let Some(path) = manifest.rpc_contract()? {
        let bytes = read_metadata(folder, path)?;
        let source = std::str::from_utf8(&bytes)
            .map_err(|error| invalid(format!("invalid OCI Proto at {path}: {error}")))?;
        actr_service_compat::spec_builder::validate_oci_rpc_routes(source)
            .map_err(|error| invalid(error.to_string()))?;
    }
    Ok(layout)
}

fn blob(
    descriptor: &Descriptor,
    layout: &ImageLayout,
    image_files: &mut BTreeSet<String>,
) -> Result<(String, [u8; 32]), PackError> {
    require(
        descriptor.urls.is_empty() && descriptor.data.is_none(),
        "OCI blobs must be provided by the tar",
    )?;
    let hash = parse_digest(&descriptor.digest)?;
    let path = format!("blobs/sha256/{hash}");
    let source = layout.open(&path)?;
    require(
        source.limit() == descriptor.size,
        "OCI descriptor size mismatch",
    )?;
    let (actual, size) = super::archive::hash_and_copy(source, std::io::sink(), descriptor.size)?;
    require(
        size == descriptor.size && hex::encode(actual) == hash,
        "OCI descriptor digest mismatch",
    )?;
    image_files.insert(path.clone());
    Ok((path, actual))
}

fn json<T: serde::de::DeserializeOwned>(layout: &ImageLayout, path: &str) -> Result<T, PackError> {
    let mut reader = layout.open(path)?;
    require(reader.limit() <= MAX_METADATA, "OCI metadata exceeds 1 MiB")?;
    let mut bytes = Vec::new();
    reader.read_to_end(&mut bytes)?;
    serde_json::from_slice(&bytes)
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

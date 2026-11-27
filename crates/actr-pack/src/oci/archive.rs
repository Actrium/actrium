//! File-backed OCI package verification and writing.

use std::collections::BTreeSet;
use std::fs::{self, File};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use ed25519_dalek::{Signer, SigningKey, VerifyingKey};
use sha2::{Digest, Sha256};
use tempfile::TempDir;
use zip::read::{ArchiveOffset, Config};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use super::{OCI_PROFILE, image, invalid, require};
use crate::actor_package::InspectedActorPackage;
use crate::util::read_zip_entry_bounded;
use crate::{
    ActorPackageManifest, PackError, RuntimePackageManifest, ValidatedActorIdentity,
    VerifiedRuntimePackage, runtime::parse_runtime_manifest,
};

const MAX_MANIFEST: usize = 1024 * 1024;
const MAX_DIRECTORY_BYTES: u64 = 4 * 1024 * 1024;
const MAX_ENTRY_COUNT: u64 = (crate::DEFAULT_MAX_VERIFIED_ENTRIES + 2) as u64;
const MAX_ENTRY_METADATA: u64 = 4096;
const END_SIZE: u64 = 22;
const ZIP64_LOCATOR_SIZE: u64 = 20;
const ZIP64_END_SIZE: u64 = 56;

#[derive(Clone, Copy, Debug)]
pub struct OciLimits {
    pub archive_bytes: u64,
    pub unpacked_layer_bytes: u64,
}

impl Default for OciLimits {
    fn default() -> Self {
        Self {
            archive_bytes: 8 * 1024 * 1024 * 1024,
            unpacked_layer_bytes: 16 * 1024 * 1024 * 1024,
        }
    }
}

pub struct OciArchive {
    archive: ZipArchive<File>,
    metadata: InspectedActorPackage,
    digest: [u8; 32],
    limits: OciLimits,
}

impl OciArchive {
    /// Capture input in an engine-owned, seekable file before interpreting it.
    pub fn snapshot(source: impl Read, limits: OciLimits) -> Result<Self, PackError> {
        let mut file = tempfile::tempfile()?;
        let (digest, _) = hash_and_copy(source, &mut file, limits.archive_bytes)?;
        file.rewind()?;
        let mut archive = open_zip(file)?;
        let (manifest, manifest_raw) = read_manifest(&mut archive)?;
        let RuntimePackageManifest::V2(manifest) = manifest else {
            return Err(invalid("OCI requires a versioned package"));
        };
        require(
            manifest.workload.profile == OCI_PROFILE,
            "expected actr.oci/1",
        )?;
        let signature_raw = read_zip_entry_bounded(&mut archive, "manifest.sig", 64)?;
        let metadata = InspectedActorPackage {
            manifest: *manifest,
            manifest_raw,
            signature_raw,
        };
        Ok(Self {
            archive,
            metadata,
            digest,
            limits,
        })
    }

    /// Unverified signed metadata for the trust provider's key selection.
    pub fn metadata(&self) -> &InspectedActorPackage {
        &self.metadata
    }

    /// Authenticate metadata before copying any payload to a private staging directory.
    pub fn verify(
        mut self,
        public_key: &VerifyingKey,
        staging_parent: Option<&Path>,
    ) -> Result<VerifiedOciPackage, PackError> {
        self.metadata.verify_signature(public_key)?;
        let manifest = RuntimePackageManifest::V2(Box::new(self.metadata.manifest));
        ValidatedActorIdentity::from_actor_type(manifest.actor_type())
            .map_err(|error| invalid(error.to_string()))?;
        let RuntimePackageManifest::V2(package) = &manifest else {
            unreachable!();
        };
        verify_entries(&mut self.archive, package)?;
        let staging = match staging_parent {
            Some(parent) => tempfile::tempdir_in(parent)?,
            None => tempfile::tempdir()?,
        };

        let mut remaining = self.limits.archive_bytes;
        for entry in &package.files {
            let source = self.archive.by_name(&entry.path)?;
            require(
                source.size() == entry.size,
                "signed ZIP entry size mismatch",
            )?;
            require(entry.size <= remaining, "OCI payload size limit exceeded")?;
            let path = staging.path().join(&entry.path);
            fs::create_dir_all(path.parent().unwrap())?;
            let output = File::options().create_new(true).write(true).open(path)?;
            let (hash, size) = hash_and_copy(source, output, entry.size)?;
            if size != entry.size || hex::encode(hash) != entry.sha256.to_ascii_lowercase() {
                return Err(PackError::ResourceHashMismatch {
                    path: entry.path.clone(),
                });
            }
            remaining -= size;
        }
        image::verify(package, staging.path(), self.limits.unpacked_layer_bytes)?;
        Ok(VerifiedOciPackage {
            verified: VerifiedRuntimePackage {
                manifest,
                manifest_raw: self.metadata.manifest_raw,
                sig_raw: self.metadata.signature_raw,
                signing_public_key: public_key.to_bytes(),
            },
            staging,
            digest: self.digest,
        })
    }
}

#[derive(Debug)]
pub struct VerifiedOciPackage {
    verified: VerifiedRuntimePackage,
    staging: TempDir,
    digest: [u8; 32],
}

impl VerifiedOciPackage {
    pub fn verification(&self) -> &VerifiedRuntimePackage {
        &self.verified
    }

    pub fn manifest(&self) -> &ActorPackageManifest {
        let RuntimePackageManifest::V2(manifest) = &self.verified.manifest else {
            unreachable!();
        };
        manifest
    }

    pub fn archive_digest(&self) -> [u8; 32] {
        self.digest
    }

    pub fn read_metadata(&self, path: &str) -> Result<Vec<u8>, PackError> {
        require(
            self.manifest().files.iter().any(|entry| entry.path == path),
            "undeclared OCI file",
        )?;
        image::read_metadata(self.staging.path(), path)
    }

    pub fn privacy_contract(
        &self,
    ) -> Result<Option<crate::egress::PackagePrivacyContract>, PackError> {
        crate::egress::validate_privacy_contract(self.manifest(), |path| self.read_metadata(path))
    }

    pub fn layout_directory(&self) -> PathBuf {
        let entry = Path::new(&self.manifest().workload.entry);
        self.staging.path().join(entry.parent().unwrap())
    }
}

/// Inspect metadata without allocating a buffer for the archive contents.
pub fn read_file_manifest(path: &Path) -> Result<RuntimePackageManifest, PackError> {
    read_open_file_manifest(&mut File::open(path)?)
}

/// Inspect an already-open archive without reopening its path.
pub fn read_open_file_manifest(file: &mut File) -> Result<RuntimePackageManifest, PackError> {
    let mut archive = open_zip(file)?;
    read_manifest(&mut archive).map(|(manifest, _)| manifest)
}

fn read_manifest(
    archive: &mut ZipArchive<impl Read + Seek>,
) -> Result<(RuntimePackageManifest, Vec<u8>), PackError> {
    let raw = read_zip_entry_bounded(archive, "manifest.toml", MAX_MANIFEST)?;
    let source = std::str::from_utf8(&raw).map_err(|error| invalid(error.to_string()))?;
    Ok((parse_runtime_manifest(source)?, raw))
}

/// Pack a local layout and its declared metadata from private snapshots.
pub fn pack_oci_package<W: Write + Seek>(
    mut manifest: ActorPackageManifest,
    payload_root: &Path,
    signing_key: &SigningKey,
    writer: W,
    limits: OciLimits,
) -> Result<W, PackError> {
    require(
        manifest.workload.profile == OCI_PROFILE,
        "expected actr.oci/1",
    )?;
    manifest.signature.algorithm = "ed25519".into();
    manifest.signature.key_id = crate::compute_key_id(&signing_key.verifying_key().to_bytes());
    manifest.validate()?;

    let root = payload_root.canonicalize()?;
    let staging = tempfile::tempdir()?;
    let mut remaining = limits.archive_bytes;
    for entry in &mut manifest.files {
        let mut source = root.clone();
        for component in Path::new(&entry.path).components() {
            source.push(component);
            require(
                !fs::symlink_metadata(&source)?.is_symlink(),
                "OCI payload path contains a symlink",
            )?;
        }
        require(
            source.is_file() && source.canonicalize()?.starts_with(&root),
            "OCI payload must be a regular file within the payload root",
        )?;
        let target = staging.path().join(&entry.path);
        fs::create_dir_all(target.parent().unwrap())?;
        let (hash, size) = hash_and_copy(
            File::open(&source)?,
            File::options().create_new(true).write(true).open(target)?,
            remaining,
        )?;
        entry.sha256 = hex::encode(hash);
        entry.size = size;
        remaining -= size;
    }
    image::verify(&manifest, staging.path(), limits.unpacked_layer_bytes)?;
    let manifest_raw = manifest.to_toml()?;
    require(
        manifest_raw.len() <= MAX_MANIFEST,
        "OCI manifest exceeds 1 MiB",
    )?;
    let signature = signing_key.sign(manifest_raw.as_bytes());

    let mut zip = ZipWriter::new(writer);
    let stored = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    zip.start_file("manifest.toml", stored)?;
    zip.write_all(manifest_raw.as_bytes())?;
    zip.start_file("manifest.sig", stored)?;
    zip.write_all(&signature.to_bytes())?;
    for entry in &manifest.files {
        zip.start_file(
            &entry.path,
            stored.large_file(entry.size >= u32::MAX as u64),
        )?;
        io::copy(&mut File::open(staging.path().join(&entry.path))?, &mut zip)?;
    }
    let mut writer = zip.finish()?;
    require(
        writer.seek(SeekFrom::End(0))? <= limits.archive_bytes,
        "OCI archive size limit exceeded",
    )?;
    Ok(writer)
}

fn verify_entries(
    archive: &mut ZipArchive<File>,
    manifest: &ActorPackageManifest,
) -> Result<(), PackError> {
    let declared: BTreeSet<_> = manifest
        .files
        .iter()
        .map(|entry| entry.path.as_str())
        .collect();
    require(
        archive.len() == declared.len() + 2,
        "OCI archive entries do not match its manifest",
    )?;
    require(
        !archive.has_overlapping_entries()?,
        "OCI ZIP entries overlap",
    )?;
    let central_start = archive.central_directory_start();
    for index in 0..archive.len() {
        let entry = archive.by_index(index)?;
        let name = entry.name()?;
        require(
            matches!(name.as_ref(), "manifest.toml" | "manifest.sig")
                || declared.contains(name.as_ref()),
            "undeclared OCI ZIP entry",
        )?;
        require(
            entry.is_file()
                && !entry.encrypted()
                && entry.compression() == CompressionMethod::Stored
                && entry
                    .unix_mode()
                    .is_none_or(|mode| matches!(mode & 0o170000, 0 | 0o100000))
                && entry
                    .data_start()
                    .and_then(|offset| offset.checked_add(entry.compressed_size()))
                    .is_some_and(|end| end <= central_start),
            "OCI ZIP entries must be unencrypted regular STORE files",
        )?;
    }
    Ok(())
}

fn open_zip<R: Read + Seek>(mut file: R) -> Result<ZipArchive<R>, PackError> {
    let (start, count) = preflight_directory(&mut file)?;
    let archive = ZipArchive::with_config(
        Config {
            archive_offset: ArchiveOffset::Known(0),
        },
        file,
    )?;
    require(
        archive.central_directory_start() == start && archive.len() as u64 == count,
        "OCI ZIP directory contains duplicate or inconsistent entries",
    )?;
    Ok(archive)
}

fn preflight_directory(file: &mut (impl Read + Seek)) -> Result<(u64, u64), PackError> {
    // Actor packages produced by the packer have no archive comment.
    let end = file
        .seek(SeekFrom::End(0))?
        .checked_sub(END_SIZE)
        .ok_or_else(|| invalid("truncated package ZIP footer"))?;
    let footer = read_at::<{ END_SIZE as usize }>(file, end)?;
    require(
        &footer[..4] == b"PK\x05\x06" && footer[4..8] == [0; 4] && footer[20..22] == [0; 2],
        "invalid package ZIP footer",
    )?;
    let count = read_u16(&footer, 10);
    require(
        read_u16(&footer, 8) == count,
        "multi-disk ZIP is unsupported",
    )?;
    let mut start = read_u32(&footer, 16);
    let mut bytes = read_u32(&footer, 12);
    let mut entries = count;
    let mut directory_end = end;
    if let Some(locator_offset) = end.checked_sub(ZIP64_LOCATOR_SIZE) {
        let locator = read_at::<{ ZIP64_LOCATOR_SIZE as usize }>(file, locator_offset)?;
        if &locator[..4] == b"PK\x06\x07" {
            require(
                read_u32(&locator, 4) == 0 && read_u32(&locator, 16) == 1,
                "multi-disk ZIP64 is unsupported",
            )?;
            directory_end = read_u64(&locator, 8);
            require(
                directory_end.checked_add(ZIP64_END_SIZE) == Some(locator_offset),
                "invalid ZIP64 footer offset",
            )?;
            let zip64 = read_at::<{ ZIP64_END_SIZE as usize }>(file, directory_end)?;
            require(
                &zip64[..4] == b"PK\x06\x06"
                    && read_u64(&zip64, 4) == ZIP64_END_SIZE - 12
                    && zip64[16..24] == [0; 8]
                    && read_u64(&zip64, 24) == read_u64(&zip64, 32),
                "invalid ZIP64 directory footer",
            )?;
            entries = read_u64(&zip64, 32);
            bytes = read_u64(&zip64, 40);
            start = read_u64(&zip64, 48);
        }
    }
    require(
        entries > 0
            && entries <= MAX_ENTRY_COUNT
            && bytes <= MAX_DIRECTORY_BYTES
            && start.checked_add(bytes) == Some(directory_end),
        "ZIP directory exceeds package metadata limits",
    )?;
    let mut position = start;
    for _ in 0..entries {
        require(
            position
                .checked_add(46)
                .is_some_and(|offset| offset <= directory_end),
            "truncated ZIP directory entry",
        )?;
        let entry = read_at::<46>(file, position)?;
        let fields = read_u16(&entry, 28) + read_u16(&entry, 30) + read_u16(&entry, 32);
        require(
            &entry[..4] == b"PK\x01\x02" && fields <= MAX_ENTRY_METADATA,
            "invalid ZIP directory entry",
        )?;
        position += 46 + fields;
    }
    require(
        position == directory_end,
        "ZIP directory has an inconsistent size",
    )?;
    Ok((start, entries))
}

fn read_at<const N: usize>(
    reader: &mut (impl Read + Seek),
    offset: u64,
) -> Result<[u8; N], PackError> {
    let mut bytes = [0; N];
    reader.seek(SeekFrom::Start(offset))?;
    reader.read_exact(&mut bytes)?;
    Ok(bytes)
}

fn read_u16(bytes: &[u8], start: usize) -> u64 {
    u16::from_le_bytes(bytes[start..start + 2].try_into().unwrap()).into()
}

fn read_u32(bytes: &[u8], start: usize) -> u64 {
    u32::from_le_bytes(bytes[start..start + 4].try_into().unwrap()).into()
}

fn read_u64(bytes: &[u8], start: usize) -> u64 {
    u64::from_le_bytes(bytes[start..start + 8].try_into().unwrap())
}

pub(super) fn hash_and_copy(
    reader: impl Read,
    mut writer: impl Write,
    limit: u64,
) -> Result<([u8; 32], u64), PackError> {
    let mut reader = reader.take(limit.saturating_add(1));
    let mut hash = Sha256::new();
    let mut size = 0;
    let mut buffer = [0; 64 * 1024];
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            return Ok((hash.finalize().into(), size));
        }
        size += read as u64;
        require(
            size <= limit,
            "OCI file or decompressed layer exceeds its byte limit",
        )?;
        hash.update(&buffer[..read]);
        writer.write_all(&buffer[..read])?;
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use serde_json::json;

    use super::*;
    use crate::RegistrationManifest;

    fn image_fixture(folder: &Path, diff_id: String) -> ActorPackageManifest {
        const IMAGE_MANIFEST: &str = "application/vnd.oci.image.manifest.v1+json";
        const IMAGE_CONFIG: &str = "application/vnd.oci.image.config.v1+json";
        const LAYER: &str = "application/vnd.oci.image.layer.v1.tar";
        let blobs = folder.join("image/blobs/sha256");
        fs::create_dir_all(&blobs).unwrap();
        let mut files = Vec::new();
        let mut blob = |media_type: &str, bytes: &[u8]| {
            let hash = hex::encode(Sha256::digest(bytes));
            fs::write(blobs.join(&hash), bytes).unwrap();
            files.push(crate::PackageFileEntry {
                path: format!("image/blobs/sha256/{hash}"),
                role: "image".into(),
                media_type: media_type.into(),
                size: 0,
                sha256: "0".repeat(64),
            });
            json!({"mediaType": media_type, "digest": format!("sha256:{hash}"), "size": bytes.len()})
        };
        let layer = blob(LAYER, &[0; 1024]);
        let config = blob(
            IMAGE_CONFIG,
            &serde_json::to_vec(&json!({
                "architecture": "amd64",
                "os": "linux",
                "rootfs": {"type": "layers", "diff_ids": [diff_id]}
            }))
            .unwrap(),
        );
        let image = blob(
            IMAGE_MANIFEST,
            &serde_json::to_vec(&json!({
                "schemaVersion": 2,
                "config": config,
                "layers": [layer]
            }))
            .unwrap(),
        );
        let index =
            serde_json::to_vec(&json!({"schemaVersion": 2, "manifests": [&image]})).unwrap();
        fs::write(folder.join("image/index.json"), index).unwrap();
        fs::write(
            folder.join("image/oci-layout"),
            br#"{"imageLayoutVersion":"1.0.0"}"#,
        )
        .unwrap();
        let digest = image["digest"].as_str().unwrap();
        let mut manifest = ActorPackageManifest::from_toml(&format!(
            r#"
format = "actr-package/2"
[actor]
publisher = "example.com"
name = "OciEcho"
version = "1.0.0"
[workload]
profile = "actr.oci/1"
entry = "image/index.json"
[workload.oci]
digest = "{digest}"
platform = "linux/amd64"
ipc = "actr.oci-ipc/1"
args = ["/app/echo"]
[host]
profile = "actr.oci-host/1"
api = "1.0.0"
[signature]
algorithm = "ed25519"
key-id = "placeholder"

[[files]]
path = "image/index.json"
role = "workload"
media-type = "application/vnd.oci.image.index.v1+json"
size = 0
sha256 = "{zero_hash}"

[[files]]
path = "image/oci-layout"
role = "image"
media-type = "application/json"
size = 0
sha256 = "{zero_hash}"
"#,
            zero_hash = "0".repeat(64)
        ))
        .unwrap();
        manifest.files.extend(files);
        manifest
    }

    #[test]
    fn verifies_the_same_snapshot_and_registers_its_signed_manifest() {
        let folder = tempfile::tempdir().unwrap();
        let key = SigningKey::from_bytes(&[7; 32]);
        let diff_id = format!("sha256:{}", hex::encode(Sha256::digest([0; 1024])));
        let manifest = image_fixture(folder.path(), diff_id);
        let mut writer = tempfile::tempfile().unwrap();
        pack_oci_package(
            manifest,
            folder.path(),
            &key,
            &mut writer,
            OciLimits::default(),
        )
        .unwrap();
        writer.rewind().unwrap();
        let mut bytes = Vec::new();
        writer.read_to_end(&mut bytes).unwrap();
        let archive = OciArchive::snapshot(Cursor::new(&bytes), OciLimits::default()).unwrap();
        let verified = archive.verify(&key.verifying_key(), None).unwrap();
        assert_eq!(
            verified.archive_digest().as_slice(),
            &Sha256::digest(&bytes)[..]
        );
        assert!(verified.layout_directory().join("index.json").is_file());
        let registration =
            RegistrationManifest::parse(&verified.verification().manifest_raw).unwrap();
        assert_eq!(registration.actor_type().name, "OciEcho");
        assert!(crate::verify_actor_package(&bytes, &key.verifying_key()).is_err());
    }

    #[test]
    fn rejects_a_valid_blob_with_the_wrong_uncompressed_digest() {
        let folder = tempfile::tempdir().unwrap();
        let key = SigningKey::from_bytes(&[7; 32]);
        let manifest = image_fixture(folder.path(), format!("sha256:{}", "0".repeat(64)));
        let err = pack_oci_package(
            manifest,
            folder.path(),
            &key,
            Cursor::new(Vec::new()),
            OciLimits::default(),
        )
        .unwrap_err();
        assert!(err.to_string().contains("DiffID mismatch"));
    }
}

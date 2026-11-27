//! Indexed access to a verified package's OCI image tar.

use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{self, Read, Seek, SeekFrom, Take};
use std::path::{Path, PathBuf};

use super::{invalid, require};
use crate::PackError;

#[derive(Debug)]
pub(super) struct ImageLayout {
    archive: PathBuf,
    files: BTreeMap<String, TarFile>,
}

#[derive(Debug)]
struct TarFile {
    offset: u64,
    size: u64,
}

impl ImageLayout {
    pub(super) fn index(archive: PathBuf) -> Result<Self, PackError> {
        let file = File::open(&archive)?;
        let len = file.metadata()?.len();
        let mut tar = tar::Archive::new(file);
        let mut files = BTreeMap::new();
        // OCI layout names fit in a regular tar header. Skip payloads by
        // seeking and reject extension headers and non-regular files.
        for (i, entry) in tar.entries_with_seek()?.raw(true).enumerate() {
            require(
                i < crate::DEFAULT_MAX_VERIFIED_ENTRIES,
                "too many OCI image tar entries",
            )?;
            let entry = entry?;
            let path = entry.path_bytes();
            let path = std::str::from_utf8(&path).map_err(|_| invalid("invalid OCI tar path"))?;
            let name = path.strip_prefix("./").unwrap_or(path);
            if entry.header().entry_type().is_dir() {
                require(
                    entry.size() == 0
                        && matches!(
                            name.trim_end_matches('/'),
                            "" | "." | "blobs" | "blobs/sha256"
                        )
                        && !name.starts_with('/'),
                    "unexpected OCI image tar directory",
                )?;
                continue;
            }
            require(
                entry.header().entry_type().is_file() && is_layout_file(name),
                "unexpected OCI image tar file",
            )?;
            let member = TarFile {
                offset: entry.raw_file_position(),
                size: entry.size(),
            };
            require(
                member
                    .offset
                    .checked_add(member.size)
                    .is_some_and(|end| end <= len),
                "truncated OCI image tar file",
            )?;
            require(
                files.insert(name.to_owned(), member).is_none(),
                "duplicate OCI image tar file",
            )?;
        }
        Ok(Self { archive, files })
    }

    pub(super) fn files(&self) -> impl Iterator<Item = &str> {
        self.files.keys().map(String::as_str)
    }

    pub(super) fn open(&self, name: &str) -> Result<Take<File>, PackError> {
        let member = self
            .files
            .get(name)
            .ok_or_else(|| invalid(format!("missing OCI image tar file: {name}")))?;
        let mut file = File::open(&self.archive)?;
        file.seek(SeekFrom::Start(member.offset))?;
        Ok(file.take(member.size))
    }

    pub(super) fn extract(&self, target: &Path) -> Result<(), PackError> {
        fs::create_dir(target)?;
        fs::create_dir_all(target.join("blobs/sha256"))?;
        for name in self.files() {
            let mut source = self.open(name)?;
            let size = source.limit();
            let mut target = File::options()
                .create_new(true)
                .write(true)
                .open(target.join(name))?;
            require(
                io::copy(&mut source, &mut target)? == size,
                "truncated OCI image tar file",
            )?;
        }
        Ok(())
    }
}

fn is_layout_file(name: &str) -> bool {
    matches!(name, "index.json" | "oci-layout")
        || name.strip_prefix("blobs/sha256/").is_some_and(|digest| {
            digest.len() == 64
                && digest
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        })
}

//! Isolated OCI unpack, image admission, and cache leases.

use std::fs::{self, File};
use std::io::{self, Read, Seek, SeekFrom};
use std::os::fd::AsRawFd;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use actr_config::OciRuntimeConfig;
use actr_pack::oci::VerifiedOciPackage;
use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};
use tokio::process::Command;

use super::runtime::{private_dir, validate_cgroup, volume_limits};

pub(super) struct ImageLease {
    pub rootfs: PathBuf,
    pub lock_path: PathBuf,
    _lock: File,
}

pub(super) async fn prepare(
    config: &OciRuntimeConfig,
    package: &VerifiedOciPackage,
) -> Result<ImageLease> {
    let image_dir = private_dir(&config.image_dir)?;
    volume_limits(
        &image_dir,
        config.max_unpack_bytes,
        config.max_unpack_inodes,
    )?;
    let uid = unsafe { libc::geteuid() };
    let gid = unsafe { libc::getegid() };
    let workload = package
        .manifest()
        .workload
        .oci
        .as_ref()
        .context("missing OCI metadata")?;
    let mut key = Sha256::new();
    key.update(b"actr-oci-rootfs-v1\0");
    key.update(workload.digest.as_bytes());
    key.update(uid.to_le_bytes());
    key.update(gid.to_le_bytes());
    key.update(config.max_unpack_bytes.to_le_bytes());
    key.update(config.max_unpack_inodes.to_le_bytes());
    for executable in [&config.umoci, &config.bubblewrap] {
        let mut file = File::open(executable)?;
        io::copy(&mut file, &mut key)?;
    }
    let name = format!("{:x}", key.finalize());
    let lock_path = image_dir.join(format!("{name}.lock"));
    let lock = open_lock(&lock_path)?;
    let build_lock = open_lock(&image_dir.join(format!("{name}.build.lock")))?;
    let rootfs = image_dir.join(&name);
    let (lock, _build) = tokio::task::spawn_blocking(move || {
        lock.lock_shared()?;
        build_lock.lock()?;
        Ok::<_, io::Error>((lock, build_lock))
    })
    .await??;
    if !rootfs.exists() {
        let stage = tempfile::tempdir_in(&image_dir)?;
        let index_file = stage.path().join("index.json");
        let original = package.read_metadata(&package.manifest().workload.entry)?;
        let mut index: serde_json::Value = serde_json::from_slice(&original)?;
        index["manifests"][0]["annotations"] =
            serde_json::json!({"org.opencontainers.image.ref.name": "actr-verified"});
        fs::write(&index_file, serde_json::to_vec(&index)?)?;
        let limits = UnpackCgroup::new(&validate_cgroup(&config.cgroup_parent)?)?;
        let diagnostic = stage.path().join("unpack.stderr");
        let mut cmd = Command::new(&config.bubblewrap);
        cmd.env_clear()
            .kill_on_drop(true)
            .args([
                "--unshare-all",
                "--die-with-parent",
                "--new-session",
                "--clearenv",
            ])
            .args([
                "--ro-bind",
                "/usr/lib",
                "/usr/lib",
                "--symlink",
                "usr/lib",
                "/lib",
            ]);
        #[cfg(target_arch = "x86_64")]
        cmd.args([
            "--ro-bind",
            "/usr/lib64",
            "/usr/lib64",
            "--symlink",
            "usr/lib64",
            "/lib64",
        ]);
        cmd.arg("--ro-bind")
            .arg(&config.umoci)
            .arg("/umoci")
            .arg("--ro-bind")
            .arg(package.layout_directory())
            .arg("/image")
            .arg("--ro-bind")
            .arg(&index_file)
            .arg("/image/index.json")
            .arg("--bind")
            .arg(stage.path())
            .arg("/stage")
            .args(["--dev", "/dev", "--proc", "/proc", "--chdir", "/stage"])
            .args([
                "/umoci",
                "raw",
                "unpack",
                "--rootless",
                "--image",
                "/image:actr-verified",
                "/stage/rootfs",
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(File::create(&diagnostic)?);
        limits.constrain(&mut cmd)?;
        let status = crate::timer::timeout(
            crate::timer::ids::OCI_IMAGE_UNPACK,
            Duration::from_secs(60),
            cmd.status(),
        )
        .await
        .context("OCI image extraction timed out")??;
        if !status.success() {
            let mut detail = Vec::new();
            File::open(diagnostic)?
                .take(2048)
                .read_to_end(&mut detail)?;
            anyhow::bail!(
                "isolated umoci unpack failed: {status}: {}",
                String::from_utf8_lossy(&detail).trim()
            );
        }
        let prepared = stage.path().join("rootfs");
        for path in ["proc", "dev", "tmp", "run/actr", "data"] {
            ensure_directory(&prepared, path)?;
        }
        check_entry(&prepared, &workload.args[0])?;
        fs::rename(&prepared, &rootfs)?;
    }
    check_entry(&rootfs, &workload.args[0])?;

    Ok(ImageLease {
        rootfs,
        lock_path,
        _lock: lock,
    })
}

fn open_lock(path: &Path) -> io::Result<File> {
    File::options()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(path)
}

fn check_entry(rootfs: &Path, entry: &str) -> Result<()> {
    let path = rootfs.join(entry.strip_prefix('/').context("OCI executable path")?);
    for ancestor in path
        .ancestors()
        .take_while(|candidate| *candidate != rootfs)
    {
        let metadata = fs::symlink_metadata(ancestor)?;
        ensure!(!metadata.is_symlink(), "OCI entry contains a symbolic link");
    }
    let metadata = fs::symlink_metadata(&path)?;
    ensure!(
        metadata.is_file() && metadata.mode() & 0o111 != 0,
        "OCI entry is not executable"
    );
    let mut file = File::open(&path)?;
    let mut elf = [0; 64];
    file.read_exact(&mut elf)?;
    // ELF e_machine values: EM_AARCH64 and EM_X86_64.
    let machine: u16 = if cfg!(target_arch = "aarch64") {
        183
    } else {
        62
    };
    ensure!(
        &elf[..6] == b"\x7fELF\x02\x01"
            && elf[16..18] == [3, 0]
            && elf[18..20] == machine.to_le_bytes(),
        "OCI Guest must be a static PIE for the native Linux architecture"
    );
    let program_offset = u64::from_le_bytes(elf[32..40].try_into().unwrap());
    let header_size = u16::from_le_bytes(elf[54..56].try_into().unwrap());
    let count = u16::from_le_bytes(elf[56..58].try_into().unwrap());
    ensure!(
        (1..=128).contains(&count) && header_size == 56,
        "invalid OCI ELF program table"
    );
    file.seek(SeekFrom::Start(program_offset))?;
    for _ in 0..count {
        let mut header = [0; 56];
        file.read_exact(&mut header)?;
        ensure!(
            header[..4] != [3, 0, 0, 0],
            "OCI Guest requires a static executable"
        );
    }
    Ok(())
}

fn ensure_directory(root: &Path, relative: &str) -> Result<()> {
    let mut current = root.to_path_buf();
    ensure!(fs::symlink_metadata(root)?.is_dir(), "invalid OCI rootfs");
    for component in Path::new(relative).components() {
        current.push(component);
        match fs::symlink_metadata(&current) {
            Ok(metadata) => ensure!(metadata.is_dir(), "OCI mount point must be a directory"),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                fs::create_dir(&current)?;
                fs::set_permissions(&current, fs::Permissions::from_mode(0o755))?;
            }
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

struct UnpackCgroup(PathBuf);

impl UnpackCgroup {
    fn new(parent: &Path) -> Result<Self> {
        let path = parent.join(format!("actr-unpack-{}", uuid::Uuid::new_v4().simple()));
        fs::create_dir(&path)?;
        let group = Self(path);
        for (file, value) in [
            ("memory.max", "536870912"),
            ("pids.max", "32"),
            ("cpu.max", "100000 100000"),
        ] {
            fs::write(group.0.join(file), value)?;
        }
        Ok(group)
    }

    fn constrain(&self, command: &mut Command) -> Result<()> {
        let cgroup = File::options()
            .write(true)
            .open(self.0.join("cgroup.procs"))?;
        // Run the child in the bounded cgroup before its first exec. The
        // pre-exec closure does one async-signal-safe write on a preopened fd.
        unsafe {
            command.pre_exec(move || {
                let result = libc::write(cgroup.as_raw_fd(), b"0".as_ptr().cast(), 1);
                if result == 1 {
                    Ok(())
                } else {
                    Err(io::Error::last_os_error())
                }
            });
        }
        Ok(())
    }
}

impl Drop for UnpackCgroup {
    fn drop(&mut self) {
        let _ = fs::write(self.0.join("cgroup.kill"), "1");
        if let Err(error) = fs::remove_dir(&self.0) {
            tracing::warn!(%error, "OCI unpack cgroup requires reconciliation");
        }
    }
}

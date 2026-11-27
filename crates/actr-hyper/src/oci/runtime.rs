//! Spawn and authenticate a rootless Syd guest.

use std::ffi::CString;
use std::fs::{self, File};
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use actr_config::{OciRuntimeConfig, WasiDataAccess};
use actr_pack::oci::{DATA_READ_ONLY, DATA_READ_WRITE, OciWorkloadDescriptor};
use actr_protocol::oci as wire;
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::watch;

use super::image::ImageLease;
use super::supervisor::{Event, GuestExit, Launch, guest_peer};

const START_TIMEOUT: Duration = Duration::from_secs(15);
const EXIT_REPORT_GRACE: Duration = Duration::from_millis(300);
const POLICY_MAX_BYTES: u64 = 64 * 1024;

/// Check the configured Linux host without launching a workload.
pub async fn doctor(config: &OciRuntimeConfig) -> Result<()> {
    ensure!(
        config.memory_limit_bytes > 0,
        "OCI memory-limit-bytes must be positive"
    );
    ensure!(
        unsafe { libc::geteuid() } != 0,
        "OCI requires a non-root service account"
    );
    private_dir(&config.state_dir)?;
    let image = private_dir(&config.image_dir)?;
    volume_limits(&image, config.max_unpack_bytes, config.max_unpack_inodes)?;
    validate_cgroup(&config.cgroup_parent)?;
    for tool in [
        &config.syd_oci,
        &config.bubblewrap,
        &config.umoci,
        &config.supervisor,
    ] {
        validate_tool(tool)?;
    }
    read_policy(&config.policy)?;
    let version = crate::timer::timeout(
        crate::timer::ids::OCI_RUNTIME_VERSION,
        Duration::from_secs(5),
        Command::new(&config.syd_oci)
            .arg("--version")
            .env_clear()
            .kill_on_drop(true)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status(),
    )
    .await
    .context("Syd version check timed out")??;
    ensure!(
        version.success(),
        "syd-oci is unavailable on this kernel: {version}"
    );
    Ok(())
}

#[derive(Clone, Copy, Debug)]
pub(super) enum GuestFailure {
    Exited(GuestExit),
    Disconnected,
}

impl GuestFailure {
    pub fn describe(self, memory_limit: i64) -> String {
        match self {
            Self::Exited(exit) if exit.oom_killed => {
                format!("OCI Guest was killed by OOM (memory limit {memory_limit} bytes)")
            }
            Self::Exited(GuestExit {
                signal: Some(signal),
                ..
            }) => {
                format!("OCI Guest exited on signal {signal}")
            }
            Self::Exited(GuestExit {
                code: Some(code), ..
            }) => {
                format!("OCI Guest exited with status {code}")
            }
            Self::Exited(_) => "OCI Guest exited".into(),
            Self::Disconnected => "OCI Guest disconnected".into(),
        }
    }
}

pub(super) async fn wait_for_guest_exit(
    exit: &mut watch::Receiver<Option<GuestExit>>,
) -> Option<GuestExit> {
    exit.wait_for(Option::is_some)
        .await
        .ok()
        .and_then(|report| *report)
}

pub(super) async fn guest_failure(exit: &mut watch::Receiver<Option<GuestExit>>) -> GuestFailure {
    crate::timer::timeout(
        crate::timer::ids::OCI_EXIT_REPORT,
        EXIT_REPORT_GRACE,
        wait_for_guest_exit(exit),
    )
    .await
    .ok()
    .flatten()
    .map_or(GuestFailure::Disconnected, GuestFailure::Exited)
}

async fn read_supervisor_event(
    state: &mut (impl tokio::io::AsyncBufRead + Unpin),
) -> Result<Event> {
    let mut bytes = Vec::new();
    state.take(512).read_until(b'\n', &mut bytes).await?;
    ensure!(
        bytes.last() == Some(&b'\n'),
        "OCI supervisor status pipe closed or exceeded its frame limit"
    );
    Ok(serde_json::from_slice(&bytes)?)
}

pub(super) struct Container {
    pub stream: Option<UnixStream>,
    shutdown: Option<ChildStdin>,
    task: Child,
    directory: PathBuf,
    exit: watch::Receiver<Option<GuestExit>>,
    _owner: File,
    _image: ImageLease,
}

impl Container {
    pub async fn start(
        host: &OciRuntimeConfig,
        workload: &OciWorkloadDescriptor,
        capabilities: &[String],
        image: ImageLease,
    ) -> Result<Self> {
        let state = private_dir(&host.state_dir)?;
        let instance = format!("actr-{}", uuid::Uuid::new_v4().simple());
        let dir = state.join(&instance);
        let cgroup = validate_cgroup(&host.cgroup_parent)?.join(&instance);
        fs::create_dir(&dir)?;
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o700))?;
        let result = Self::launch(
            host,
            workload,
            capabilities,
            image,
            dir.clone(),
            cgroup,
            instance,
        )
        .await;
        if result.is_err() && !dir.join("launch.json").exists() {
            let _ = fs::remove_dir_all(dir);
        }
        result
    }

    async fn launch(
        host: &OciRuntimeConfig,
        workload: &OciWorkloadDescriptor,
        capabilities: &[String],
        image: ImageLease,
        directory: PathBuf,
        cgroup: PathBuf,
        instance: String,
    ) -> Result<Self> {
        validate_tool(&host.supervisor)?;
        fs::create_dir_all(directory.join("config/syd/oci"))?;
        fs::create_dir_all(directory.join("run"))?;
        fs::create_dir_all(directory.join("state"))?;
        fs::create_dir_all(directory.join("ipc"))?;
        let socket = directory.join("ipc/guest.sock");
        let listener = UnixListener::bind(&socket).context("bind the private OCI IPC socket")?;
        let policy = read_policy(&host.policy)?;
        write_json(
            &directory.join("config.json"),
            &bundle(host, workload, capabilities, &image, &directory, &cgroup)?,
        )?;
        fs::write(directory.join("config/syd/oci/default.syd-3"), policy)?;
        // The starter and supervisor can hold this lock together. GC needs
        // exclusive access before reclaiming a bundle.
        let owner = File::create_new(directory.join("owner.lock"))?;
        owner.lock_shared()?;
        let launch = Launch {
            runtime: host.syd_oci.clone(),
            bundle: directory.clone(),
            cgroup: cgroup.clone(),
            instance,
            image_lock: image.lock_path.clone(),
        };
        let mut launch_file = tempfile::NamedTempFile::new_in(&directory)?;
        serde_json::to_writer(launch_file.as_file_mut(), &launch)?;
        launch_file.as_file().sync_all()?;
        launch_file.persist_noclobber(directory.join("launch.json"))?;

        let mut task = Command::new(&host.supervisor)
            .arg(directory.join("launch.json"))
            .env_clear()
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .context("spawn the OCI supervisor")?;
        let shutdown = task.stdin.take();
        let mut state = BufReader::new(
            task.stdout
                .take()
                .context("missing OCI supervisor status pipe")?,
        );
        let (send_exit, exit) = watch::channel(None);
        let mut startup_exit = exit.clone();
        let mut container = Self {
            stream: None,
            shutdown,
            task,
            directory,
            exit,
            _owner: owner,
            _image: image,
        };
        let ready = async {
            let pid = match read_supervisor_event(&mut state)
                .await
                .context("OCI supervisor did not start the Guest")?
            {
                Event::Running { pid } => pid,
                Event::Exited(exit) => {
                    anyhow::bail!(
                        "{}",
                        GuestFailure::Exited(exit).describe(host.memory_limit_bytes)
                    );
                }
            };
            tokio::spawn(async move {
                if let Ok(Event::Exited(report)) = read_supervisor_event(&mut state).await {
                    send_exit.send_replace(Some(report));
                }
            });
            loop {
                tokio::select! {
                    outcome = wait_for_guest_exit(&mut startup_exit) => {
                        let failure = outcome.map_or(GuestFailure::Disconnected, GuestFailure::Exited);
                        anyhow::bail!("{}", failure.describe(host.memory_limit_bytes));
                    }
                    accepted = listener.accept() => {
                        let (stream, _) = accepted?;
                        let credentials = stream.peer_cred()?;
                        if credentials.uid() == unsafe { libc::geteuid() }
                            && credentials.pid().is_some_and(|peer| guest_peer(peer, pid, &cgroup))
                        {
                            return Ok(stream);
                        }
                    }
                }
            }
        };
        let result =
            crate::timer::timeout(crate::timer::ids::OCI_CONTAINER_START, START_TIMEOUT, ready)
                .await
                .context("OCI Guest did not establish an authenticated socket in time")
                .and_then(|result| result);
        match result {
            Ok(stream) => {
                container.stream = Some(stream);
                Ok(container)
            }
            Err(error) => Err(container.abort_start(error).await),
        }
    }

    pub fn exit(&self) -> watch::Receiver<Option<GuestExit>> {
        self.exit.clone()
    }

    pub async fn abort_start(&mut self, error: anyhow::Error) -> anyhow::Error {
        if let Err(cleanup) = self.stop().await {
            tracing::warn!(%cleanup, "OCI Guest startup cleanup requires reconciliation");
        }
        error
    }

    pub async fn stop(&mut self) -> Result<()> {
        self.shutdown.take();
        self.stream.take();
        let status = crate::timer::timeout(
            crate::timer::ids::OCI_CONTAINER_STOP,
            Duration::from_secs(15),
            self.task.wait(),
        )
        .await
        .context("OCI supervisor cleanup is still pending")??;
        ensure!(
            self.directory.join("stopped").is_file(),
            "OCI cleanup is pending: {status}"
        );
        if let Ok(exit) = fs::read_to_string(self.directory.join("init-exit")) {
            tracing::info!(%exit, "OCI Guest exited");
        }
        fs::remove_dir_all(&self.directory).context("remove the stopped OCI bundle")?;
        Ok(())
    }
}

fn bundle(
    host: &OciRuntimeConfig,
    workload: &OciWorkloadDescriptor,
    capabilities: &[String],
    image: &ImageLease,
    dir: &Path,
    cgroup: &Path,
) -> Result<Value> {
    let uid = unsafe { libc::geteuid() };
    let gid = unsafe { libc::getegid() };
    let ipc_dir = dir.join("ipc");
    let mount = |destination: &str, kind: &str, source: &Path, options: &[&str]| json!({"destination": destination, "type": kind, "source": source, "options": options});
    let mut mounts = vec![
        mount(
            "/proc",
            "proc",
            Path::new("proc"),
            &["nosuid", "nodev", "noexec"],
        ),
        mount(
            "/dev",
            "tmpfs",
            Path::new("tmpfs"),
            &["nosuid", "mode=755", "size=16777216"],
        ),
        mount(
            "/tmp",
            "tmpfs",
            Path::new("tmpfs"),
            &["nosuid", "nodev", "noexec", "mode=1777", "size=67108864"],
        ),
        mount(
            "/run/actr",
            "bind",
            &ipc_dir,
            &["bind", "ro", "nosuid", "nodev", "noexec"],
        ),
    ];
    if let Some((data, write)) = data_grant(host, capabilities)? {
        mounts.push(mount(
            "/data",
            "bind",
            &data,
            &[
                "bind",
                if write { "rw" } else { "ro" },
                "nosuid",
                "nodev",
                "noexec",
            ],
        ));
    }
    let mut env = workload
        .env
        .iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect::<Vec<_>>();
    if !workload.env.contains_key("PATH") {
        env.push("PATH=/usr/bin:/bin".to_owned());
    }
    env.push(format!("{}={}", wire::SOCKET_ENV, wire::GUEST_SOCKET_PATH));
    let cgroup = cgroup.strip_prefix("/sys/fs/cgroup")?;
    let cgroup = Path::new("/").join(cgroup);
    let mut unified = serde_json::Map::new();
    unified.insert("memory.oom.group".into(), json!("1"));
    if let Some(swap_limit) = host.swap_limit_bytes {
        // OCI memory.swap is RAM plus swap. Syd's cgroup backend skips the
        // zero produced by subtracting those equal limits.
        unified.insert("memory.swap.max".into(), json!(swap_limit.to_string()));
    }
    let namespaces =
        ["pid", "network", "ipc", "uts", "mount", "user"].map(|kind| json!({"type": kind}));
    Ok(json!({
        "ociVersion": "1.0.2",
        "hostname": "actr-guest",
        "root": {"path": image.rootfs, "readonly": true},
        "process": {
            "terminal": false,
            "cwd": "/",
            "args": workload.args,
            "env": env,
            "user": {"uid": 0, "gid": 0},
            "capabilities": {"bounding": [], "effective": [], "inheritable": [], "permitted": [], "ambient": []},
            "noNewPrivileges": true
        },
        "mounts": mounts,
        "linux": {
            "namespaces": namespaces,
            "uidMappings": [{"containerID":0, "hostID":uid, "size":1}],
            "gidMappings": [{"containerID":0, "hostID":gid, "size":1}],
            "cgroupsPath": cgroup,
            "resources": {
                "memory": {"limit": host.memory_limit_bytes},
                "unified": unified,
                "cpu":{"quota":100000,"period":100000}, "pids":{"limit":64}
            },
            "devices": []
        }
    }))
}

pub(super) fn data_grant(
    host: &OciRuntimeConfig,
    capabilities: &[String],
) -> Result<Option<(PathBuf, bool)>> {
    let write = capabilities
        .iter()
        .any(|capability| capability == DATA_READ_WRITE);
    let read = capabilities
        .iter()
        .any(|capability| capability == DATA_READ_ONLY);
    if !write && !read {
        return Ok(None);
    }
    let grant = host
        .data
        .as_ref()
        .context("OCI data capability requires a host data grant")?;
    let data = private_dir(&grant.path)?;
    let state = private_dir(&host.state_dir)?;
    let images = private_dir(&host.image_dir)?;
    ensure!(
        !data.starts_with(&state)
            && !data.starts_with(&images)
            && !state.starts_with(&data)
            && !images.starts_with(&data),
        "OCI data must be separate from runtime and cache storage"
    );
    ensure!(
        !write || grant.access == WasiDataAccess::ReadWrite,
        "OCI data grant is read-only"
    );
    if write {
        volume_limits(&data, grant.max_bytes, grant.max_inodes)?;
    }
    Ok(Some((data, write)))
}

pub(super) fn private_dir(path: &Path) -> Result<PathBuf> {
    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("inspect OCI directory {}", path.display()))?;
    ensure!(
        metadata.is_dir()
            && metadata.uid() == unsafe { libc::geteuid() }
            && metadata.mode() & 0o077 == 0,
        "OCI directory must be private and owned by the service user: {}",
        path.display()
    );
    Ok(path.canonicalize()?)
}

pub(super) fn validate_cgroup(path: &Path) -> Result<PathBuf> {
    let path = path
        .canonicalize()
        .context("OCI cgroup parent is not available")?;
    ensure!(
        path.starts_with("/sys/fs/cgroup") && path != Path::new("/sys/fs/cgroup"),
        "OCI cgroup parent must be delegated below the cgroup v2 root"
    );
    let enabled = fs::read_to_string(path.join("cgroup.subtree_control"))?;
    for controller in ["cpu", "memory", "pids"] {
        ensure!(
            enabled.split_whitespace().any(|value| value == controller),
            "OCI requires delegated {controller} controller"
        );
    }
    Ok(path)
}

pub(super) fn volume_limits(path: &Path, max_bytes: u64, max_inodes: u64) -> Result<()> {
    let path_bytes = CString::new(path.as_os_str().as_bytes())?;
    let mut stat = std::mem::MaybeUninit::<libc::statvfs>::uninit();
    if unsafe { libc::statvfs(path_bytes.as_ptr(), stat.as_mut_ptr()) } != 0 {
        return Err(io::Error::last_os_error()).context("read OCI volume limits");
    }
    let stat = unsafe { stat.assume_init() };
    ensure!(
        stat.f_blocks
            .checked_mul(stat.f_frsize)
            .is_some_and(|size| size > 0 && size <= max_bytes)
            && stat.f_files > 0
            && stat.f_files <= max_inodes,
        "OCI directory requires a volume with enforced byte and inode limits: {}",
        path.display()
    );
    Ok(())
}

fn validate_tool(path: &Path) -> Result<()> {
    let metadata = fs::metadata(path)?;
    let uid = unsafe { libc::geteuid() };
    ensure!(
        path.is_absolute()
            && metadata.is_file()
            && metadata.mode() & 0o111 != 0
            && metadata.mode() & 0o022 == 0
            && (metadata.uid() == 0 || metadata.uid() == uid),
        "OCI tool must be an absolute, trusted executable: {}",
        path.display()
    );
    Ok(())
}

fn read_policy(path: &Path) -> Result<String> {
    let mut policy = String::new();
    use std::io::Read;
    File::open(path)?
        .take(POLICY_MAX_BYTES + 1)
        .read_to_string(&mut policy)?;
    ensure!(
        policy.len() as u64 <= POLICY_MAX_BYTES && policy.lines().last() == Some("lock:on"),
        "OCI Syd policy must be bounded and end with lock:on"
    );
    Ok(policy)
}

fn write_json(path: &Path, value: &Value) -> Result<()> {
    let mut file = File::create_new(path)?;
    serde_json::to_writer(&mut file, value)?;
    file.sync_all()?;
    Ok(())
}

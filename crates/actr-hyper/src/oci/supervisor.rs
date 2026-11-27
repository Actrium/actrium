//! Dedicated child subreaper for one container.

use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};

const COMMAND_TIMEOUT: Duration = Duration::from_secs(5);
const OUTPUT_LIMIT: u64 = 256 * 1024;

#[derive(Serialize, Deserialize)]
pub(super) struct Launch {
    pub runtime: PathBuf,
    pub bundle: PathBuf,
    pub cgroup: PathBuf,
    pub instance: String,
    pub image_lock: PathBuf,
}

#[derive(Deserialize, Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub(super) enum Event {
    Running { pid: i32 },
    Exited(GuestExit),
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub(super) struct GuestExit {
    pub code: Option<i32>,
    pub signal: Option<i32>,
    pub oom_killed: bool,
}

fn send_event(stdout: &mut impl Write, event: Event) -> Result<()> {
    serde_json::to_writer(&mut *stdout, &event)?;
    stdout.write_all(b"\n")?;
    stdout.flush()?;
    Ok(())
}

/// Run the host-owned supervisor contract used by the OCI backend.
#[doc(hidden)]
pub fn supervisor_main() -> Result<()> {
    let launch_path = std::env::args_os()
        .nth(1)
        .context("missing OCI launch file")?;
    let launch_path = Path::new(&launch_path);
    let bundle = launch_path
        .parent()
        .context("missing OCI bundle directory")?;
    let live = File::open(bundle.join("owner.lock"))?;
    live.lock_shared()?;
    // Reopen the launch record under the lock. If GC won after the host
    // exited, the record no longer exists and this helper must not start.
    let config: Launch = serde_json::from_reader(File::open(launch_path)?)?;
    ensure!(
        config.bundle == bundle,
        "OCI launch record does not match its bundle"
    );
    // The helper parents both OCI runtime commands and orphaned container init
    // processes. This setting is confined to the dedicated helper process.
    let ret = unsafe { libc::prctl(libc::PR_SET_CHILD_SUBREAPER, 1, 0, 0, 0) };
    if ret != 0 {
        return Err(io::Error::last_os_error()).context("enable OCI subreaper");
    }
    let image = File::open(&config.image_lock)?;
    image.lock_shared()?;
    let (parent_lost, parent) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = io::stdin().read(&mut [0]);
        let _ = parent_lost.send(());
    });

    let result = start_and_wait(&config, parent);
    let clean = cleanup(&config);
    if clean.is_ok() {
        fs::write(config.bundle.join("stopped"), [])?;
    }
    result.and(clean)
}

fn start_and_wait(config: &Launch, parent: mpsc::Receiver<()>) -> Result<()> {
    let bundle = config.bundle.to_str().context("invalid OCI bundle path")?;
    invoke(config, &["create", "--bundle", bundle, &config.instance])?;
    let state = invoke(config, &["state", &config.instance])?;
    let state: serde_json::Value = serde_json::from_slice(&state)?;
    let policy = fs::read_to_string(config.bundle.join("config/syd/oci/default.syd-3"))?;
    ensure!(
        state["annotations"]["org.exherbo.syd.config"].as_str() == Some(&policy),
        "Syd did not apply the configured OCI policy"
    );
    let pid = state["pid"]
        .as_i64()
        .and_then(|pid| i32::try_from(pid).ok().filter(|pid| *pid > 0))
        .context("Syd did not report a valid container init PID")?;
    ensure!(
        process_in_cgroup(pid, &config.cgroup)?,
        "container init is outside its cgroup"
    );
    fs::write(config.bundle.join("init.pid"), pid.to_string())?;
    if parent.try_recv().is_ok() {
        bail!("OCI host stopped during container creation");
    }
    invoke(config, &["start", &config.instance])?;
    let mut stdout = io::stdout().lock();
    send_event(&mut stdout, Event::Running { pid })?;

    loop {
        if let Some(status) = reap_init(pid)? {
            let exit = guest_exit(config, status);
            let published = send_event(&mut stdout, Event::Exited(exit));
            let recorded = record_exit(config, exit);
            return published.and(recorded);
        }
        match parent.recv_timeout(Duration::from_millis(50)) {
            Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected) => return Ok(()),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
    }
}

pub(super) fn process_in_cgroup(pid: i32, cgroup: &Path) -> Result<bool> {
    let expected = cgroup
        .strip_prefix("/sys/fs/cgroup")
        .context("expected cgroup v2 path")?;
    let membership = fs::read_to_string(format!("/proc/{pid}/cgroup"))?;
    Ok(membership.lines().any(|line| {
        line.strip_prefix("0::")
            .and_then(|entry| Path::new(entry).strip_prefix("/").ok())
            == Some(expected)
    }))
}

/// Syd's init can parent the actual executable inside the container.
pub(super) fn guest_peer(pid: i32, init: i32, group: &Path) -> bool {
    let mut candidate = pid;
    for _ in 0..64 {
        if candidate <= 1 || !process_in_cgroup(candidate, group).unwrap_or(false) {
            return false;
        }
        if candidate == init {
            return true;
        }
        let Ok(status) = fs::read_to_string(format!("/proc/{candidate}/status")) else {
            return false;
        };
        let Some(parent) = status
            .lines()
            .find_map(|line| line.strip_prefix("PPid:"))
            .and_then(|value| value.trim().parse::<i32>().ok())
        else {
            return false;
        };
        if parent == candidate {
            return false;
        }
        candidate = parent;
    }
    false
}

pub(super) fn cleanup(config: &Launch) -> Result<()> {
    // Use only the private cgroup allocated to this instance.
    if config.cgroup.exists() {
        let _ = invoke(config, &["kill", "--all", &config.instance, "TERM"]);
        if !wait_empty(&config.cgroup, Duration::from_secs(1))? {
            fs::write(config.cgroup.join("cgroup.kill"), b"1")?;
            ensure!(
                wait_empty(&config.cgroup, COMMAND_TIMEOUT)?,
                "OCI cgroup remains populated"
            );
        }
    }
    if let Ok(pid) = fs::read_to_string(config.bundle.join("init.pid")) {
        if let Ok(pid) = pid.parse() {
            if let Ok(Some(status)) = reap_init(pid) {
                record_exit(config, guest_exit(config, status))?;
            }
        }
    }
    invoke(config, &["delete", "--force", &config.instance])?;
    reap_orphans();
    Ok(())
}

fn guest_exit(config: &Launch, status: ExitStatus) -> GuestExit {
    use std::os::unix::process::ExitStatusExt;

    // The runtime puts every Guest process directly in this private cgroup.
    let oom_killed =
        fs::read_to_string(config.cgroup.join("memory.events.local")).is_ok_and(|events| {
            events
                .lines()
                .find_map(|line| line.strip_prefix("oom_kill "))
                .and_then(|count| count.parse::<u64>().ok())
                .is_some_and(|count| count > 0)
        });
    GuestExit {
        code: status.code(),
        signal: status.signal(),
        oom_killed,
    }
}

fn record_exit(config: &Launch, exit: GuestExit) -> Result<()> {
    fs::write(config.bundle.join("init-exit"), serde_json::to_vec(&exit)?)?;
    Ok(())
}

fn wait_empty(group: &Path, duration: Duration) -> Result<bool> {
    let deadline = Instant::now() + duration;
    loop {
        match fs::read_to_string(group.join("cgroup.events")) {
            Ok(events) if events.lines().any(|line| line == "populated 0") => return Ok(true),
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(true),
            Err(error) => return Err(error.into()),
        }
        if Instant::now() >= deadline {
            return Ok(false);
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn reap_init(pid: i32) -> Result<Option<ExitStatus>> {
    use std::os::unix::process::ExitStatusExt;

    let mut status = 0;
    // The PID is obtained from Syd and checked against the private cgroup.
    let result = unsafe { libc::waitpid(pid, &mut status, libc::WNOHANG) };
    match result {
        n if n == pid => Ok(Some(ExitStatus::from_raw(status))),
        0 => Ok(None),
        _ => Err(io::Error::last_os_error()).context("reap the container init"),
    }
}

fn reap_orphans() {
    let mut status = 0;
    // The dedicated helper has no unrelated child processes.
    while unsafe { libc::waitpid(-1, &mut status, libc::WNOHANG) } > 0 {}
}

fn invoke(config: &Launch, args: &[&str]) -> Result<Vec<u8>> {
    let log = tempfile::tempfile_in(&config.bundle)?;
    let mut read_output = log.try_clone()?;
    let mut cmd = Command::new(&config.runtime);
    cmd.env_clear()
        .env("HOME", &config.bundle)
        .env("PATH", "/usr/bin:/bin")
        .env("SYD_LOG", "warn")
        .env("SYD_LOG_FD", "2")
        .env("XDG_CONFIG_HOME", config.bundle.join("config"))
        .env("XDG_RUNTIME_DIR", config.bundle.join("run"))
        .arg("--root")
        .arg(config.bundle.join("state"))
        .args(args)
        .stdin(Stdio::null());
    if args[0] == "create" {
        // The Guest inherits the writers. Serve both independently and
        // discard content beyond a bounded diagnostic prefix.
        let (out, stdout) = io::pipe()?;
        let (err, stderr) = io::pipe()?;
        let out_log = config.bundle.join("guest.stdout");
        let err_log = config.bundle.join("guest.stderr");
        std::thread::spawn(move || drain_log(out, &out_log));
        std::thread::spawn(move || drain_log(err, &err_log));
        cmd.stdout(stdout).stderr(stderr);
    } else {
        cmd.stdout(log).stderr(Stdio::null());
    }
    let mut child = cmd.spawn()?;
    let deadline = Instant::now() + COMMAND_TIMEOUT;
    loop {
        if let Some(status) = child.try_wait()? {
            ensure!(status.success(), "syd-oci {} failed: {status}", args[0]);
            use std::io::Seek;
            read_output.rewind()?;
            let mut output = Vec::new();
            read_output
                .take(OUTPUT_LIMIT + 1)
                .read_to_end(&mut output)?;
            ensure!(
                output.len() as u64 <= OUTPUT_LIMIT,
                "Syd response exceeds its limit"
            );
            return Ok(output);
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            bail!("syd-oci {} timed out", args[0]);
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn drain_log(mut pipe: io::PipeReader, path: &Path) {
    let mut output = File::create_new(path).ok();
    let mut budget = 64 * 1024;
    let mut buffer = [0; 4096];
    while let Ok(count) = pipe.read(&mut buffer) {
        if count == 0 {
            break;
        }
        let retained = budget.min(count);
        if let Some(file) = &mut output {
            if file.write_all(&buffer[..retained]).is_err() {
                output = None;
            }
        }
        budget -= retained;
    }
}

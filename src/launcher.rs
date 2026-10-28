use anyhow::{Context, Result, bail};
use std::{
    fs,
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const FLATPAK_ID: &str = "org.prismlauncher.PrismLauncher";
static LOG_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum LaunchPhase {
    #[default]
    Preparing,
    Authenticating,
    Downloading,
    StartingGame,
}

impl LaunchPhase {
    pub fn display(self) -> &'static str {
        match self {
            Self::Preparing => "Preparing instance",
            Self::Authenticating => "Authenticating account",
            Self::Downloading => "Preparing game files",
            Self::StartingGame => "Starting Minecraft",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchUpdate {
    pub phase: LaunchPhase,
    pub failure: Option<String>,
}

pub struct LaunchHandle {
    child: Option<Child>,
    log: fs::File,
    cursor: u64,
    pending: Vec<u8>,
    discard_line: bool,
    phase: LaunchPhase,
    failure: Option<String>,
}

impl LaunchHandle {
    /// Polls only the current launch's newly appended output. Exit zero can mean
    /// Prism handed the request to an existing process; it never means readiness.
    pub fn poll(&mut self) -> Result<LaunchUpdate> {
        self.read_new_log()?;
        if let Some(child) = self.child.as_mut()
            && let Some(status) = child
                .try_wait()
                .context("Cannot check Prism Launcher status")?
        {
            if !status.success() && self.failure.is_none() {
                self.failure = Some(format!(
                    "Prism Launcher exited with {status}. Open Prism normally to check the instance."
                ));
            }
            self.child = None;
        }
        Ok(LaunchUpdate {
            phase: self.phase,
            failure: self.failure.clone(),
        })
    }

    fn read_new_log(&mut self) -> Result<()> {
        const MAX_READ: u64 = 64 * 1024;
        const MAX_LINE: usize = 8 * 1024;
        let length = self.log.metadata()?.len();
        if length < self.cursor {
            self.cursor = length;
            self.pending.clear();
            self.discard_line = false;
        }
        if length.saturating_sub(self.cursor) > MAX_READ {
            self.cursor = length - MAX_READ;
            self.pending.clear();
            self.discard_line = true;
        }
        self.log.seek(SeekFrom::Start(self.cursor))?;
        let mut bytes = Vec::new();
        (&mut self.log).take(MAX_READ).read_to_end(&mut bytes)?;
        self.cursor += bytes.len() as u64;
        for byte in bytes {
            if byte == b'\n' {
                if !self.discard_line {
                    let line = String::from_utf8_lossy(&self.pending).to_ascii_lowercase();
                    self.observe_line(&line);
                }
                self.pending.clear();
                self.discard_line = false;
            } else if !self.discard_line {
                self.pending.push(byte);
                if self.pending.len() > MAX_LINE {
                    self.pending.clear();
                    self.discard_line = true;
                }
            }
        }
        Ok(())
    }

    fn observe_line(&mut self, line: &str) {
        // Prism Task::emitFailed emits this exact task/class structure. Do not
        // treat arbitrary game errors, retried requests or offscreen warnings as
        // launch failure, and never expose the raw line (it can contain tokens).
        let task_failed = line.contains("task ") && line.contains("failed:");
        if task_failed && (line.contains("launchcontroller(") || line.contains("launchtask(")) {
            self.failure.get_or_insert_with(|| {
                "Prism could not launch this instance. Open Prism normally to see the error.".into()
            });
        } else if task_failed && line.contains("authflow(") {
            self.failure.get_or_insert_with(|| {
                "Authentication needs attention. Open Prism normally to sign in again.".into()
            });
        }
        if line.contains("authflow:") || line.contains("authflow(") || line.contains("msastep(") {
            self.phase = LaunchPhase::Authenticating;
        } else if line.contains("minecraftloadandcheck(") || line.contains("netjob(") {
            self.phase = LaunchPhase::Downloading;
        } else if line.contains("launcherpartlaunch(") {
            self.phase = LaunchPhase::StartingGame;
        }
    }
}

impl Drop for LaunchHandle {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            // Releasing the monitor (including a UI timeout) must never stop the
            // game or hidden launcher. Reap it later if this process stays open.
            let _ = thread::Builder::new()
                .name("prism-launch-reaper".into())
                .spawn(move || {
                    let _ = child.wait();
                });
        }
    }
}

pub fn launch_instance(instance_path: &Path, hide_launcher: bool) -> Result<()> {
    let mut launch = start_instance(instance_path, hide_launcher)?;
    // The visible launch path retains its quick startup-error check.
    thread::sleep(Duration::from_millis(200));
    if let Some(failure) = launch.poll()?.failure {
        bail!("{failure}. See {}", log_path()?.display());
    }
    Ok(())
}

pub fn start_instance(instance_path: &Path, hide_launcher: bool) -> Result<LaunchHandle> {
    let metadata = fs::metadata(instance_path).with_context(|| {
        format!(
            "Instance folder is unavailable: {}",
            instance_path.display()
        )
    })?;
    if !metadata.is_dir() {
        bail!("Instance is not a folder: {}", instance_path.display());
    }
    // Preserve the directory ID and data root even when an instance is a symlink.
    let instance_path = std::path::absolute(instance_path)?;
    let mut command = launch_command(&instance_path, hide_launcher)?;
    start_command(&mut command, &log_path()?)
}

pub fn log_path() -> Result<PathBuf> {
    Ok(crate::config::xdg_dir("XDG_STATE_HOME", ".local/state")?
        .join("minecraft-tui")
        .join("launcher.log"))
}

fn launch_command(instance_path: &Path, hide_launcher: bool) -> Result<Command> {
    // Prism's CLI expects the directory ID, which can differ from the display name.
    let instance_id = instance_path
        .file_name()
        .context("Instance has no directory ID")?;
    let data_path = instance_path
        .parent()
        .and_then(Path::parent)
        .context("Instance has no Prism data directory")?;
    let is_flatpak = instance_path
        .components()
        .any(|part| part.as_os_str() == FLATPAK_ID);
    let mut command = if is_flatpak {
        let mut command = Command::new("flatpak");
        command.args(["run", FLATPAK_ID]);
        command
    } else {
        Command::new("prismlauncher")
    };
    if hide_launcher {
        // Qt consumes this argument before Prism parses --launch. Using an argument
        // instead of QT_QPA_PLATFORM keeps the game's environment unchanged.
        command.args(["-platform", "offscreen"]);
    }
    command
        .arg("--dir")
        .arg(data_path)
        .arg("--launch")
        .arg(instance_id);
    Ok(command)
}

fn start_command(command: &mut Command, path: &Path) -> Result<LaunchHandle> {
    fs::create_dir_all(
        path.parent()
            .context("Launch log has no parent directory")?,
    )?;
    archive_previous_log(path)?;
    let mut options = fs::OpenOptions::new();
    options.create_new(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::{fs::OpenOptionsExt, process::CommandExt};
        options.mode(0o600);
        // A separate process group and closed terminal streams let Prism and the
        // game continue after the TUI or its interactive shell exits.
        command.process_group(0);
    }
    let mut log = options
        .open(path)
        .with_context(|| format!("Cannot open launch log at {}", path.display()))?;
    writeln!(
        log,
        "\n--- Launch requested at {} ---",
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs()
    )?;
    let cursor = log.metadata()?.len();
    let reader = fs::File::open(path)?;
    command
        .stdin(Stdio::null())
        .stdout(log.try_clone()?)
        .stderr(log);
    let child = command.spawn().with_context(|| {
        format!(
            "Cannot start Prism Launcher; check that it is installed. Log: {}",
            path.display()
        )
    })?;
    Ok(LaunchHandle {
        child: Some(child),
        log: reader,
        cursor,
        pending: Vec::new(),
        discard_line: false,
        phase: LaunchPhase::Preparing,
        failure: None,
    })
}

fn archive_previous_log(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if !metadata.is_file() && !metadata.file_type().is_symlink() {
                bail!("Launch log path is not a file: {}", path.display());
            }
            let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
            let sequence = LOG_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let archive = path.with_file_name(format!(
                "{}-{timestamp}-{}-{sequence}.log",
                path.file_stem().unwrap_or_default().to_string_lossy(),
                std::process::id()
            ));
            // Existing launcher processes retain their old file descriptor, so
            // later output from an older game can never be read by this handle.
            fs::rename(path, &archive)
                .with_context(|| format!("Cannot archive launch log at {}", path.display()))?;
            Ok(())
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => {
            Err(error).with_context(|| format!("Cannot inspect launch log at {}", path.display()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsStr;

    fn args(command: &Command) -> Vec<String> {
        command
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn native_launch_uses_directory_id_and_only_hides_when_requested() {
        let path = Path::new("/data/PrismLauncher/instances/My pack (1)");
        let command = launch_command(path, false).unwrap();
        assert_eq!(command.get_program(), OsStr::new("prismlauncher"));
        assert_eq!(
            args(&command),
            ["--dir", "/data/PrismLauncher", "--launch", "My pack (1)"]
        );
        let command = launch_command(path, true).unwrap();
        assert_eq!(
            args(&command),
            [
                "-platform",
                "offscreen",
                "--dir",
                "/data/PrismLauncher",
                "--launch",
                "My pack (1)"
            ]
        );
        assert!(
            !command
                .get_envs()
                .any(|(name, _)| name == "QT_QPA_PLATFORM")
        );
    }

    #[test]
    fn flatpak_launch_places_qt_arguments_after_app_id() {
        let command = launch_command(Path::new("/home/user/.var/app/org.prismlauncher.PrismLauncher/data/PrismLauncher/instances/pack"), true).unwrap();
        assert_eq!(command.get_program(), OsStr::new("flatpak"));
        assert_eq!(
            &args(&command)[..4],
            ["run", FLATPAK_ID, "-platform", "offscreen"]
        );
    }

    fn temporary_directory(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "minecraft-tui-{label}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[cfg(unix)]
    #[test]
    fn nonblocking_launch_reports_delayed_failure_without_exposing_logs() {
        let directory = temporary_directory("delayed-failure");
        let log = directory.join("launcher.log");
        let mut command = Command::new("sh");
        command.args(["-c", "sleep 0.1; echo 'Task \"LaunchController(0x123 ID: abc)\" failed: secret-access-token'; exit 7"]);
        let mut launch = start_command(&mut command, &log).unwrap();
        assert_eq!(launch.poll().unwrap().failure, None);
        thread::sleep(Duration::from_millis(180));
        let update = launch.poll().unwrap();
        let failure = update.failure.unwrap();
        assert!(failure.contains("could not launch"));
        assert!(!failure.contains("secret"));
        assert!(launch.child.is_none());
        fs::remove_dir_all(directory).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn stale_failures_are_ignored_and_zero_exit_is_not_readiness() {
        let directory = temporary_directory("stale-log");
        fs::create_dir_all(&directory).unwrap();
        let log = directory.join("launcher.log");
        fs::write(&log, "Task \"LaunchController(old)\" failed: old error\n").unwrap();
        let mut old_writer = fs::OpenOptions::new().append(true).open(&log).unwrap();
        let mut command = Command::new("sh");
        command.args([
            "-c",
            "sleep 0.08; echo 'AuthFlow: refreshing account'; exit 0",
        ]);
        let mut launch = start_command(&mut command, &log).unwrap();
        old_writer
            .write_all(b"Task \"LaunchController(old)\" failed: late old failure\n")
            .unwrap();
        assert_eq!(launch.poll().unwrap().failure, None);
        thread::sleep(Duration::from_millis(160));
        let update = launch.poll().unwrap();
        assert_eq!(update.failure, None);
        assert_eq!(update.phase, LaunchPhase::Authenticating);
        assert!(launch.child.is_none());
        assert_eq!(launch.poll().unwrap(), update);
        assert!(!fs::read_to_string(&log).unwrap().contains("old failure"));
        let archive = fs::read_dir(&directory)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .find(|path| path.file_name() != Some(OsStr::new("launcher.log")))
            .unwrap();
        assert!(
            fs::read_to_string(archive)
                .unwrap()
                .contains("late old failure")
        );
        fs::remove_dir_all(directory).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn dropping_monitor_keeps_delayed_launcher_alive() {
        let directory = temporary_directory("drop-monitor");
        let log = directory.join("launcher.log");
        let mut command = Command::new("sh");
        command.args(["-c", "sleep 0.12; echo launch-survived; exit 0"]);
        drop(start_command(&mut command, &log).unwrap());
        thread::sleep(Duration::from_millis(200));
        assert!(
            fs::read_to_string(&log)
                .unwrap()
                .contains("launch-survived")
        );
        fs::remove_dir_all(directory).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn log_reads_are_bounded_and_only_known_fatal_tasks_fail() {
        let directory = temporary_directory("bounded-log");
        let log = directory.join("launcher.log");
        let mut command = Command::new("sh");
        command.args(["-c", "exit 0"]);
        let mut launch = start_command(&mut command, &log).unwrap();
        let mut writer = fs::OpenOptions::new().append(true).open(&log).unwrap();
        writer.write_all(&vec![b'x'; 200_000]).unwrap();
        writer.write_all(b"\nOAuth2 request failed: transient\nThis plugin does not support raise()\nTask \"NetJob(files)\" starting for the first time\n").unwrap();
        let update = launch.poll().unwrap();
        assert_eq!(update.failure, None);
        assert_eq!(update.phase, LaunchPhase::Downloading);
        assert!(launch.pending.len() <= 8 * 1024);
        writer
            .write_all(b"Task \"LaunchTask(current)\" failed: sensitive detail\n")
            .unwrap();
        assert!(launch.poll().unwrap().failure.is_some());
        fs::remove_dir_all(directory).unwrap();
    }
}

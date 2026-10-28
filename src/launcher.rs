use anyhow::{Context, Result, bail};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const FLATPAK_ID: &str = "org.prismlauncher.PrismLauncher";

pub fn launch_instance(instance_path: &Path, hide_launcher: bool) -> Result<()> {
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
    spawn_detached(&mut command, &log_path()?)
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

fn spawn_detached(command: &mut Command, path: &Path) -> Result<()> {
    fs::create_dir_all(
        path.parent()
            .context("Launch log has no parent directory")?,
    )?;
    let mut options = fs::OpenOptions::new();
    options.create(true).append(true);
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
    command
        .stdin(Stdio::null())
        .stdout(log.try_clone()?)
        .stderr(log);
    let mut child = command.spawn().with_context(|| {
        format!(
            "Cannot start Prism Launcher; check that it is installed. Log: {}",
            path.display()
        )
    })?;
    // Report missing Flatpak apps and unavailable Qt plugins instead of silently
    // dropping their startup errors when the TUI closes.
    thread::sleep(Duration::from_millis(200));
    if let Some(status) = child.try_wait()? {
        if !status.success() {
            bail!(
                "Prism Launcher exited with {status}. See {}",
                path.display()
            );
        }
    } else {
        // Reap the launcher when the TUI is kept open; dropping this thread when
        // the TUI exits does not terminate the independent child process.
        thread::spawn(move || {
            let _ = child.wait();
        });
    }
    Ok(())
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

    #[cfg(unix)]
    #[test]
    fn detached_start_reports_failure_and_preserves_logs() {
        let directory = std::env::temp_dir().join(format!(
            "minecraft-tui-launch-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let log = directory.join("launcher.log");
        let mut failing = Command::new("sh");
        failing.args(["-c", "echo startup-failed >&2; exit 7"]);
        let error = spawn_detached(&mut failing, &log).unwrap_err();
        assert!(error.to_string().contains("7"));
        assert!(fs::read_to_string(&log).unwrap().contains("startup-failed"));
        let mut successful = Command::new("sh");
        successful.args(["-c", "echo accepted; exit 0"]);
        spawn_detached(&mut successful, &log).unwrap();
        assert!(fs::read_to_string(&log).unwrap().contains("accepted"));
        fs::remove_dir_all(directory).unwrap();
    }
}

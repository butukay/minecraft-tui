use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    env, fs,
    io::Write,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default)]
pub struct Settings {
    pub hide_launcher: bool,
    pub details: bool,
}

impl Settings {
    pub fn load() -> Result<Self> {
        Self::load_from(&settings_path()?)
    }

    pub fn save(&self) -> Result<()> {
        self.save_to(&settings_path()?)
    }

    fn load_from(path: &Path) -> Result<Self> {
        match fs::read_to_string(path) {
            Ok(content) => serde_json::from_str(&content)
                .with_context(|| format!("Invalid settings in {}", path.display())),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(error) => {
                Err(error).with_context(|| format!("Cannot read settings at {}", path.display()))
            }
        }
    }

    fn save_to(&self, path: &Path) -> Result<()> {
        let parent = path
            .parent()
            .context("Settings path has no parent directory")?;
        fs::create_dir_all(parent)
            .with_context(|| format!("Cannot create {}", parent.display()))?;
        let temporary = path.with_extension(format!("{}.tmp", std::process::id()));
        let result = (|| -> Result<()> {
            let mut options = fs::OpenOptions::new();
            options.write(true).create(true).truncate(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let mut file = options.open(&temporary)?;
            serde_json::to_writer_pretty(&mut file, self)?;
            file.write_all(b"\n")?;
            file.sync_all()?;
            fs::rename(&temporary, path)?;
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result.with_context(|| format!("Cannot save settings to {}", path.display()))
    }
}

pub fn settings_path() -> Result<PathBuf> {
    Ok(xdg_dir("XDG_CONFIG_HOME", ".config")?
        .join("minecraft-tui")
        .join("config.json"))
}

pub(crate) fn xdg_dir(variable: &str, fallback: &str) -> Result<PathBuf> {
    if let Some(path) = env::var_os(variable)
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
    {
        return Ok(path);
    }
    Ok(PathBuf::from(env::var_os("HOME").context("HOME is not set")?).join(fallback))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn settings_round_trip_and_missing_fields_use_defaults() {
        let directory = env::temp_dir().join(format!(
            "minecraft-tui-settings-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let path = directory.join("config.json");
        assert_eq!(Settings::load_from(&path).unwrap(), Settings::default());
        let settings = Settings {
            hide_launcher: true,
            details: false,
        };
        settings.save_to(&path).unwrap();
        assert_eq!(Settings::load_from(&path).unwrap(), settings);
        fs::write(&path, r#"{"hide_launcher":true}"#).unwrap();
        assert_eq!(
            Settings::load_from(&path).unwrap(),
            Settings {
                hide_launcher: true,
                details: false
            }
        );
        fs::write(&path, "broken json").unwrap();
        assert!(Settings::load_from(&path).is_err());
        fs::remove_dir_all(directory).unwrap();
    }
}

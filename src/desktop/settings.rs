use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

const MAX_RECENT: usize = 10;
const MAX_SETTINGS_BYTES: u64 = 128 * 1024;

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    #[default]
    Dark,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct Preferences {
    pub theme: Theme,
    pub recent: Vec<PathBuf>,
}

impl Preferences {
    fn remember(&mut self, path: &Path) {
        let path = normalized(path);
        self.recent.retain(|previous| normalized(previous) != path);
        self.recent.insert(0, path);
        self.recent.truncate(MAX_RECENT);
    }
}

fn normalized(path: &Path) -> PathBuf {
    fs::canonicalize(path)
        .or_else(|_| std::path::absolute(path))
        .unwrap_or_else(|_| path.into())
}

pub struct Settings {
    directory: Option<PathBuf>,
    pub preferences: Preferences,
    pub warning: Option<String>,
}

impl Settings {
    pub fn load() -> Self {
        let directory = std::env::var_os("SLX_VIEWER_SETTINGS_DIR")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("APPDATA").map(|path| PathBuf::from(path).join("slx_viewer"))
            });
        match directory {
            Some(directory) => Self::at(directory),
            None => Self {
                directory: None,
                preferences: Preferences::default(),
                warning: Some("The application settings directory is unavailable.".into()),
            },
        }
    }

    pub fn at(directory: PathBuf) -> Self {
        match read(&directory) {
            Ok(mut preferences) => {
                preferences.recent.truncate(MAX_RECENT);
                Self {
                    directory: Some(directory),
                    preferences,
                    warning: None,
                }
            }
            Err(error) => Self {
                directory: Some(directory),
                preferences: Preferences::default(),
                warning: Some(format!("Could not read application settings. {error}")),
            },
        }
    }

    pub fn record_success(&mut self, path: &Path) {
        self.preferences.remember(path);
        if let Err(error) = self.save_recent(path) {
            self.warning = Some(format!("Could not save recent models. {error}"));
        }
    }

    fn save_recent(&mut self, path: &Path) -> io::Result<()> {
        let directory = self
            .directory
            .as_ref()
            .ok_or_else(|| io::Error::other("Settings directory is unavailable"))?;
        fs::create_dir_all(directory)?;
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(directory.join("settings.lock"))?;
        lock.try_lock().map_err(io::Error::other)?;
        let mut latest = match read(directory) {
            Ok(preferences) => preferences,
            Err(error) if error.kind() == io::ErrorKind::InvalidData => Preferences::default(),
            Err(error) => return Err(error),
        };
        latest.theme = self.preferences.theme.clone();
        latest.remember(path);
        let mut temporary = tempfile::NamedTempFile::new_in(directory)?;
        serde_json::to_writer_pretty(&mut temporary, &latest)?;
        temporary.write_all(b"\n")?;
        temporary.as_file().sync_all()?;
        temporary
            .persist(directory.join("settings.json"))
            .map_err(|error| error.error)?;
        self.preferences = latest;
        self.warning = None;
        Ok(())
    }
}

fn read(directory: &Path) -> io::Result<Preferences> {
    let file = match File::open(directory.join("settings.json")) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Preferences::default()),
        Err(error) => return Err(error),
    };
    let mut contents = Vec::new();
    file.take(MAX_SETTINGS_BYTES + 1)
        .read_to_end(&mut contents)?;
    if contents.len() as u64 > MAX_SETTINGS_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Settings file is too large",
        ));
    }
    serde_json::from_slice(&contents)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recent_models_persist_deduplicate_and_merge_across_instances() {
        let root = tempfile::tempdir().unwrap();
        let directory = root.path().join("settings");
        let mut first = Settings::at(directory.clone());
        let mut second = Settings::at(directory.clone());
        for index in 0..12 {
            let path = root.path().join(format!("model {index} Ω.slx"));
            fs::write(&path, b"fixture").unwrap();
            first.record_success(&path);
        }
        let latest = root.path().join("model 11 Ω.slx");
        second.record_success(&latest);
        second.record_success(&PathBuf::from(latest.to_string_lossy().to_uppercase()));
        assert!(second.warning.is_none());
        let loaded = Settings::at(directory);
        assert_eq!(loaded.preferences.recent.len(), 10);
        assert_eq!(loaded.preferences.recent[0], normalized(&latest));
        assert!(matches!(loaded.preferences.theme, Theme::Dark));
        assert_eq!(
            loaded
                .preferences
                .recent
                .iter()
                .filter(|path| **path == normalized(&latest))
                .count(),
            1
        );
        let before = loaded.preferences.recent.clone();
        fs::remove_file(&latest).unwrap();
        assert_eq!(
            Settings::at(root.path().join("settings"))
                .preferences
                .recent,
            before
        );
    }

    #[test]
    fn invalid_settings_recover_on_a_successful_load() {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("settings.json"), b"invalid").unwrap();
        let mut settings = Settings::at(root.path().into());
        assert!(settings.warning.is_some());
        settings.record_success(Path::new("tests/fixtures/dc_motor_speed_control.slx"));
        assert!(settings.warning.is_none());
        assert_eq!(Settings::at(root.path().into()).preferences.recent.len(), 1);
    }
}

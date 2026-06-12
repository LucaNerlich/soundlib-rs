//! Configuration loading, validation, and environment overrides.
//!
//! The config is a small YAML file (see [`Config`]) stored under the platform
//! config directory, e.g. `~/.config/soundlib/config.yaml` on Linux or
//! `~/Library/Application Support/soundlib/config.yaml` on macOS. It is created
//! with sensible defaults on first run.
//!
//! Two environment variables influence loading:
//!
//! - `SOUNDLIB_CONFIG` — use an alternate config file path.
//! - `SOUNDLIB_ROOT` — override [`Config::library_root`] for a single run.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// User configuration for the library scan and playback engine.
///
/// Serialized to and from YAML. Missing fields fall back to their defaults via
/// [`Config::default`] / `#[serde(default)]`, so older config files keep
/// loading after new fields are added.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Top-level directory scanned for audio files. Must exist and be a
    /// directory, otherwise loading fails validation. Defaults to the OS music
    /// folder.
    pub library_root: PathBuf,
    /// File extensions to include when scanning, matched case-insensitively
    /// with any leading dot stripped (see [`Config::extension_set`]).
    pub audio_extensions: Vec<String>,
    /// Output volume multiplier. `1.0` is the unmodified signal; the engine
    /// clamps applied values to the range `0.0..=2.0`.
    #[serde(default = "default_volume")]
    pub volume: f32,
}

fn default_volume() -> f32 {
    1.0
}

/// Cross-platform default library location: the OS "Music" directory when it
/// can be resolved, otherwise the user's home directory.
fn default_library_root() -> PathBuf {
    dirs::audio_dir()
        .or_else(dirs::home_dir)
        .unwrap_or_else(|| PathBuf::from("."))
}

impl Default for Config {
    fn default() -> Self {
        Self {
            library_root: default_library_root(),
            audio_extensions: vec![
                "mp3".into(),
                "flac".into(),
                "ogg".into(),
                "opus".into(),
                "wav".into(),
                "m4a".into(),
            ],
            volume: 1.0,
        }
    }
}

impl Config {
    /// Load the configuration from the default path (or `SOUNDLIB_CONFIG`),
    /// creating it with defaults if absent, applying environment overrides,
    /// and validating the result.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be read or parsed, or if validation
    /// fails (for example, the library root does not exist).
    pub fn load() -> Result<Self> {
        Self::load_from_path(&config_path()?, true)
    }

    pub(crate) fn load_from_path(path: &Path, apply_env: bool) -> Result<Self> {
        let mut config = if path.exists() {
            let contents = fs::read_to_string(path)
                .with_context(|| format!("reading config at {}", path.display()))?;
            serde_yaml_ng::from_str(&contents)
                .with_context(|| format!("parsing config at {}", path.display()))?
        } else {
            let config = Config::default();
            write_config(path, &config)?;
            config
        };

        if apply_env {
            config.apply_env_overrides();
        }
        config.validate()?;
        Ok(config)
    }

    pub(crate) fn apply_env_overrides(&mut self) {
        if let Ok(root) = env::var("SOUNDLIB_ROOT") {
            self.library_root = PathBuf::from(root);
        }
    }

    pub(crate) fn validate(&self) -> Result<()> {
        if !self.library_root.is_dir() {
            anyhow::bail!(
                "library root does not exist or is not a directory: {}",
                self.library_root.display()
            );
        }
        Ok(())
    }

    /// Normalize [`Config::audio_extensions`] into a lookup set: leading dots
    /// removed and lowercased, ready for case-insensitive matching during the
    /// library scan.
    pub fn extension_set(&self) -> std::collections::HashSet<String> {
        self.audio_extensions
            .iter()
            .map(|ext| ext.trim_start_matches('.').to_ascii_lowercase())
            .collect()
    }
}

fn config_path() -> Result<PathBuf> {
    if let Ok(path) = env::var("SOUNDLIB_CONFIG") {
        return Ok(PathBuf::from(path));
    }

    let config_dir = dirs::config_dir().context("could not determine config directory")?;
    Ok(config_dir.join("soundlib").join("config.yaml"))
}

fn write_config(path: &Path, config: &Config) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("creating config directory {}", parent.display()))?;
    }

    let contents = serde_yaml_ng::to_string(config).context("serializing default config")?;
    fs::write(path, contents)
        .with_context(|| format!("writing config to {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    use crate::library::scan_library;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn default_config_values() {
        let config = Config::default();
        assert_eq!(config.audio_extensions.len(), 6);
        assert_eq!(config.volume, 1.0);
    }

    #[test]
    fn write_and_load_roundtrip() {
        let dir = TempDir::new().expect("tempdir");
        fs::create_dir_all(dir.path().join("music")).expect("music dir");
        let config_path = dir.path().join("config.yaml");

        let original = Config {
            library_root: dir.path().join("music"),
            audio_extensions: vec!["mp3".into()],
            volume: 0.5,
        };
        write_config(&config_path, &original).expect("write");

        let loaded = Config::load_from_path(&config_path, false).expect("load");
        assert_eq!(loaded.library_root, original.library_root);
        assert_eq!(loaded.volume, 0.5);
        assert_eq!(loaded.audio_extensions, vec!["mp3"]);
    }

    #[test]
    fn load_from_path_rejects_invalid_yaml() {
        let file = NamedTempFile::new().expect("temp file");
        fs::write(file.path(), "not: [valid: yaml").expect("write bad yaml");

        let err = Config::load_from_path(file.path(), false).expect_err("bad yaml");
        assert!(err.to_string().contains("parsing config"));
    }

    #[test]
    fn config_works_with_library_scan() {
        let dir = TempDir::new().expect("tempdir");
        let root = dir.path().join("library");
        fs::create_dir_all(&root).expect("root");
        fs::write(root.join("song.mp3"), b"mp3").expect("song");

        let config = Config {
            library_root: root,
            ..Config::default()
        };
        config.validate().expect("validate");

        let tree = scan_library(&config.library_root, &config.extension_set()).expect("scan");
        assert_eq!(tree.children.len(), 1);
    }
}

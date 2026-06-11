mod common;

use std::path::PathBuf;

use soundlib_rs::config::Config;
use temp_env::with_vars;
use tempfile::NamedTempFile;

use common::{write_config, MockCliamp, TestLibrary};

#[test]
fn default_config_has_expected_extensions() {
    let config = Config::default();
    let extensions = config.extension_set();

    for ext in ["mp3", "flac", "ogg", "opus", "wav", "m4a"] {
        assert!(extensions.contains(ext));
    }
}

#[test]
fn extension_set_strips_leading_dots_and_lowercases() {
    let mut config = Config::default();
    config.audio_extensions = vec![".MP3".into(), "Flac".into()];

    let extensions = config.extension_set();
    assert!(extensions.contains("mp3"));
    assert!(extensions.contains("flac"));
    assert_eq!(extensions.len(), 2);
}

#[test]
fn load_uses_yaml_and_env_overrides() {
    let lib = TestLibrary::minimal();
    let mock = MockCliamp::success();
    let config_file = NamedTempFile::new().expect("config file");

    write_config(
        config_file.path(),
        &PathBuf::from("/overridden/by/env"),
        "cliamp",
    );

    with_vars(
        [
            ("SOUNDLIB_CONFIG", Some(config_file.path().to_str().unwrap())),
            ("SOUNDLIB_ROOT", Some(lib.root.to_str().unwrap())),
            ("SOUNDLIB_CLIAMP_BIN", Some(mock.bin.to_str().unwrap())),
        ],
        || {
            let config = Config::load().expect("load config");
            assert_eq!(config.library_root, lib.root);
            assert_eq!(config.cliamp_bin, mock.bin.to_str().unwrap());
        },
    );
}

#[test]
fn load_fails_when_library_root_missing() {
    let config_file = NamedTempFile::new().expect("config file");
    write_config(
        config_file.path(),
        &PathBuf::from("/nonexistent/soundlib-missing-root"),
        "cliamp",
    );

    with_vars(
        [
            ("SOUNDLIB_CONFIG", Some(config_file.path().to_str().unwrap())),
            ("SOUNDLIB_ROOT", None::<&str>),
        ],
        || {
            let err = Config::load().expect_err("missing root");
            assert!(err.to_string().contains("does not exist"));
        },
    );
}

#[test]
fn config_roundtrips_through_yaml() {
    let lib = TestLibrary::minimal();
    let config_file = NamedTempFile::new().expect("config file");

    write_config(config_file.path(), &lib.root, "cliamp");

    with_vars(
        [
            ("SOUNDLIB_CONFIG", Some(config_file.path().to_str().unwrap())),
            ("SOUNDLIB_ROOT", Some(lib.root.to_str().unwrap())),
        ],
        || {
            let loaded = Config::load().expect("load");
            let yaml = serde_yaml_ng::to_string(&loaded).expect("serialize");
            let again: Config = serde_yaml_ng::from_str(&yaml).expect("deserialize");
            assert_eq!(again.library_root, lib.root);
            assert_eq!(again.cliamp_bin, "cliamp");
        },
    );
}

#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use soundlib_rs::library::{scan_library, LibraryNode, NodeKind};
use tempfile::TempDir;

pub fn extensions_mp3_flac() -> std::collections::HashSet<String> {
    ["mp3", "flac", "ogg", "wav"]
        .into_iter()
        .map(str::to_string)
        .collect()
}

pub struct TestLibrary {
    pub _dir: TempDir,
    pub root: PathBuf,
}

impl TestLibrary {
    pub fn minimal() -> Self {
        let dir = TempDir::new().expect("tempdir");
        let root = dir.path().join("library");
        fs::create_dir_all(&root).expect("create root");

        fs::create_dir_all(root.join("alpha")).expect("alpha dir");
        fs::write(root.join("alpha/01-intro.mp3"), b"mp3").expect("alpha track 1");
        fs::write(root.join("alpha/10-outro.mp3"), b"mp3").expect("alpha track 2");
        fs::write(root.join("alpha/cover.jpg"), b"jpg").expect("non-audio");

        fs::create_dir_all(root.join("beta")).expect("beta dir");
        fs::write(root.join("beta/theme.FLAC"), b"flac").expect("beta flac");

        fs::create_dir_all(root.join("gamma/nested")).expect("nested dir");
        fs::write(root.join("gamma/nested/deep.wav"), b"wav").expect("nested wav");
        fs::write(root.join("gamma/readme.txt"), b"txt").expect("non-audio");

        fs::write(root.join("loose.ogg"), b"ogg").expect("loose ogg");
        fs::write(root.join("notes.md"), b"md").expect("non-audio");

        Self {
            _dir: dir,
            root: root.clone(),
        }
    }

    pub fn empty_album() -> Self {
        let dir = TempDir::new().expect("tempdir");
        let root = dir.path().join("empty-lib");
        fs::create_dir_all(&root).expect("create root");
        fs::create_dir_all(root.join("no-tracks")).expect("empty album");
        Self {
            _dir: dir,
            root,
        }
    }

    pub fn scan(&self) -> LibraryNode {
        scan_library(&self.root, &extensions_mp3_flac()).expect("scan test library")
    }
}

pub struct MockCliamp {
    pub _dir: TempDir,
    pub bin: PathBuf,
    pub log: PathBuf,
}

impl MockCliamp {
    pub fn success() -> Self {
        let dir = TempDir::new().expect("tempdir");
        let log = dir.path().join("cliamp.log");
        let socket = dir.path().join("cliamp.sock");
        let bin = dir.path().join("mock_cliamp.sh");
        fs::write(
            &bin,
            format!(
                r#"#!/bin/sh
echo "$@" >> "{}"
if echo "$@" | grep -q -- --daemon; then
  touch "{}"
fi
if echo "$@" | grep -q -- "status --json"; then
  printf '%s\n' '{{"ok":true,"state":"playing","track":{{"title":"Mock Track","artist":"Mock Artist","path":"/mock/track.mp3"}},"position":12,"duration":180,"total":1,"shuffle":false,"repeat":"off"}}'
fi
exit 0
"#,
                log.display(),
                socket.display()
            ),
        )
        .expect("write mock cliamp");
        make_executable(&bin);
        Self {
            _dir: dir,
            bin,
            log,
        }
    }

    pub fn failing(message: &str) -> Self {
        let dir = TempDir::new().expect("tempdir");
        let log = dir.path().join("cliamp.log");
        let bin = dir.path().join("mock_cliamp_fail.sh");
        fs::write(
            &bin,
            format!(
                "#!/bin/sh\necho \"$@\" >> \"{}\"\necho \"{message}\" >&2\nexit 1\n",
                log.display()
            ),
        )
        .expect("write failing mock cliamp");
        make_executable(&bin);
        Self {
            _dir: dir,
            bin,
            log,
        }
    }

    pub fn socket_path(&self) -> PathBuf {
        self._dir.path().join("cliamp.sock")
    }

    pub fn log_lines(&self) -> Vec<String> {
        fs::read_to_string(&self.log)
            .unwrap_or_default()
            .lines()
            .map(str::to_string)
            .collect()
    }
}

pub fn write_config(path: &Path, library_root: &Path, cliamp_bin: &str) {
    let yaml = format!(
        "library_root: {}\naudio_extensions:\n  - mp3\n  - flac\n  - ogg\n  - wav\ncliamp_bin: {cliamp_bin}\n",
        library_root.display()
    );
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("config parent");
    }
    fs::write(path, yaml).expect("write config");
}

fn make_executable(path: &Path) {
    Command::new("chmod")
        .args(["+x", path.to_str().expect("utf8 path")])
        .status()
        .expect("chmod mock cliamp");
}

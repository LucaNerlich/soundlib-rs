use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::library::{scan_library, LibraryNode, NodeKind};
use crate::playback::PlaybackInfo;
use crate::player::PlaybackEngine;
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

/// A test double for [`PlaybackEngine`] that records the commands it receives
/// and returns a caller-controlled snapshot. Clones share the same underlying
/// state, so a test can hold a handle while the `App` owns a boxed clone.
#[derive(Clone, Default)]
pub struct RecordingEngine {
    commands: Arc<Mutex<Vec<String>>>,
    last_tracks: Arc<Mutex<Vec<PathBuf>>>,
    snapshot: Arc<Mutex<Option<PlaybackInfo>>>,
}

impl RecordingEngine {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn commands(&self) -> Vec<String> {
        self.commands.lock().unwrap().clone()
    }

    pub fn last_tracks(&self) -> Vec<PathBuf> {
        self.last_tracks.lock().unwrap().clone()
    }

    #[allow(dead_code)]
    pub fn set_snapshot(&self, info: Option<PlaybackInfo>) {
        *self.snapshot.lock().unwrap() = info;
    }

    fn record(&self, label: &str) {
        self.commands.lock().unwrap().push(label.to_string());
    }
}

impl PlaybackEngine for RecordingEngine {
    fn play_replace(&self, tracks: Vec<PathBuf>) {
        *self.last_tracks.lock().unwrap() = tracks;
        self.record("play_replace");
    }

    fn append(&self, tracks: Vec<PathBuf>) {
        *self.last_tracks.lock().unwrap() = tracks;
        self.record("append");
    }

    fn toggle(&self) {
        self.record("toggle");
    }

    fn next(&self) {
        self.record("next");
    }

    fn prev(&self) {
        self.record("prev");
    }

    fn stop(&self) {
        self.record("stop");
    }

    fn shuffle_toggle(&self) {
        self.record("shuffle_toggle");
    }

    fn repeat_cycle(&self) {
        self.record("repeat_cycle");
    }

    fn set_volume(&self, _volume: f32) {
        self.record("set_volume");
    }

    fn snapshot(&self) -> Option<PlaybackInfo> {
        self.snapshot.lock().unwrap().clone()
    }

    fn shutdown(&self) {
        self.record("shutdown");
    }
}

pub fn write_config(path: &Path, library_root: &Path) {
    let yaml = format!(
        "library_root: {}\naudio_extensions:\n  - mp3\n  - flac\n  - ogg\n  - wav\nvolume: 1.0\n",
        library_root.display()
    );
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("config parent");
    }
    fs::write(path, yaml).expect("write config");
}

pub fn node(root: PathBuf, name: &str, children: Vec<LibraryNode>) -> LibraryNode {
    LibraryNode {
        name: name.into(),
        path: root,
        kind: NodeKind::Folder,
        children,
    }
}

pub fn file_node(parent: &Path, name: &str) -> LibraryNode {
    LibraryNode {
        name: name.into(),
        path: parent.join(name),
        kind: NodeKind::File,
        children: Vec::new(),
    }
}

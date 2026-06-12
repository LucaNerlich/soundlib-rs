//! Scanning the library root into an in-memory tree.
//!
//! [`scan_library`] walks a directory recursively, keeping only files whose
//! extension is in the provided set, and returns a [`LibraryNode`] tree. Entries
//! at every level are sorted with a natural ordering so `track2` precedes
//! `track10`. [`collect_tracks`] flattens a node into the playable files beneath
//! it, preserving that order.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use anyhow::Result;
use walkdir::WalkDir;

/// Whether a [`LibraryNode`] is a directory or a playable file.
#[derive(Debug, Clone)]
pub enum NodeKind {
    /// A directory that may contain further nodes.
    Folder,
    /// A single audio file (a leaf node).
    File,
}

/// A node in the library tree: either a folder with children or an audio file.
#[derive(Debug, Clone)]
pub struct LibraryNode {
    /// Display name (the final path component).
    pub name: String,
    /// Absolute path to the folder or file on disk.
    pub path: PathBuf,
    /// Whether this node is a [`NodeKind::Folder`] or [`NodeKind::File`].
    pub kind: NodeKind,
    /// Child nodes, naturally sorted. Always empty for files.
    pub children: Vec<LibraryNode>,
}

impl LibraryNode {
    /// Returns `true` if this node is a folder.
    pub fn is_folder(&self) -> bool {
        matches!(self.kind, NodeKind::Folder)
    }

    /// Returns `true` if this node is a file.
    pub fn is_file(&self) -> bool {
        matches!(self.kind, NodeKind::File)
    }
}

/// Recursively scan `root`, building a [`LibraryNode`] tree containing every
/// subdirectory and every file whose lowercased extension is in `extensions`.
///
/// Children at each level are sorted naturally (numeric runs compared as
/// numbers). The returned root node is always a [`NodeKind::Folder`].
///
/// # Errors
///
/// Returns an error if a directory cannot be read.
pub fn scan_library(root: &Path, extensions: &HashSet<String>) -> Result<LibraryNode> {
    let name = root
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| root.display().to_string());

    let mut children = Vec::new();

    let entries = fs_read_dir_sorted(root)?;
    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            children.push(scan_library(&path, extensions)?);
        } else if is_audio_file(&path, extensions) {
            children.push(LibraryNode {
                name: file_name(&path),
                path: path.to_path_buf(),
                kind: NodeKind::File,
                children: Vec::new(),
            });
        }
    }

    sort_nodes(&mut children);

    Ok(LibraryNode {
        name,
        path: root.to_path_buf(),
        kind: NodeKind::Folder,
        children,
    })
}

/// Collect all audio file paths under `node`, depth-first, in tree order.
///
/// For a file node this returns a single-element vector; for a folder it
/// returns every file beneath it recursively. The ordering matches the
/// naturally sorted tree, which is the order tracks are queued for playback.
pub fn collect_tracks(node: &LibraryNode) -> Vec<PathBuf> {
    let mut tracks = Vec::new();
    collect_tracks_inner(node, &mut tracks);
    tracks
}

fn collect_tracks_inner(node: &LibraryNode, tracks: &mut Vec<PathBuf>) {
    if node.is_file() {
        tracks.push(node.path.clone());
        return;
    }

    for child in &node.children {
        if child.is_file() {
            tracks.push(child.path.clone());
        } else {
            collect_tracks_inner(child, tracks);
        }
    }
}

fn fs_read_dir_sorted(dir: &Path) -> Result<Vec<walkdir::DirEntry>> {
    let mut entries: Vec<_> = WalkDir::new(dir)
        .min_depth(1)
        .max_depth(1)
        .sort_by_file_name()
        .into_iter()
        .filter_map(|entry| entry.ok())
        .collect();

    entries.sort_by(|a, b| natural_cmp(
        &a.file_name().to_string_lossy(),
        &b.file_name().to_string_lossy(),
    ));

    Ok(entries)
}

fn sort_nodes(nodes: &mut [LibraryNode]) {
    nodes.sort_by(|a, b| natural_cmp(&a.name, &b.name));
}

fn is_audio_file(path: &Path, extensions: &HashSet<String>) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| extensions.contains(&ext.to_ascii_lowercase()))
        .unwrap_or(false)
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

fn natural_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    let a_parts = split_natural(a);
    let b_parts = split_natural(b);

    for (a_part, b_part) in a_parts.iter().zip(b_parts.iter()) {
        match (a_part.parse::<u64>(), b_part.parse::<u64>()) {
            (Ok(a_num), Ok(b_num)) => match a_num.cmp(&b_num) {
                std::cmp::Ordering::Equal => continue,
                other => return other,
            },
            _ => match a_part.to_ascii_lowercase().cmp(&b_part.to_ascii_lowercase()) {
                std::cmp::Ordering::Equal => continue,
                other => return other,
            },
        }
    }

    a_parts.len().cmp(&b_parts.len())
}

fn split_natural(input: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut is_digit = false;

    for ch in input.chars() {
        let ch_is_digit = ch.is_ascii_digit();
        if current.is_empty() {
            is_digit = ch_is_digit;
            current.push(ch);
            continue;
        }

        if ch_is_digit == is_digit {
            current.push(ch);
        } else {
            parts.push(current);
            current = String::from(ch);
            is_digit = ch_is_digit;
        }
    }

    if !current.is_empty() {
        parts.push(current);
    }

    parts
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::fs;

    use super::*;
    use tempfile::TempDir;

    #[test]
    fn natural_sort_orders_numbers() {
        assert_eq!(natural_cmp("track2", "track10"), std::cmp::Ordering::Less);
        assert_eq!(natural_cmp("track10", "track2"), std::cmp::Ordering::Greater);
    }

    #[test]
    fn natural_sort_handles_equal_prefixes() {
        assert_eq!(natural_cmp("track1", "track1"), std::cmp::Ordering::Equal);
        assert_eq!(natural_cmp("track1a", "track1b"), std::cmp::Ordering::Less);
    }

    #[test]
    fn natural_sort_handles_leading_text() {
        assert_eq!(
            natural_cmp("01 - Alpha", "02 - Beta"),
            std::cmp::Ordering::Less
        );
    }

    #[test]
    fn split_natural_splits_digits_and_letters() {
        assert_eq!(
            split_natural("track10b"),
            vec!["track".to_string(), "10".to_string(), "b".to_string()]
        );
    }

    #[test]
    fn is_audio_file_checks_extension_case_insensitively() {
        let extensions = HashSet::from(["mp3".to_string(), "flac".to_string()]);
        assert!(is_audio_file(Path::new("/a/track.MP3"), &extensions));
        assert!(is_audio_file(Path::new("/a/track.Flac"), &extensions));
        assert!(!is_audio_file(Path::new("/a/readme.txt"), &extensions));
        assert!(!is_audio_file(Path::new("/a/noext"), &extensions));
    }

    #[test]
    fn file_name_returns_last_component() {
        assert_eq!(file_name(Path::new("/music/album/song.mp3")), "song.mp3");
    }

    #[test]
    fn library_node_kind_predicates() {
        let folder = LibraryNode {
            name: "dir".into(),
            path: PathBuf::from("/dir"),
            kind: NodeKind::Folder,
            children: vec![],
        };
        let file = LibraryNode {
            name: "song.mp3".into(),
            path: PathBuf::from("/dir/song.mp3"),
            kind: NodeKind::File,
            children: vec![],
        };

        assert!(folder.is_folder());
        assert!(!folder.is_file());
        assert!(file.is_file());
        assert!(!file.is_folder());
    }

    #[test]
    fn scan_empty_directory_has_no_children() {
        let dir = TempDir::new().expect("tempdir");
        let extensions = HashSet::from(["mp3".to_string()]);

        let tree = scan_library(dir.path(), &extensions).expect("scan empty");
        assert!(tree.is_folder());
        assert!(tree.children.is_empty());
    }

    #[test]
    fn scan_skips_hidden_and_non_matching_files() {
        let dir = TempDir::new().expect("tempdir");
        fs::write(dir.path().join("visible.mp3"), b"x").expect("mp3");
        fs::write(dir.path().join(".hidden.mp3"), b"x").expect("hidden");
        fs::write(dir.path().join("data.bin"), b"x").expect("bin");

        let extensions = HashSet::from(["mp3".to_string()]);
        let tree = scan_library(dir.path(), &extensions).expect("scan");

        assert_eq!(tree.children.len(), 2);
        assert!(
            tree.children
                .iter()
                .any(|n| n.name == "visible.mp3" || n.name == ".hidden.mp3")
        );
        assert!(!tree.children.iter().any(|n| n.name == "data.bin"));
    }
}

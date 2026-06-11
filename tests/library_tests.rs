mod common;

use std::collections::HashSet;
use std::path::Path;

use soundlib_rs::library::{collect_tracks, scan_library, NodeKind};

use common::{extensions_mp3_flac, TestLibrary};

#[test]
fn scan_includes_only_audio_files() {
    let lib = TestLibrary::minimal();
    let tree = lib.scan();

    let names: Vec<_> = tree
        .children
        .iter()
        .flat_map(|album| {
            album
                .children
                .iter()
                .map(|child| (album.name.clone(), child.name.clone()))
        })
        .collect();

    assert!(names.contains(&("alpha".into(), "01-intro.mp3".into())));
    assert!(names.contains(&("alpha".into(), "10-outro.mp3".into())));
    assert!(names.contains(&("beta".into(), "theme.FLAC".into())));
    assert!(!names.iter().any(|(_, n)| n == "cover.jpg"));
    assert!(!names.iter().any(|(_, n)| n == "readme.txt"));
    assert!(!names.iter().any(|(_, n)| n == "notes.md"));
}

#[test]
fn scan_includes_loose_audio_at_root() {
    let lib = TestLibrary::minimal();
    let tree = lib.scan();

    assert!(
        tree.children
            .iter()
            .any(|n| n.is_file() && n.name == "loose.ogg")
    );
}

#[test]
fn scan_includes_nested_directories() {
    let lib = TestLibrary::minimal();
    let tree = lib.scan();

    let gamma = tree.children.iter().find(|n| n.name == "gamma").expect("gamma");
    let nested = gamma
        .children
        .iter()
        .find(|n| n.name == "nested")
        .expect("nested");
    assert_eq!(nested.children.len(), 1);
    assert_eq!(nested.children[0].name, "deep.wav");
}

#[test]
fn scan_sorts_albums_naturally() {
    let lib = TestLibrary::minimal();
    let tree = lib.scan();

    let album_names: Vec<_> = tree
        .children
        .iter()
        .filter(|n| n.is_folder())
        .map(|n| n.name.as_str())
        .collect();

    assert_eq!(album_names, vec!["alpha", "beta", "gamma"]);
}

#[test]
fn scan_sorts_tracks_naturally_within_album() {
    let lib = TestLibrary::minimal();
    let tree = lib.scan();

    let alpha = tree.children.iter().find(|n| n.name == "alpha").expect("alpha");
    let track_names: Vec<_> = alpha.children.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(track_names, vec!["01-intro.mp3", "10-outro.mp3"]);
}

#[test]
fn scan_treats_extension_case_insensitively() {
    let lib = TestLibrary::minimal();
    let tree = lib.scan();

    let beta = tree.children.iter().find(|n| n.name == "beta").expect("beta");
    assert_eq!(beta.children.len(), 1);
    assert!(beta.children[0].is_file());
}

#[test]
fn collect_tracks_from_folder_is_recursive_and_sorted() {
    let lib = TestLibrary::minimal();
    let tree = lib.scan();

    let gamma = tree.children.iter().find(|n| n.name == "gamma").expect("gamma");
    let tracks = collect_tracks(gamma);

    assert_eq!(tracks.len(), 1);
    assert!(tracks[0].ends_with("gamma/nested/deep.wav"));
}

#[test]
fn collect_tracks_from_root_includes_all_audio() {
    let lib = TestLibrary::minimal();
    let tree = lib.scan();
    let tracks = collect_tracks(&tree);

    assert_eq!(tracks.len(), 5);
    let names: Vec<_> = tracks
        .iter()
        .filter_map(|p| p.file_name().and_then(|n| n.to_str()))
        .collect();
    assert!(names.contains(&"01-intro.mp3"));
    assert!(names.contains(&"10-outro.mp3"));
    assert!(names.contains(&"theme.FLAC"));
    assert!(names.contains(&"deep.wav"));
    assert!(names.contains(&"loose.ogg"));
}

#[test]
fn collect_tracks_from_single_file_returns_one() {
    let lib = TestLibrary::minimal();
    let tree = lib.scan();
    let loose = tree
        .children
        .iter()
        .find(|n| n.name == "loose.ogg")
        .expect("loose");

    let tracks = collect_tracks(loose);
    assert_eq!(tracks.len(), 1);
    assert_eq!(tracks[0], loose.path);
}

#[test]
fn collect_tracks_from_empty_folder_returns_empty() {
    let lib = TestLibrary::empty_album();
    let tree = lib.scan();
    let empty = tree
        .children
        .iter()
        .find(|n| n.name == "no-tracks")
        .expect("empty album");

    assert!(collect_tracks(empty).is_empty());
}

#[test]
fn scan_respects_custom_extension_filter() {
    let lib = TestLibrary::minimal();
    let extensions = HashSet::from(["mp3".to_string()]);

    let tree = scan_library(&lib.root, &extensions).expect("scan");
    let all_files: Vec<_> = collect_tracks(&tree)
        .iter()
        .filter_map(|p| p.extension().and_then(|e| e.to_str()))
        .map(str::to_ascii_lowercase)
        .collect();

    assert!(all_files.iter().all(|ext| ext == "mp3"));
    assert_eq!(all_files.len(), 2);
}

#[test]
fn scan_root_node_is_folder_named_after_directory() {
    let lib = TestLibrary::minimal();
    let tree = lib.scan();

    assert!(tree.is_folder());
    assert_eq!(tree.name, "library");
    assert_eq!(tree.path, lib.root);
    assert!(matches!(tree.kind, NodeKind::Folder));
}

#[test]
fn scan_soundtracks_library_when_present() {
    let root = Path::new("/home/luca/Nextcloud/_media/Soundtracks");
    if !root.is_dir() {
        return;
    }

    let tree = scan_library(root, &extensions_mp3_flac()).expect("scan soundtracks");
    assert!(!tree.children.is_empty());
}

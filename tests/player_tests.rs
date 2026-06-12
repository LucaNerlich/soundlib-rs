use std::path::PathBuf;

use rand::SeedableRng;
use rand::rngs::StdRng;
use soundlib_rs::player::{PlayState, Playlist, Repeat};

fn paths(names: &[&str]) -> Vec<PathBuf> {
    names.iter().map(PathBuf::from).collect()
}

fn seeded() -> StdRng {
    StdRng::seed_from_u64(7)
}

#[test]
fn set_tracks_positions_at_first_track() {
    let mut rng = seeded();
    let mut playlist = Playlist::new();
    playlist.set_tracks(paths(&["a", "b", "c"]), &mut rng);

    assert_eq!(playlist.len(), 3);
    assert_eq!(playlist.current().unwrap(), &PathBuf::from("a"));
    assert_eq!(playlist.state, PlayState::Playing);
}

#[test]
fn manual_next_prev_wrap_around() {
    let mut rng = seeded();
    let mut playlist = Playlist::new();
    playlist.set_tracks(paths(&["a", "b", "c"]), &mut rng);

    assert_eq!(playlist.next().unwrap(), &PathBuf::from("b"));
    assert_eq!(playlist.next().unwrap(), &PathBuf::from("c"));
    assert_eq!(playlist.next().unwrap(), &PathBuf::from("a"));
    assert_eq!(playlist.prev().unwrap(), &PathBuf::from("c"));
}

#[test]
fn repeat_off_stops_at_end() {
    let mut rng = seeded();
    let mut playlist = Playlist::new();
    playlist.set_tracks(paths(&["a", "b"]), &mut rng);

    assert_eq!(playlist.advance_auto().unwrap(), &PathBuf::from("b"));
    assert!(playlist.advance_auto().is_none());
    assert_eq!(playlist.state, PlayState::Stopped);
}

#[test]
fn repeat_all_wraps_to_start() {
    let mut rng = seeded();
    let mut playlist = Playlist::new();
    playlist.set_tracks(paths(&["a", "b"]), &mut rng);
    playlist.repeat = Repeat::All;

    assert_eq!(playlist.advance_auto().unwrap(), &PathBuf::from("b"));
    assert_eq!(playlist.advance_auto().unwrap(), &PathBuf::from("a"));
    assert_eq!(playlist.state, PlayState::Playing);
}

#[test]
fn repeat_one_replays_current() {
    let mut rng = seeded();
    let mut playlist = Playlist::new();
    playlist.set_tracks(paths(&["a", "b"]), &mut rng);
    playlist.repeat = Repeat::One;

    assert_eq!(playlist.advance_auto().unwrap(), &PathBuf::from("a"));
    assert_eq!(playlist.advance_auto().unwrap(), &PathBuf::from("a"));
}

#[test]
fn repeat_cycle_order() {
    let mut playlist = Playlist::new();
    assert_eq!(playlist.repeat, Repeat::Off);
    playlist.cycle_repeat();
    assert_eq!(playlist.repeat, Repeat::All);
    playlist.cycle_repeat();
    assert_eq!(playlist.repeat, Repeat::One);
    playlist.cycle_repeat();
    assert_eq!(playlist.repeat, Repeat::Off);
}

#[test]
fn shuffle_is_deterministic_for_a_seed_and_keeps_current() {
    let mut rng = seeded();
    let mut playlist = Playlist::new();
    playlist.set_tracks(paths(&["a", "b", "c", "d", "e", "f"]), &mut rng);
    playlist.next();
    let current = playlist.current().cloned().unwrap();

    playlist.toggle_shuffle(&mut rng);
    assert!(playlist.shuffle);
    assert_eq!(playlist.current().unwrap(), &current);

    playlist.toggle_shuffle(&mut rng);
    assert!(!playlist.shuffle);
    assert_eq!(playlist.current().unwrap(), &current);
}

#[test]
fn append_grows_the_queue() {
    let mut rng = seeded();
    let mut playlist = Playlist::new();
    playlist.set_tracks(paths(&["a"]), &mut rng);
    playlist.append(paths(&["b", "c"]));

    assert_eq!(playlist.len(), 3);
    assert_eq!(playlist.next().unwrap(), &PathBuf::from("b"));
}

#[test]
fn stop_marks_state_stopped() {
    let mut rng = seeded();
    let mut playlist = Playlist::new();
    playlist.set_tracks(paths(&["a"]), &mut rng);
    playlist.stop();
    assert_eq!(playlist.state, PlayState::Stopped);
}

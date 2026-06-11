mod common;

use soundlib_rs::player::Player;

use common::{MockCliamp, TestLibrary};

#[test]
fn queue_track_invokes_cliamp_with_path() {
    let mock = MockCliamp::success();
    std::fs::write(&mock.socket_path(), b"").expect("socket");
    let lib = TestLibrary::minimal();
    let track = lib.root.join("loose.ogg");

    let player = Player::with_socket_override(
        mock.bin.to_string_lossy().to_string(),
        false,
        mock.socket_path(),
    );
    player.queue_track(&track).expect("queue track");

    let lines = mock.log_lines();
    assert_eq!(lines.len(), 1);
    assert!(lines[0].contains("queue"));
    assert!(lines[0].contains("loose.ogg"));
}

#[test]
fn queue_tracks_invokes_cliamp_for_each_path() {
    let mock = MockCliamp::success();
    std::fs::write(&mock.socket_path(), b"").expect("socket");
    let lib = TestLibrary::minimal();
    let tracks = [
        lib.root.join("loose.ogg"),
        lib.root.join("alpha/01-intro.mp3"),
    ];

    let player = Player::with_socket_override(
        mock.bin.to_string_lossy().to_string(),
        false,
        mock.socket_path(),
    );
    let count = player.queue_tracks(&tracks).expect("queue tracks");

    assert_eq!(count, 2);
    assert_eq!(mock.log_lines().len(), 2);
}

#[test]
fn play_stop_and_transport_forward_arguments() {
    let mock = MockCliamp::success();
    std::fs::write(&mock.socket_path(), b"").expect("socket");
    let player = Player::with_socket_override(
        mock.bin.to_string_lossy().to_string(),
        false,
        mock.socket_path(),
    );

    player.play().expect("play");
    player.toggle().expect("toggle");
    player.next().expect("next");
    player.prev().expect("prev");
    player.stop().expect("stop");

    let lines = mock.log_lines();
    assert_eq!(lines, vec!["play", "toggle", "next", "prev", "stop"]);
}

#[test]
fn failed_cliamp_command_returns_stderr_message() {
    let mock = MockCliamp::failing("daemon not running");
    let player = Player::with_options(mock.bin.to_string_lossy().to_string(), false);

    let err = player.play().expect_err("play should fail");
    assert!(err.to_string().contains("daemon not running"));
}

#[test]
fn queue_tracks_stops_on_first_failure() {
    let mock = MockCliamp::failing("queue rejected");
    let lib = TestLibrary::minimal();
    let tracks = [
        lib.root.join("loose.ogg"),
        lib.root.join("alpha/01-intro.mp3"),
    ];

    let player = Player::with_options(mock.bin.to_string_lossy().to_string(), false);
    let err = player.queue_tracks(&tracks).expect_err("should fail on first");
    assert!(err.to_string().contains("queue rejected"));
    assert_eq!(mock.log_lines().len(), 1);
}

#[test]
fn cliamp_queue_smoke_when_daemon_running() {
    let track = std::path::Path::new(
        "/home/luca/Nextcloud/_media/Soundtracks/Barotrauma - Soundtrack/02 - Monster Nearby.flac",
    );
    if !track.is_file() {
        return;
    }

    let player = Player::new("cliamp");
    if player.stop().is_err() {
        return;
    }

    player.queue_track(track).expect("queue");
    player.stop().expect("stop");
}

//! The embedded playback engine and its audio-free playlist core.
//!
//! The UI talks to playback only through the [`PlaybackEngine`] trait. In
//! production that trait is implemented by [`RodioEngine`], which spawns a
//! dedicated background thread owning the (non-`Send`) audio device and a
//! [`rodio`] sink. Commands flow to that thread over a channel and a shared
//! [`PlaybackInfo`] snapshot flows back, polled by the TUI.
//!
//! All queue logic — ordering, the current position, shuffle, repeat, and
//! next/previous navigation — lives in [`Playlist`], which performs no audio
//! I/O and is therefore fully unit testable.
//!
//! ```no_run
//! use soundlib_rs::player::{PlaybackEngine, RodioEngine};
//! use std::path::PathBuf;
//!
//! let engine = RodioEngine::new();
//! engine.play_replace(vec![PathBuf::from("/music/a.mp3"), PathBuf::from("/music/b.flac")]);
//! engine.next();
//! engine.shutdown();
//! ```

use std::collections::HashMap;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use rand::rngs::StdRng;
use rand::seq::SliceRandom;
use rand::{Rng, SeedableRng};

use crate::playback::PlaybackInfo;

/// How often the audio thread wakes up to refresh the snapshot and check for
/// track completion when no command is pending.
const TICK: Duration = Duration::from_millis(100);

/// Repeat behaviour applied when a track finishes on its own.
///
/// Only affects [automatic advance](Playlist::advance_auto); manual
/// [`next`](Playlist::next)/[`prev`](Playlist::prev) always move and wrap.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Repeat {
    /// Stop after the last track.
    #[default]
    Off,
    /// Wrap to the first track after the last.
    All,
    /// Replay the current track indefinitely.
    One,
}

impl Repeat {
    /// Advance to the next mode in the cycle: `Off → All → One → Off`.
    pub fn cycle(self) -> Self {
        match self {
            Repeat::Off => Repeat::All,
            Repeat::All => Repeat::One,
            Repeat::One => Repeat::Off,
        }
    }

    /// The lowercase token used in [`PlaybackInfo`]: `"off"`, `"all"`, `"one"`.
    pub fn as_str(self) -> &'static str {
        match self {
            Repeat::Off => "off",
            Repeat::All => "all",
            Repeat::One => "one",
        }
    }
}

/// The logical playback state, independent of the audio backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PlayState {
    /// Nothing is playing.
    #[default]
    Stopped,
    /// A track is actively playing.
    Playing,
    /// A track is loaded but paused.
    Paused,
}

impl PlayState {
    /// The lowercase token used in [`PlaybackInfo`]: `"stopped"`, `"playing"`,
    /// `"paused"`.
    pub fn as_str(self) -> &'static str {
        match self {
            PlayState::Stopped => "stopped",
            PlayState::Playing => "playing",
            PlayState::Paused => "paused",
        }
    }
}

/// Pure, audio-free playlist bookkeeping. Owns the track ordering, the current
/// position, shuffle/repeat modes and the logical play state. This is kept free
/// of any `rodio` interaction so it can be unit tested deterministically.
///
/// Shuffle is modelled as a permutation (`order`) over the track indices, with
/// the current position indexing into that permutation; toggling shuffle keeps
/// the active track current.
#[derive(Debug, Default, Clone)]
pub struct Playlist {
    tracks: Vec<PathBuf>,
    order: Vec<usize>,
    pos: usize,
    /// Whether shuffle is enabled.
    pub shuffle: bool,
    /// Repeat mode applied on automatic advance.
    pub repeat: Repeat,
    /// Current logical play state.
    pub state: PlayState,
}

impl Playlist {
    /// Create an empty, stopped playlist.
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of tracks in the queue.
    pub fn len(&self) -> usize {
        self.tracks.len()
    }

    /// Returns `true` if the queue has no tracks.
    pub fn is_empty(&self) -> bool {
        self.tracks.is_empty()
    }

    /// Replace the whole queue and position at the first track.
    ///
    /// `rng` seeds the shuffle permutation when shuffle is enabled. The state
    /// becomes [`PlayState::Playing`] for a non-empty queue, otherwise
    /// [`PlayState::Stopped`].
    pub fn set_tracks<R: Rng>(&mut self, tracks: Vec<PathBuf>, rng: &mut R) {
        self.tracks = tracks;
        self.pos = 0;
        self.rebuild_order(rng);
        self.state = if self.tracks.is_empty() {
            PlayState::Stopped
        } else {
            PlayState::Playing
        };
    }

    /// Append tracks to the end of the queue, preserving the current position.
    pub fn append(&mut self, tracks: impl IntoIterator<Item = PathBuf>) {
        for track in tracks {
            let idx = self.tracks.len();
            self.tracks.push(track);
            self.order.push(idx);
        }
    }

    /// The path of the track at the current position, if any.
    pub fn current(&self) -> Option<&PathBuf> {
        self.order.get(self.pos).and_then(|&idx| self.tracks.get(idx))
    }

    /// Advance after a track has finished on its own, honouring repeat mode.
    /// Returns the track to play next, or `None` if playback should stop.
    pub fn advance_auto(&mut self) -> Option<&PathBuf> {
        if self.tracks.is_empty() {
            self.state = PlayState::Stopped;
            return None;
        }

        match self.repeat {
            Repeat::One => {}
            Repeat::All => {
                self.pos = (self.pos + 1) % self.order.len();
            }
            Repeat::Off => {
                if self.pos + 1 >= self.order.len() {
                    self.state = PlayState::Stopped;
                    return None;
                }
                self.pos += 1;
            }
        }

        self.state = PlayState::Playing;
        self.current()
    }

    /// Manual skip forward. Always wraps around the queue and keeps playing.
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Option<&PathBuf> {
        if self.tracks.is_empty() {
            return None;
        }
        self.pos = (self.pos + 1) % self.order.len();
        self.state = PlayState::Playing;
        self.current()
    }

    /// Manual skip backward. Always wraps around the queue and keeps playing.
    pub fn prev(&mut self) -> Option<&PathBuf> {
        if self.tracks.is_empty() {
            return None;
        }
        let len = self.order.len();
        self.pos = (self.pos + len - 1) % len;
        self.state = PlayState::Playing;
        self.current()
    }

    /// Mark playback as stopped. Leaves the queue and position intact.
    pub fn stop(&mut self) {
        self.state = PlayState::Stopped;
    }

    /// Advance the repeat mode (`Off → All → One → Off`).
    pub fn cycle_repeat(&mut self) {
        self.repeat = self.repeat.cycle();
    }

    /// Toggle shuffle while keeping the currently selected track active.
    pub fn toggle_shuffle<R: Rng>(&mut self, rng: &mut R) {
        let current_track = self.order.get(self.pos).copied();
        self.shuffle = !self.shuffle;
        self.rebuild_order(rng);

        if let Some(track_idx) = current_track
            && let Some(new_pos) = self.order.iter().position(|&idx| idx == track_idx)
        {
            if self.shuffle {
                // Keep the active track at the front so it keeps playing.
                self.order.swap(0, new_pos);
                self.pos = 0;
            } else {
                self.pos = new_pos;
            }
        }
    }

    fn rebuild_order<R: Rng>(&mut self, rng: &mut R) {
        self.order = (0..self.tracks.len()).collect();
        if self.shuffle {
            self.order.shuffle(rng);
        }
    }
}

/// Commands the TUI sends to the audio thread.
#[derive(Debug)]
enum Command {
    PlayReplace(Vec<PathBuf>),
    Append(Vec<PathBuf>),
    Toggle,
    Next,
    Prev,
    Stop,
    ShuffleToggle,
    RepeatCycle,
    SetVolume(f32),
    Shutdown,
}

/// Abstraction over the playback backend so the TUI can be exercised in tests
/// without a real audio device.
///
/// All control methods are fire-and-forget: they enqueue an action and return
/// immediately. Observe the result through [`snapshot`](PlaybackEngine::snapshot).
pub trait PlaybackEngine {
    /// Replace the queue with `tracks` and start playing from the first one.
    fn play_replace(&self, tracks: Vec<PathBuf>);
    /// Append `tracks` to the queue. If nothing is playing, playback starts.
    fn append(&self, tracks: Vec<PathBuf>);
    /// Toggle between playing and paused (starts the current track if stopped).
    fn toggle(&self);
    /// Skip to the next track, wrapping around at the end of the queue.
    fn next(&self);
    /// Skip to the previous track, wrapping around at the start of the queue.
    fn prev(&self);
    /// Stop playback and clear the current track.
    fn stop(&self);
    /// Toggle shuffle, keeping the current track active.
    fn shuffle_toggle(&self);
    /// Cycle the repeat mode (`off → all → one → off`).
    fn repeat_cycle(&self);
    /// Set the output volume multiplier (`1.0` = unmodified).
    fn set_volume(&self, volume: f32);
    /// The latest playback snapshot, or `None` when nothing is active.
    fn snapshot(&self) -> Option<PlaybackInfo>;
    /// Signal the backend to stop and release resources.
    fn shutdown(&self);
}

/// The real, `rodio`-backed engine. Owns a background thread that holds the
/// (non-`Send`) audio device and sink, and exposes a shared snapshot the UI
/// polls.
///
/// If no audio device can be opened, the engine still constructs and accepts
/// commands — they simply become no-ops — so the UI keeps working. The
/// background thread is stopped and joined on [`shutdown`](PlaybackEngine::shutdown)
/// and again on drop.
pub struct RodioEngine {
    cmd_tx: Sender<Command>,
    state: Arc<Mutex<PlaybackInfo>>,
    handle: Option<JoinHandle<()>>,
}

impl RodioEngine {
    /// Create the engine and spawn its background audio thread.
    pub fn new() -> Self {
        let (cmd_tx, cmd_rx) = mpsc::channel();
        let state = Arc::new(Mutex::new(PlaybackInfo::default()));
        let thread_state = Arc::clone(&state);
        let handle = thread::Builder::new()
            .name("soundlib-audio".into())
            .spawn(move || audio_thread(cmd_rx, thread_state))
            .ok();

        Self {
            cmd_tx,
            state,
            handle,
        }
    }

    fn send(&self, cmd: Command) {
        let _ = self.cmd_tx.send(cmd);
    }
}

impl Default for RodioEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl PlaybackEngine for RodioEngine {
    fn play_replace(&self, tracks: Vec<PathBuf>) {
        self.send(Command::PlayReplace(tracks));
    }

    fn append(&self, tracks: Vec<PathBuf>) {
        self.send(Command::Append(tracks));
    }

    fn toggle(&self) {
        self.send(Command::Toggle);
    }

    fn next(&self) {
        self.send(Command::Next);
    }

    fn prev(&self) {
        self.send(Command::Prev);
    }

    fn stop(&self) {
        self.send(Command::Stop);
    }

    fn shuffle_toggle(&self) {
        self.send(Command::ShuffleToggle);
    }

    fn repeat_cycle(&self) {
        self.send(Command::RepeatCycle);
    }

    fn set_volume(&self, volume: f32) {
        self.send(Command::SetVolume(volume));
    }

    fn snapshot(&self) -> Option<PlaybackInfo> {
        let info = self.state.lock().ok()?.clone();
        if info.is_active() {
            Some(info)
        } else {
            None
        }
    }

    fn shutdown(&self) {
        // Signal the audio thread to stop. The thread is joined in `Drop` so
        // this stays callable through a shared reference.
        self.send(Command::Shutdown);
    }
}

impl Drop for RodioEngine {
    fn drop(&mut self) {
        let _ = self.cmd_tx.send(Command::Shutdown);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

#[derive(Debug, Clone, Default)]
struct TrackMeta {
    title: String,
    artist: String,
    duration_secs: f64,
}

fn audio_thread(cmd_rx: Receiver<Command>, state: Arc<Mutex<PlaybackInfo>>) {
    let mut stream = match rodio::stream::DeviceSinkBuilder::open_default_sink() {
        Ok(stream) => stream,
        Err(_) => {
            // No audio device. Keep draining commands so the UI never blocks,
            // but playback is a no-op.
            drain_until_shutdown(&cmd_rx);
            return;
        }
    };
    // Avoid rodio printing to stderr on drop, which would corrupt the TUI.
    stream.log_on_drop(false);
    let sink = rodio::Player::connect_new(stream.mixer());

    let mut playlist = Playlist::new();
    let mut rng = StdRng::from_entropy();
    let mut meta_cache: HashMap<PathBuf, TrackMeta> = HashMap::new();
    let mut current_meta = TrackMeta::default();

    loop {
        match cmd_rx.recv_timeout(TICK) {
            Ok(Command::Shutdown) | Err(RecvTimeoutError::Disconnected) => {
                sink.stop();
                break;
            }
            Ok(command) => {
                handle_command(
                    command,
                    &sink,
                    &mut playlist,
                    &mut rng,
                    &mut meta_cache,
                    &mut current_meta,
                );
            }
            Err(RecvTimeoutError::Timeout) => {
                // Detect natural track completion and auto-advance.
                if playlist.state == PlayState::Playing && sink.empty() {
                    advance_and_play(
                        &sink,
                        &mut playlist,
                        &mut meta_cache,
                        &mut current_meta,
                    );
                }
            }
        }

        write_snapshot(&state, &sink, &playlist, &current_meta);
    }
}

fn handle_command(
    command: Command,
    sink: &rodio::Player,
    playlist: &mut Playlist,
    rng: &mut StdRng,
    meta_cache: &mut HashMap<PathBuf, TrackMeta>,
    current_meta: &mut TrackMeta,
) {
    match command {
        Command::PlayReplace(tracks) => {
            playlist.set_tracks(tracks, rng);
            start_playing_current(sink, playlist, meta_cache, current_meta);
        }
        Command::Append(tracks) => {
            let was_idle = playlist.is_empty() || playlist.state == PlayState::Stopped;
            playlist.append(tracks);
            if was_idle {
                playlist.state = PlayState::Playing;
                start_playing_current(sink, playlist, meta_cache, current_meta);
            }
        }
        Command::Toggle => match playlist.state {
            PlayState::Playing => {
                sink.pause();
                playlist.state = PlayState::Paused;
            }
            PlayState::Paused => {
                sink.play();
                playlist.state = PlayState::Playing;
            }
            PlayState::Stopped => {
                start_playing_current(sink, playlist, meta_cache, current_meta);
            }
        },
        Command::Next => {
            if playlist.next().is_some() {
                start_playing_current(sink, playlist, meta_cache, current_meta);
            }
        }
        Command::Prev => {
            if playlist.prev().is_some() {
                start_playing_current(sink, playlist, meta_cache, current_meta);
            }
        }
        Command::Stop => {
            sink.stop();
            playlist.stop();
            *current_meta = TrackMeta::default();
        }
        Command::ShuffleToggle => {
            playlist.toggle_shuffle(rng);
        }
        Command::RepeatCycle => {
            playlist.cycle_repeat();
        }
        Command::SetVolume(volume) => {
            sink.set_volume(volume.clamp(0.0, 2.0));
        }
        Command::Shutdown => {
            sink.stop();
        }
    }
}

/// Load and start the current track, skipping over any tracks that fail to
/// decode (bounded by the queue length to avoid infinite loops).
fn start_playing_current(
    sink: &rodio::Player,
    playlist: &mut Playlist,
    meta_cache: &mut HashMap<PathBuf, TrackMeta>,
    current_meta: &mut TrackMeta,
) {
    let attempts = playlist.len();
    for _ in 0..attempts {
        let Some(path) = playlist.current().cloned() else {
            break;
        };
        match load_track(sink, &path) {
            Ok(()) => {
                *current_meta = meta_for(&path, meta_cache);
                playlist.state = PlayState::Playing;
                return;
            }
            Err(_) => {
                // Skip the unplayable track.
                if playlist.advance_auto().is_none() {
                    break;
                }
            }
        }
    }

    sink.stop();
    playlist.stop();
    *current_meta = TrackMeta::default();
}

fn advance_and_play(
    sink: &rodio::Player,
    playlist: &mut Playlist,
    meta_cache: &mut HashMap<PathBuf, TrackMeta>,
    current_meta: &mut TrackMeta,
) {
    if playlist.advance_auto().is_some() {
        start_playing_current(sink, playlist, meta_cache, current_meta);
    } else {
        sink.stop();
        *current_meta = TrackMeta::default();
    }
}

fn load_track(sink: &rodio::Player, path: &Path) -> anyhow::Result<()> {
    let file = File::open(path)?;
    let decoder = rodio::Decoder::try_from(file)?;
    sink.clear();
    sink.append(decoder);
    sink.play();
    Ok(())
}

fn meta_for(path: &Path, cache: &mut HashMap<PathBuf, TrackMeta>) -> TrackMeta {
    if let Some(meta) = cache.get(path) {
        return meta.clone();
    }
    let meta = read_meta(path);
    cache.insert(path.to_path_buf(), meta.clone());
    meta
}

fn read_meta(path: &Path) -> TrackMeta {
    use lofty::file::{AudioFile, TaggedFileExt};
    use lofty::prelude::Accessor;
    use lofty::probe::Probe;

    let fallback_title = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_string();

    match Probe::open(path).and_then(|probe| probe.read()) {
        Ok(tagged) => {
            let duration_secs = tagged.properties().duration().as_secs_f64();
            let tag = tagged.primary_tag().or_else(|| tagged.first_tag());
            let title = tag
                .and_then(|tag| tag.title())
                .map(|title| title.to_string())
                .filter(|title| !title.is_empty())
                .unwrap_or(fallback_title);
            let artist = tag
                .and_then(|tag| tag.artist())
                .map(|artist| artist.to_string())
                .unwrap_or_default();
            TrackMeta {
                title,
                artist,
                duration_secs,
            }
        }
        Err(_) => TrackMeta {
            title: fallback_title,
            artist: String::new(),
            duration_secs: 0.0,
        },
    }
}

fn write_snapshot(
    state: &Arc<Mutex<PlaybackInfo>>,
    sink: &rodio::Player,
    playlist: &Playlist,
    current_meta: &TrackMeta,
) {
    let Ok(mut guard) = state.lock() else {
        return;
    };

    if playlist.state == PlayState::Stopped {
        *guard = PlaybackInfo {
            playlist_total: playlist.len() as u32,
            shuffle: playlist.shuffle,
            repeat: playlist.repeat.as_str().to_string(),
            ..PlaybackInfo::default()
        };
        return;
    }

    let path = playlist
        .current()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();

    *guard = PlaybackInfo {
        state: playlist.state.as_str().to_string(),
        title: current_meta.title.clone(),
        artist: current_meta.artist.clone(),
        path,
        position_secs: sink.get_pos().as_secs_f64(),
        duration_secs: current_meta.duration_secs,
        playlist_total: playlist.len() as u32,
        shuffle: playlist.shuffle,
        repeat: playlist.repeat.as_str().to_string(),
    };
}

fn drain_until_shutdown(cmd_rx: &Receiver<Command>) {
    while let Ok(command) = cmd_rx.recv() {
        if matches!(command, Command::Shutdown) {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths(names: &[&str]) -> Vec<PathBuf> {
        names.iter().map(PathBuf::from).collect()
    }

    fn seeded() -> StdRng {
        StdRng::seed_from_u64(42)
    }

    #[test]
    fn set_tracks_starts_at_first_and_plays() {
        let mut rng = seeded();
        let mut playlist = Playlist::new();
        playlist.set_tracks(paths(&["a", "b", "c"]), &mut rng);

        assert_eq!(playlist.current().unwrap(), &PathBuf::from("a"));
        assert_eq!(playlist.state, PlayState::Playing);
        assert_eq!(playlist.len(), 3);
    }

    #[test]
    fn manual_next_and_prev_wrap_around() {
        let mut rng = seeded();
        let mut playlist = Playlist::new();
        playlist.set_tracks(paths(&["a", "b", "c"]), &mut rng);

        assert_eq!(playlist.next().unwrap(), &PathBuf::from("b"));
        assert_eq!(playlist.next().unwrap(), &PathBuf::from("c"));
        assert_eq!(playlist.next().unwrap(), &PathBuf::from("a"));
        assert_eq!(playlist.prev().unwrap(), &PathBuf::from("c"));
    }

    #[test]
    fn auto_advance_stops_at_end_when_repeat_off() {
        let mut rng = seeded();
        let mut playlist = Playlist::new();
        playlist.set_tracks(paths(&["a", "b"]), &mut rng);

        assert_eq!(playlist.advance_auto().unwrap(), &PathBuf::from("b"));
        assert!(playlist.advance_auto().is_none());
        assert_eq!(playlist.state, PlayState::Stopped);
    }

    #[test]
    fn auto_advance_wraps_when_repeat_all() {
        let mut rng = seeded();
        let mut playlist = Playlist::new();
        playlist.set_tracks(paths(&["a", "b"]), &mut rng);
        playlist.repeat = Repeat::All;

        assert_eq!(playlist.advance_auto().unwrap(), &PathBuf::from("b"));
        assert_eq!(playlist.advance_auto().unwrap(), &PathBuf::from("a"));
        assert_eq!(playlist.state, PlayState::Playing);
    }

    #[test]
    fn auto_advance_repeats_same_track_when_repeat_one() {
        let mut rng = seeded();
        let mut playlist = Playlist::new();
        playlist.set_tracks(paths(&["a", "b"]), &mut rng);
        playlist.repeat = Repeat::One;

        assert_eq!(playlist.advance_auto().unwrap(), &PathBuf::from("a"));
        assert_eq!(playlist.advance_auto().unwrap(), &PathBuf::from("a"));
    }

    #[test]
    fn repeat_cycles_through_modes() {
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
    fn toggle_shuffle_keeps_current_track_active() {
        let mut rng = seeded();
        let mut playlist = Playlist::new();
        playlist.set_tracks(paths(&["a", "b", "c", "d", "e"]), &mut rng);
        playlist.next();
        let before = playlist.current().cloned().unwrap();

        playlist.toggle_shuffle(&mut rng);
        assert!(playlist.shuffle);
        assert_eq!(playlist.current().unwrap(), &before);

        playlist.toggle_shuffle(&mut rng);
        assert!(!playlist.shuffle);
        assert_eq!(playlist.current().unwrap(), &before);
    }

    #[test]
    fn append_extends_queue() {
        let mut rng = seeded();
        let mut playlist = Playlist::new();
        playlist.set_tracks(paths(&["a"]), &mut rng);
        playlist.append(paths(&["b", "c"]));

        assert_eq!(playlist.len(), 3);
        assert_eq!(playlist.next().unwrap(), &PathBuf::from("b"));
        assert_eq!(playlist.next().unwrap(), &PathBuf::from("c"));
    }
}

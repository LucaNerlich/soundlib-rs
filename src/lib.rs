//! `soundlib-rs` is a lightweight, self-contained terminal audio library.
//!
//! It scans a configurable folder tree (for example a game soundtrack
//! collection), presents it in an interactive [ratatui](https://docs.rs/ratatui)
//! TUI, and plays audio in-process using a pure-Rust engine: [`rodio`] for
//! output, [Symphonia](https://docs.rs/symphonia) for decoding, and
//! [`lofty`](https://docs.rs/lofty) for reading track tags. There is no
//! external player or daemon, so it behaves identically on Linux and macOS.
//!
//! # Architecture
//!
//! The crate is organised into a handful of focused modules:
//!
//! | Module | Responsibility |
//! |--------|----------------|
//! | [`config`] | Load and validate the YAML config, applying environment overrides. |
//! | [`library`] | Scan the library root into an in-memory tree with natural sorting. |
//! | [`player`] | The embedded playback engine ([`player::RodioEngine`]) and its audio-free [`player::Playlist`] core. |
//! | [`playback`] | The [`playback::PlaybackInfo`] snapshot model and "Now Playing" render helpers. |
//! | [`app`] | The [`app::App`] TUI: event loop, tree navigation, and key handling. |
//!
//! Playback runs on a dedicated background thread that owns the (non-`Send`)
//! audio device. The UI communicates with it exclusively through the
//! [`player::PlaybackEngine`] trait — sending commands and polling a shared
//! [`playback::PlaybackInfo`] snapshot — which also makes the UI testable
//! against a mock engine.
//!
//! # Example
//!
//! Driving the engine directly, without the TUI:
//!
//! ```no_run
//! use soundlib_rs::player::{PlaybackEngine, RodioEngine};
//! use std::path::PathBuf;
//!
//! let engine = RodioEngine::new();
//! engine.set_volume(0.8);
//! engine.play_replace(vec![PathBuf::from("/music/track.flac")]);
//! // ... later ...
//! if let Some(info) = engine.snapshot() {
//!     println!("{} ({:.0}s)", info.title, info.duration_secs);
//! }
//! engine.shutdown();
//! ```

pub mod app;
pub mod config;
pub mod library;
pub mod playback;
pub mod player;

#[cfg(test)]
pub(crate) mod testsupport;

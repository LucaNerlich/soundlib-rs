# soundlib-rs

[![Crates.io](https://img.shields.io/crates/v/soundlib-rs)](https://crates.io/crates/soundlib-rs)

A lightweight, self-contained terminal audio library for browsing local music folders and playing tracks directly in your terminal.

`soundlib-rs` scans a configurable folder tree (for example a game soundtrack collection), presents it in an interactive TUI, and plays audio in-process using a pure-Rust engine ([rodio](https://github.com/RustAudio/rodio) for output, [Symphonia](https://github.com/pdeljanov/Symphonia) for decoding, [lofty](https://github.com/Serial-ATA/lofty-rs) for tags). There is no external player or daemon — it runs the same on Linux and macOS.

## Features

- **Tree browser** — navigate albums and tracks with expand/collapse
- **Folder playback** — queue all audio files in a folder recursively, in natural sort order
- **Single-file playback** — queue individual tracks
- **Append mode** — add to the current queue without stopping playback
- **Transport controls** — play/pause, next, previous, stop
- **Shuffle and repeat** — shuffle the queue and cycle repeat off → all → one
- **Live filter** — type-to-filter across the library tree
- **Configurable** — YAML config file with environment variable overrides
- **Rescan** — refresh the library without restarting

## How playback works

Playback runs entirely in-process. A background audio thread owns the audio
device and a playback queue; the TUI sends it commands (play, append, transport,
shuffle, repeat) and polls a shared snapshot for the Now Playing display.

```
config.yaml + env vars
        │
        ▼
   library scan (walkdir)
        │
        ▼
   in-memory tree (ratatui TUI)
        │
        ▼
   embedded engine (rodio queue: play / transport / shuffle / repeat)
        │
        ▼
   OS audio output (CoreAudio on macOS, ALSA/PulseAudio on Linux via cpal)
```

### Supported formats

Decoding is handled by Symphonia (rodio's default), covering **mp3, flac,
mp4/aac (m4a), ogg vorbis, and wav**. Track title/artist and duration are read
from embedded tags via `lofty`, falling back to the filename when no tags are
present.

> Note: `opus` is listed in the default config for convenience, but Symphonia's
> opus support is limited; opus files may not decode. Remove it from
> `audio_extensions` if you do not use it.

## Prerequisites

| Requirement | Notes |
|-------------|-------|
| **Rust toolchain** | Edition 2024, Rust 1.87+. Install via [rustup](https://rustup.rs/). |
| **ALSA dev headers (Linux only)** | `cpal` needs them at build time: `libasound2-dev` (Debian/Ubuntu) or `alsa-lib` (Arch/Fedora). macOS uses built-in CoreAudio — no extra packages. |
| **A music folder** | A directory tree containing audio files (`.mp3`, `.flac`, `.ogg`, etc.). |

## Build

Clone the repository and build with Cargo:

```bash
git clone <repo-url>
cd soundlib-rs
cargo build --release
```

The binary is written to `target/release/soundlib-rs`.

### Development build

```bash
cargo build
./target/debug/soundlib-rs
```

### Run tests

```bash
cargo test
```

The test suite has **66 tests** across unit and integration targets:

| Area | Coverage |
|------|----------|
| `library` | Natural sort, extension filtering, recursive scan, `collect_tracks`, temp-dir fixtures |
| `config` | YAML load/save, env overrides, validation, serde roundtrip |
| `player` | Pure playlist core: next/prev wrap, repeat off/all/one, deterministic seeded shuffle, append |
| `app` | TUI navigation, expand/collapse, filter, play/append, transport keys (via a recording engine, no real audio device) |

## Installation

Copy or symlink the release binary somewhere on your `PATH`:

```bash
cargo install --path .
# binary ends up at ~/.cargo/bin/soundlib-rs
```

Or manually:

```bash
cargo build --release
install -m 755 target/release/soundlib-rs ~/.local/bin/soundlib-rs
```

## Configuration

On first run, `soundlib-rs` creates a default config file at:

```
~/.config/soundlib/config.yaml        # Linux
~/Library/Application Support/soundlib/config.yaml   # macOS
```

### Default config

The default `library_root` is your OS Music folder (`~/Music` on most systems).

```yaml
library_root: /Users/you/Music
audio_extensions:
  - mp3
  - flac
  - ogg
  - opus
  - wav
  - m4a
volume: 1.0
```

| Field | Description |
|-------|-------------|
| `library_root` | Top-level directory to scan. Must exist and be a directory. |
| `audio_extensions` | File extensions to include (case-insensitive, leading dots optional). |
| `volume` | Output volume multiplier. `1.0` is unmodified; values are clamped to `0.0`–`2.0`. |

Edit this file to point at your own music library.

### Environment variable overrides

Environment variables take precedence over the YAML file:

| Variable | Effect |
|----------|--------|
| `SOUNDLIB_ROOT` | Overrides `library_root` |
| `SOUNDLIB_CONFIG` | Use an alternate config file path |

Example — point at a different library for one session:

```bash
SOUNDLIB_ROOT=/mnt/music/osts soundlib-rs
```

Example — custom config location:

```bash
SOUNDLIB_CONFIG=~/.config/soundlib/work.yaml soundlib-rs
```

## Usage

```bash
soundlib-rs
```

The TUI opens in your terminal with three panes:

1. **Library** — expandable tree of folders and audio files under `library_root`
2. **Now Playing** — live track title, artist, progress bar, elapsed/total time, shuffle/repeat, and an activity visualizer
3. **Status** — last action, errors, and keybinding hints

The library root folder is shown at the top of the tree and expanded by default. Folders are marked `[dir]`, files `[file]`.

### Playing music

| Action | How |
|--------|-----|
| Play a single file | Select the file, press `Enter` |
| Play an entire folder | Select a folder, press `Enter` or `p` |
| Append without interrupting | Select a file or folder, press `a` |

**Replace mode** (`Enter` / `p`): replaces the current queue with the selected tracks and starts playing.

**Append mode** (`a`): adds the selected tracks to the end of the current queue. If nothing is playing, playback starts immediately.

When a folder is selected, all audio files under it are collected recursively and queued in natural sort order (so `track2` comes before `track10`).

### Keybindings

#### Navigation

| Key | Action |
|-----|--------|
| `j` / `↓` | Move selection down |
| `k` / `↑` | Move selection up |
| `l` / `→` | Expand folder |
| `h` / `←` | Collapse folder; on a file, jump to parent |

#### Playback

| Key | Action |
|-----|--------|
| `Enter` | Replace queue with selection + play |
| `p` | Same as `Enter` |
| `a` | Append selection to queue |
| `Space` | Toggle play/pause |
| `n` | Next track |
| `b` | Previous track |
| `s` | Stop playback |
| `z` | Toggle shuffle |
| `R` | Cycle repeat mode off → all → one |

#### Library

| Key | Action |
|-----|--------|
| `/` | Enter filter mode |
| `Esc` | Clear filter and exit filter mode |
| `Enter` (in filter mode) | Confirm filter and exit filter mode |
| `Backspace` | Delete last filter character |
| `r` | Rescan `library_root` from disk |
| `q` | Quit |

In filter mode, type any substring to narrow the tree. Matching is case-insensitive and includes folder names — if a child matches, ancestor folders are shown too.

## Project structure

```
soundlib-rs/
├── Cargo.toml
├── README.md
└── src/
    ├── main.rs      # entry point, terminal setup/teardown
    ├── config.rs    # YAML config, env overrides, validation
    ├── library.rs   # folder tree scan, natural sort, collect_tracks
    ├── playback.rs  # PlaybackInfo model + Now Playing render helpers
    ├── player.rs    # embedded rodio engine + pure playlist core
    └── app.rs       # ratatui TUI and event loop
```

## Troubleshooting

### `library root does not exist or is not a directory`

Set a valid path in your config file or via `SOUNDLIB_ROOT`:

```bash
SOUNDLIB_ROOT=/path/to/your/music soundlib-rs
```

### No sound / nothing plays

- Confirm your system has a working default audio output device.
- On Linux, make sure ALSA dev headers were available at build time (rebuild after installing `libasound2-dev` / `alsa-lib`).
- Check that the file format is supported (see Supported formats). `opus` in particular may not decode.

### Empty library tree

- Verify audio files exist under `library_root`.
- Check that their extensions are listed in `audio_extensions`.
- Press `r` to rescan after adding files.

### Terminal looks broken after a crash

`soundlib-rs` installs a panic hook that restores the terminal (raw mode off, alternate screen exited). If the display is still corrupted, run:

```bash
reset
```

## License

MIT — see [LICENSE](LICENSE).

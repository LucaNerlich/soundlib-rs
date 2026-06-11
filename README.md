# soundlib-rs

A lightweight terminal audio library for browsing local music folders and playing tracks through [cliamp](https://github.com/cliamp/cliamp).

`soundlib-rs` scans a configurable folder tree (for example a game soundtrack collection), presents it in an interactive TUI, and delegates playback to `cliamp` via its CLI. It does not decode or render audio itself — it is a browser and remote control for your existing `cliamp` daemon.

## Features

- **Tree browser** — navigate albums and tracks with expand/collapse
- **Folder playback** — queue all audio files in a folder recursively, in natural sort order
- **Single-file playback** — queue individual tracks
- **Append mode** — add to the current queue without stopping playback
- **Transport controls** — play/pause, next, previous, stop (forwarded to `cliamp`)
- **Live filter** — type-to-filter across the library tree
- **Configurable** — YAML config file with environment variable overrides
- **Rescan** — refresh the library without restarting

## Prerequisites

| Requirement | Notes |
|-------------|-------|
| **Rust toolchain** | Edition 2024 (Rust 1.85+). Install via [rustup](https://rustup.rs/). |
| **cliamp** | Must be on your `PATH`. Used for all playback. |
| **cliamp daemon** | Started automatically by default (`cliamp --daemon`). You can also run `cliamp` or `cliamp -d` yourself first. |
| **A music folder** | A directory tree containing audio files (`.mp3`, `.flac`, `.ogg`, etc.). |

### cliamp daemon

`soundlib-rs` talks to `cliamp` over its Unix socket (`~/.config/cliamp/cliamp.sock`). By default, if the daemon is not running when you queue or play, soundlib starts it headlessly:

```bash
cliamp --daemon --log-level error
```

To disable auto-start (and require a manually running `cliamp`):

```yaml
cliamp_auto_daemon: false
```

Or: `SOUNDLIB_CLIAMP_AUTO_DAEMON=false`

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

The test suite has **53 tests** across unit and integration targets:

| Area | Coverage |
|------|----------|
| `library` | Natural sort, extension filtering, recursive scan, `collect_tracks`, temp-dir fixtures |
| `config` | YAML load/save, env overrides, validation, serde roundtrip |
| `player` | Mock `cliamp` script verifying queue/play/transport args and error handling |
| `app` | TUI navigation, expand/collapse, filter, play/append, transport keys (no real terminal) |

Optional smoke tests skip automatically when your Soundtracks path or a running `cliamp` daemon is unavailable.

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
~/.config/soundlib/config.yaml
```

### Default config

```yaml
library_root: /home/luca/Nextcloud/_media/Soundtracks
audio_extensions:
  - mp3
  - flac
  - ogg
  - opus
  - wav
  - m4a
cliamp_bin: cliamp
cliamp_auto_daemon: true
```

| Field | Description |
|-------|-------------|
| `library_root` | Top-level directory to scan. Must exist and be a directory. |
| `audio_extensions` | File extensions to include (case-insensitive, leading dots optional). |
| `cliamp_bin` | Name or absolute path of the `cliamp` executable. |
| `cliamp_auto_daemon` | Start `cliamp --daemon` automatically when the socket is missing. |

Edit this file to point at your own music library.

### Environment variable overrides

Environment variables take precedence over the YAML file:

| Variable | Effect |
|----------|--------|
| `SOUNDLIB_ROOT` | Overrides `library_root` |
| `SOUNDLIB_CLIAMP_BIN` | Overrides `cliamp_bin` |
| `SOUNDLIB_CLIAMP_AUTO_DAEMON` | `true`/`false` — overrides `cliamp_auto_daemon` |
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
2. **Now Playing** — live track title, artist, progress bar, elapsed/total time, shuffle/repeat, and an activity visualizer (polled from `cliamp status --json`)
3. **Status** — last action, errors from `cliamp`, and keybinding hints

The library root folder is shown at the top of the tree and expanded by default. Folders are marked `[dir]`, files `[file]`.

### Playing music

| Action | How |
|--------|-----|
| Play a single file | Select the file, press `Enter` |
| Play an entire folder | Select a folder, press `Enter` or `p` |
| Append without interrupting | Select a file or folder, press `a` |

**Replace mode** (`Enter` / `p`): stops current playback, queues all selected tracks, then calls `cliamp play`.

**Append mode** (`a`): queues tracks without calling `cliamp stop` first.

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
| `Enter` | Stop + queue selection + play |
| `p` | Same as `Enter` |
| `a` | Append selection to queue (no stop) |
| `Space` | Toggle play/pause (`cliamp toggle`) |
| `n` | Next track |
| `b` | Previous track |
| `s` | Stop playback |

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

## How it works

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
   cliamp CLI (queue / play / transport)
        │
        ▼
   cliamp daemon (actual audio output)
```

`soundlib-rs` builds an in-memory tree at startup by walking `library_root`. Only files whose extension is listed in `audio_extensions` are included. Each directory's children are sorted with natural ordering.

Playback is delegated to `cliamp`:

- **Enter / p (replace)** — restarts the cliamp daemon with your selection: `cliamp --daemon --auto-play <file-or-folder>`. This is required because `cliamp queue` only appends to the current playlist (e.g. the default radio stream) and does not replace it.
- **a (append)** — `cliamp queue <path>` for each track when a daemon is already running
- **Space / n / b / s** — `cliamp toggle`, `next`, `prev`, `stop`

If a `cliamp` command fails, the error message is shown in the status bar; the TUI keeps running.

## Project structure

```
soundlib-rs/
├── Cargo.toml
├── README.md
└── src/
    ├── main.rs      # entry point, terminal setup/teardown
    ├── config.rs    # YAML config, env overrides, validation
    ├── library.rs   # folder tree scan, natural sort, collect_tracks
    ├── player.rs    # cliamp process wrapper
    └── app.rs       # ratatui TUI and event loop
```

## Troubleshooting

### `library root does not exist or is not a directory`

Set a valid path in `~/.config/soundlib/config.yaml` or via `SOUNDLIB_ROOT`:

```bash
SOUNDLIB_ROOT=/path/to/your/music soundlib-rs
```

### `cliamp is not running` / `daemon did not start`

With `cliamp_auto_daemon: true` (default), soundlib should start the daemon for you. If it still fails:

- Run `cliamp --daemon` manually in another terminal
- Confirm `cliamp` is on your `PATH` (`which cliamp`)
- Check `~/.config/cliamp/cliamp.log` for errors

### `cliamp queue failed: ...`

- Confirm the file path exists and is readable.
- Confirm `cliamp_bin` in config points to the correct executable.
- Check that the file extension is supported by `cliamp`.

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

See repository license file (if present).

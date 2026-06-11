# Changelog

All notable changes to this project will be documented in this file.

## [0.2.0] - 2026-06-11

### Added

- Now Playing panel with track title, artist, progress bar, elapsed/total time, and activity visualizer
- Live playback status polling via `cliamp status --json`
- Scrollable library list with persistent scroll state, scrollbar, and PgUp/PgDn / Home / End navigation
- Integration and unit test suite (library, config, player, app)
- `src/lib.rs` crate layout for testable modules
- Gothic 1 Remake and full-library scan regression tests

### Fixed

- cliamp playback now restarts the daemon with `--auto-play` and file/folder paths instead of `queue`, which only appended to the default radio stream
- Expanding a folder jumps to the first track so contents are immediately visible
- Folder contents appeared missing when the list could not scroll past the first screen

### Changed

- Append mode still uses `cliamp queue` when a daemon is already playing local files
- Status bar shows total item count and current selection index in the library title

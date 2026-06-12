# Changelog

All notable changes to this project will be documented in this file.

## [2.0.1] - 2026-06-12

### Added

- Extensive rustdoc across the public API so docs.rs renders a populated reference
- `cargo install soundlib-rs` instructions and a Crates.io badge in the README

### Changed

- Documentation and UI text now use consistent dash styling
- Crate description and keywords updated to reflect the embedded audio engine (no cliamp)

## [2.0.0] - 2026-06-12

### Added

- Embedded cross-OS audio engine replaces the external cliamp daemon - no cliamp installation required
- GitHub Actions CI workflow (`rust.yml`) for automated builds and tests
- ALSA dev headers installed automatically in the Linux CI build

### Changed

- Audio playback is now handled entirely within soundlib-rs via the embedded engine; cliamp remote control is no longer used

## [1.0.0] - 2026-06-11

### Added

- Keys sidebar listing navigation, playback, and app hotkeys (switches to filter hints in filter mode)
- `z` hotkey to toggle shuffle and `R` to cycle repeat mode via cliamp
- Automatic cliamp daemon shutdown on quit when soundlib started the daemon; externally started daemons are left running

### Changed

- Status bar no longer repeats the full hotkey list; see the Keys sidebar instead

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

//! Binary entry point for `soundlib-rs`.
//!
//! Loads the [configuration](soundlib_rs::config::Config), builds the
//! [`App`](soundlib_rs::app::App), installs a terminal guard that restores the
//! terminal on exit or panic, and runs the TUI event loop. All library
//! functionality lives in the [`soundlib_rs`] crate.

use anyhow::Result;
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::ExecutableCommand;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use soundlib_rs::app::App;
use soundlib_rs::config::Config;
use std::io::stdout;
use std::panic;
use std::sync::OnceLock;

static TERMINAL_GUARD: OnceLock<TerminalGuard> = OnceLock::new();

struct TerminalGuard;

impl TerminalGuard {
    fn install() -> Result<()> {
        enable_raw_mode()?;
        stdout().execute(EnterAlternateScreen)?;

        let previous_hook = panic::take_hook();
        panic::set_hook(Box::new(move |info| {
            restore_terminal();
            previous_hook(info);
        }));

        TERMINAL_GUARD
            .set(TerminalGuard)
            .map_err(|_| anyhow::anyhow!("terminal guard already installed"))?;
        Ok(())
    }
}

fn restore_terminal() {
    let _ = disable_raw_mode();
    let _ = stdout().execute(LeaveAlternateScreen);
}

fn main() -> Result<()> {
    let config = Config::load()?;
    let mut app = App::new(&config)?;

    TerminalGuard::install()?;
    let result = run_tui(&mut app);
    restore_terminal();
    result
}

fn run_tui(app: &mut App) -> Result<()> {
    let backend = CrosstermBackend::new(stdout());
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;
    app.run(&mut terminal)?;
    Ok(())
}

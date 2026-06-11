use std::collections::HashSet;
use std::io;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState};
use ratatui::{Frame, Terminal};

use crate::config::Config;
use crate::library::{collect_tracks, scan_library, LibraryNode};
use crate::playback::{self, format_duration, progress_bar, activity_wave, PlaybackInfo};
use crate::player::Player;

const STATUS_POLL_INTERVAL: Duration = Duration::from_millis(400);
const NOW_PLAYING_HEIGHT: u16 = 8;

type NodePath = Vec<usize>;

pub struct App {
    root: LibraryNode,
    extensions: HashSet<String>,
    library_root: PathBuf,
    expanded: HashSet<PathBuf>,
    visible_rows: Vec<NodePath>,
    list_state: ListState,
    selected: usize,
    filter: String,
    filter_active: bool,
    status: String,
    player: Player,
    playback: Option<PlaybackInfo>,
    last_status_poll: Instant,
    ui_tick: u64,
    should_quit: bool,
}

impl App {
    pub fn new(config: &Config) -> Result<Self> {
        let extensions = config.extension_set();
        let root = scan_library(&config.library_root, &extensions)?;
        let mut expanded = HashSet::new();
        expanded.insert(root.path.clone());

        let mut app = Self {
            root,
            extensions,
            library_root: config.library_root.clone(),
            expanded,
            visible_rows: Vec::new(),
            list_state: ListState::default(),
            selected: 0,
            filter: String::new(),
            filter_active: false,
            status: "Ready".into(),
            player: Player::with_options(config.cliamp_bin.clone(), config.cliamp_auto_daemon),
            playback: None,
            last_status_poll: Instant::now() - STATUS_POLL_INTERVAL,
            ui_tick: 0,
            should_quit: false,
        };
        app.rebuild_visible_rows();
        Ok(app)
    }

    pub fn run(&mut self, terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
        loop {
            self.refresh_playback_if_due();
            terminal.draw(|frame| self.draw(frame))?;

            if event::poll(Duration::from_millis(100))? {
                if let Event::Key(key) = event::read()? {
                    self.handle_key(key)?;
                }
            }

            if self.should_quit {
                break;
            }
        }
        Ok(())
    }

    fn refresh_playback(&mut self) {
        self.playback = playback::poll_player(&self.player);
        self.last_status_poll = Instant::now();
        self.ui_tick = self.ui_tick.wrapping_add(1);
    }

    fn refresh_playback_if_due(&mut self) {
        if self.last_status_poll.elapsed() >= STATUS_POLL_INTERVAL {
            self.refresh_playback();
        }
    }

    fn draw(&mut self, frame: &mut Frame) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(1),
                Constraint::Length(NOW_PLAYING_HEIGHT),
                Constraint::Length(3),
            ])
            .split(frame.area());

        self.draw_tree(frame, chunks[0]);
        self.draw_now_playing(frame, chunks[1]);
        self.draw_status(frame, chunks[2]);
    }

    fn draw_tree(&mut self, frame: &mut Frame, area: Rect) {
        self.sync_list_state();

        let total = self.visible_rows.len();
        let title = if self.filter_active {
            format!("Library  filter: {}  ({total} items)", self.filter)
        } else {
            format!(
                "Library  {}  ({total} items, {} selected)",
                self.library_root.display(),
                self.selected + 1
            )
        };

        let visible_rows = self.visible_rows.clone();
        let expanded = self.expanded.clone();
        let items: Vec<ListItem> = visible_rows
            .iter()
            .map(|path| {
                let node = self.node_at(path);
                let depth = path.len();
                let indent = "  ".repeat(depth);
                let marker = if node.is_folder() {
                    if expanded.contains(&node.path) {
                        "▼ "
                    } else {
                        "▶ "
                    }
                } else {
                    "  "
                };
                let kind = if node.is_folder() { "dir" } else { "file" };
                let line = Line::from(vec![
                    Span::raw(format!("{indent}{marker}")),
                    Span::raw(node.name.clone()),
                    Span::raw(format!("  [{kind}]")),
                ]);
                ListItem::new(line)
            })
            .collect();

        let list = List::new(items)
            .block(Block::default().borders(Borders::ALL).title(title))
            .highlight_style(Style::default().add_modifier(Modifier::REVERSED));

        frame.render_stateful_widget(list, area, &mut self.list_state);

        if self.visible_rows.len() > area.height.saturating_sub(2) as usize {
            let mut scrollbar_state =
                ScrollbarState::new(self.visible_rows.len()).position(self.list_state.offset());
            frame.render_stateful_widget(
                Scrollbar::new(ScrollbarOrientation::VerticalRight),
                area,
                &mut scrollbar_state,
            );
        }
    }

    fn draw_now_playing(&self, frame: &mut Frame, area: Rect) {
        let inner_width = area.width.saturating_sub(2) as usize;
        let bar_width = inner_width.saturating_sub(2);

        let lines = if let Some(info) = &self.playback {
            let headline = if info.artist.is_empty() {
                format!("{} {}", info.state_icon(), info.title)
            } else {
                format!("{} {} — {}", info.state_icon(), info.artist, info.title)
            };

            let file_name = std::path::Path::new(&info.path)
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or(&info.path);

            let position = format_duration(info.position_secs);
            let duration = format_duration(info.duration_secs);
            let bar = progress_bar(info.progress_ratio(), bar_width);
            let wave = activity_wave(self.ui_tick, bar_width, info.state == "playing");

            let mut modes = vec![info.state.clone()];
            if info.shuffle {
                modes.push("shuffle".into());
            }
            if info.repeat != "off" && !info.repeat.is_empty() {
                modes.push(format!("repeat {}", info.repeat.to_ascii_lowercase()));
            }
            if info.playlist_total > 0 {
                modes.push(format!("{} tracks", info.playlist_total));
            }

            vec![
                Line::from(Span::styled(headline, Style::default().add_modifier(Modifier::BOLD))),
                Line::from(Span::styled(
                    truncate_middle(file_name, inner_width),
                    Style::default().fg(Color::DarkGray),
                )),
                Line::from(bar),
                Line::from(vec![
                    Span::raw(format!("{position} / {duration}  ")),
                    Span::styled(modes.join(" · "), Style::default().fg(Color::Cyan)),
                ]),
                Line::from(Span::styled(wave, Style::default().fg(Color::Magenta))),
            ]
        } else {
            vec![
                Line::from(Span::styled(
                    "Nothing playing",
                    Style::default().fg(Color::DarkGray),
                )),
                Line::from("Select a track or folder and press Enter"),
                Line::from(progress_bar(0.0, bar_width)),
                Line::from(""),
                Line::from(activity_wave(0, bar_width, false)),
            ]
        };

        let paragraph = Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .title("Now Playing"),
        );

        frame.render_widget(paragraph, area);
    }

    fn draw_status(&self, frame: &mut Frame, area: Rect) {
        let help = if self.filter_active {
            "Type to filter | Esc: clear | Enter: confirm"
        } else {
            "↑↓/jk: move | PgUp/PgDn: page | l/→: expand | h/←: collapse | Enter: play | a: append | Space: toggle | n/b: next/prev | s: stop | /: filter | r: rescan | q: quit"
        };

        let paragraph = Paragraph::new(vec![
            Line::from(self.status.as_str()),
            Line::from(help),
        ])
        .block(Block::default().borders(Borders::ALL).title("Status"));

        frame.render_widget(paragraph, area);
    }

    fn handle_key(&mut self, key: KeyEvent) -> Result<()> {
        if self.filter_active {
            return self.handle_filter_key(key);
        }

        match key.code {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Char('j') | KeyCode::Down => self.move_selection(1),
            KeyCode::Char('k') | KeyCode::Up => self.move_selection(-1),
            KeyCode::PageDown => self.move_selection_page(1),
            KeyCode::PageUp => self.move_selection_page(-1),
            KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.move_selection_page(1)
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.move_selection_page(-1)
            }
            KeyCode::Home => self.select_index(0),
            KeyCode::End => self.select_last(),
            KeyCode::Char('l') | KeyCode::Right => self.expand_selected(),
            KeyCode::Char('h') | KeyCode::Left => self.collapse_selected(),
            KeyCode::Enter => self.play_selected_replace()?,
            KeyCode::Char('p') => self.play_selected_replace()?,
            KeyCode::Char('a') => self.play_selected_append()?,
            KeyCode::Char(' ') => self.run_player_action("toggle", |p| p.toggle()),
            KeyCode::Char('n') => self.run_player_action("next", |p| p.next()),
            KeyCode::Char('b') => self.run_player_action("prev", |p| p.prev()),
            KeyCode::Char('s') => self.run_player_action("stop", |p| p.stop()),
            KeyCode::Char('/') => {
                self.filter_active = true;
                self.filter.clear();
                self.status = "Filter mode".into();
            }
            KeyCode::Char('r') => self.rescan()?,
            _ => {}
        }

        Ok(())
    }

    fn handle_filter_key(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Esc => {
                self.filter.clear();
                self.filter_active = false;
                self.rebuild_visible_rows();
                self.status = "Filter cleared".into();
            }
            KeyCode::Enter => {
                self.filter_active = false;
                self.rebuild_visible_rows();
                self.status = format!("Filter: {}", self.filter);
            }
            KeyCode::Backspace => {
                self.filter.pop();
                self.rebuild_visible_rows();
            }
            KeyCode::Char(ch) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.filter.push(ch);
                self.rebuild_visible_rows();
            }
            _ => {}
        }
        Ok(())
    }

    fn move_selection(&mut self, delta: i32) {
        if self.visible_rows.is_empty() {
            return;
        }
        let len = self.visible_rows.len() as i32;
        let next = (self.selected as i32 + delta).rem_euclid(len);
        self.selected = next as usize;
        self.sync_list_state();
    }

    fn move_selection_page(&mut self, direction: i32) {
        if self.visible_rows.is_empty() {
            return;
        }
        let page = 10;
        let len = self.visible_rows.len() as i32;
        let next = if direction > 0 {
            (self.selected as i32 + page).min(len - 1)
        } else {
            (self.selected as i32 - page).max(0)
        };
        self.selected = next as usize;
        self.sync_list_state();
    }

    fn select_last(&mut self) {
        if !self.visible_rows.is_empty() {
            self.selected = self.visible_rows.len() - 1;
            self.sync_list_state();
        }
    }

    fn sync_list_state(&mut self) {
        if self.visible_rows.is_empty() {
            self.selected = 0;
            self.list_state.select(None);
            *self.list_state.offset_mut() = 0;
            return;
        }
        if self.selected >= self.visible_rows.len() {
            self.selected = self.visible_rows.len() - 1;
        }
        self.list_state.select(Some(self.selected));
    }

    fn expand_selected(&mut self) {
        let Some(path) = self.selected_path().cloned() else {
            return;
        };
        let (is_folder, folder_path, name) = {
            let node = self.node_at(&path);
            (node.is_folder(), node.path.clone(), node.name.clone())
        };
        if is_folder {
            self.expanded.insert(folder_path.clone());
            self.rebuild_visible_rows();
            self.reveal_first_child_after(&path);
            self.status = format!("Expanded {name}");
        }
    }

    fn reveal_first_child_after(&mut self, folder_path: &NodePath) {
        let Some(folder_idx) = self
            .visible_rows
            .iter()
            .position(|row| row == folder_path)
        else {
            return;
        };

        let child_depth = folder_path.len() + 1;
        if let Some((child_idx, _)) = self.visible_rows[folder_idx + 1..]
            .iter()
            .enumerate()
            .find(|(_, row)| row.len() == child_depth)
        {
            self.selected = folder_idx + 1 + child_idx;
            self.sync_list_state();
        }
    }

    fn collapse_selected(&mut self) {
        let Some(path) = self.selected_path().cloned() else {
            return;
        };
        let (is_folder, folder_path, name) = {
            let node = self.node_at(&path);
            (node.is_folder(), node.path.clone(), node.name.clone())
        };
        if is_folder && path.is_empty() {
            return;
        }
        if is_folder {
            self.expanded.remove(&folder_path);
            self.rebuild_visible_rows();
            self.status = format!("Collapsed {name}");
        } else if !path.is_empty() {
            let parent_path = parent_path(&path);
            self.select_path(&parent_path);
            self.status = format!("Selected parent of {name}");
        }
    }

    fn play_selected_replace(&mut self) -> Result<()> {
        let Some(path) = self.selected_path().cloned() else {
            return Ok(());
        };
        let node = self.node_at(&path);

        if !node.is_file() && collect_tracks(node).is_empty() {
            self.status = format!("No audio files in {}", node.name);
            return Ok(());
        }

        match self.player.play_replace(&[node.path.clone()]) {
            Ok(()) => {
                let count = if node.is_file() {
                    1
                } else {
                    collect_tracks(node).len()
                };
                self.status = format!("Playing {} track(s) from {}", count, node.name);
                self.refresh_playback();
            }
            Err(err) => self.status = format!("Play failed: {err:#}"),
        }
        Ok(())
    }

    fn play_selected_append(&mut self) -> Result<()> {
        let Some(path) = self.selected_path().cloned() else {
            return Ok(());
        };
        let node = self.node_at(&path);
        let tracks = if node.is_file() {
            vec![node.path.clone()]
        } else {
            collect_tracks(node)
        };

        if tracks.is_empty() {
            self.status = format!("No audio files in {}", node.name);
            return Ok(());
        }

        if !self.player.is_daemon_running() {
            match self.player.play_replace(&[node.path.clone()]) {
                Ok(()) => {
                    self.status = format!("Playing {} track(s) from {}", tracks.len(), node.name);
                }
                Err(err) => self.status = format!("Play failed: {err:#}"),
            }
            return Ok(());
        }

        match self.player.queue_tracks(&tracks) {
            Ok(count) => {
                self.status = format!("Queued {} track(s) from {}", count, node.name);
            }
            Err(err) => self.status = format!("Queue failed: {err:#}"),
        }
        Ok(())
    }

    fn run_player_action(
        &mut self,
        label: &str,
        action: impl FnOnce(&Player) -> Result<()>,
    ) {
        match action(&self.player) {
            Ok(()) => {
                self.status = format!("cliamp {label}");
                self.refresh_playback();
            }
            Err(err) => self.status = format!("cliamp {label} failed: {err:#}"),
        }
    }

    fn rescan(&mut self) -> Result<()> {
        let started = Instant::now();
        let selected_path = self.selected_path().cloned();
        self.root = scan_library(&self.library_root, &self.extensions)?;
        self.expanded.retain(|p| p.exists());
        if !self.expanded.contains(&self.root.path) {
            self.expanded.insert(self.root.path.clone());
        }
        self.rebuild_visible_rows();
        if let Some(path) = selected_path {
            self.select_path(&path);
        }
        self.status = format!(
            "Rescanned in {:.2}s",
            started.elapsed().as_secs_f32()
        );
        Ok(())
    }

    fn rebuild_visible_rows(&mut self) {
        let filter = self.filter.trim().to_ascii_lowercase();
        let filtering = !filter.is_empty();

        self.visible_rows.clear();
        let root_path = NodePath::new();
        self.append_visible(&root_path, filtering, &filter);

        self.sync_list_state();
    }

    fn append_visible(&mut self, path: &NodePath, filtering: bool, filter: &str) {
        let node_snapshot = {
            let node = self.node_at(path);
            let subtree_matches = filtering && self.subtree_matches(node, filter);
            NodeSnapshot {
                is_folder: node.is_folder(),
                is_expanded: self.expanded.contains(&node.path),
                show_self: !filtering
                    || node.name.to_ascii_lowercase().contains(filter)
                    || subtree_matches,
                children: node
                    .children
                    .iter()
                    .map(|child| ChildSnapshot {
                        matches_filter: !filtering
                            || child.name.to_ascii_lowercase().contains(filter)
                            || self.subtree_matches(child, filter),
                    })
                    .collect(),
            }
        };

        if node_snapshot.show_self {
            self.visible_rows.push(path.to_vec());
        }

        if node_snapshot.is_folder && node_snapshot.is_expanded {
            for (idx, child) in node_snapshot.children.iter().enumerate() {
                if child.matches_filter {
                    let mut child_path = path.to_vec();
                    child_path.push(idx);
                    self.append_visible(&child_path, filtering, filter);
                }
            }
        }
    }

    fn subtree_matches(&self, node: &LibraryNode, filter: &str) -> bool {
        if node.name.to_ascii_lowercase().contains(filter) {
            return true;
        }
        node.children
            .iter()
            .any(|child| self.subtree_matches(child, filter))
    }

    fn node_at(&self, path: &NodePath) -> &LibraryNode {
        let mut node = &self.root;
        for &idx in path {
            node = &node.children[idx];
        }
        node
    }

    fn selected_path(&self) -> Option<&NodePath> {
        self.visible_rows.get(self.selected)
    }

    fn select_path(&mut self, path: &NodePath) {
        if let Some(idx) = self.visible_rows.iter().position(|row| row == path) {
            self.selected = idx;
            self.sync_list_state();
        }
    }

    fn select_index(&mut self, index: usize) {
        if index < self.visible_rows.len() {
            self.selected = index;
            self.sync_list_state();
        }
    }
}

#[cfg(test)]
impl App {
    pub(crate) fn from_tree(root: LibraryNode, player: Player) -> Self {
        let library_root = root.path.clone();
        let mut expanded = HashSet::new();
        expanded.insert(root.path.clone());

        let mut app = Self {
            root,
            extensions: HashSet::from(["mp3".into(), "flac".into()]),
            library_root,
            expanded,
            visible_rows: Vec::new(),
            list_state: ListState::default(),
            selected: 0,
            filter: String::new(),
            filter_active: false,
            status: "Ready".into(),
            player,
            playback: None,
            last_status_poll: Instant::now() - STATUS_POLL_INTERVAL,
            ui_tick: 0,
            should_quit: false,
        };
        app.rebuild_visible_rows();
        app
    }

    pub(crate) fn visible_row_paths(&self) -> Vec<NodePath> {
        self.visible_rows.clone()
    }

    pub(crate) fn visible_row_names(&self) -> Vec<String> {
        self.visible_rows
            .iter()
            .map(|path| self.node_at(path).name.clone())
            .collect()
    }

    pub(crate) fn status_message(&self) -> &str {
        &self.status
    }

    pub(crate) fn selected_index(&self) -> usize {
        self.selected
    }

    pub(crate) fn should_quit(&self) -> bool {
        self.should_quit
    }

    pub(crate) fn is_filter_active(&self) -> bool {
        self.filter_active
    }

    pub(crate) fn filter_text(&self) -> &str {
        &self.filter
    }

    pub(crate) fn simulate_key(&mut self, key: KeyEvent) -> Result<()> {
        self.handle_key(key)
    }

    pub(crate) fn set_expanded(&mut self, path: PathBuf, expanded: bool) {
        if expanded {
            self.expanded.insert(path);
        } else {
            self.expanded.remove(&path);
        }
        self.rebuild_visible_rows();
    }

    pub(crate) fn subtree_matches_for_test(&self, node: &LibraryNode, filter: &str) -> bool {
        self.subtree_matches(node, filter)
    }
}

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    use std::fs;

    use super::*;
    use crate::testsupport::{file_node, node, MockCliamp, TestLibrary};

    fn test_player(mock: &MockCliamp) -> Player {
        Player::with_socket_override(
            mock.bin.to_string_lossy().to_string(),
            false,
            mock.socket_path(),
        )
    }

    fn app_from_test_library() -> (App, MockCliamp) {
        let lib = TestLibrary::minimal();
        let mock = MockCliamp::success();
        let tree = lib.scan();
        let app = App::from_tree(tree, test_player(&mock));
        (app, mock)
    }

    #[test]
    fn starts_with_root_expanded_and_visible_children() {
        let (app, _) = app_from_test_library();
        let names = app.visible_row_names();

        assert!(names.first().is_some_and(|n| n == "library"));
        assert!(names.contains(&"alpha".to_string()));
        assert!(names.contains(&"beta".to_string()));
        assert!(names.contains(&"loose.ogg".to_string()));
    }

    #[test]
    fn move_selection_wraps_around() {
        let (mut app, _) = app_from_test_library();
        let count = app.visible_row_paths().len();
        app.select_index(count - 1);

        app.simulate_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::empty()))
            .expect("move down");
        assert_eq!(app.selected_index(), 0);

        app.simulate_key(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::empty()))
            .expect("move up");
        assert_eq!(app.selected_index(), count - 1);
    }

    #[test]
    fn expand_and_collapse_folder_updates_visible_rows() {
        let (mut app, _) = app_from_test_library();
        let alpha_idx = app
            .visible_row_names()
            .iter()
            .position(|n| n == "alpha")
            .expect("alpha visible");

        app.select_index(alpha_idx);
        app.simulate_key(KeyEvent::new(KeyCode::Char('l'), KeyModifiers::empty()))
            .expect("expand");

        assert!(app.visible_row_names().contains(&"01-intro.mp3".to_string()));

        app.simulate_key(KeyEvent::new(KeyCode::Char('h'), KeyModifiers::empty()))
            .expect("parent");
        app.simulate_key(KeyEvent::new(KeyCode::Char('h'), KeyModifiers::empty()))
            .expect("collapse");

        assert!(!app.visible_row_names().contains(&"01-intro.mp3".to_string()));
        assert!(app.status_message().contains("Collapsed"));
    }

    #[test]
    fn expand_selects_first_visible_child() {
        let (mut app, _) = app_from_test_library();
        let alpha_idx = app
            .visible_row_names()
            .iter()
            .position(|n| n == "alpha")
            .expect("alpha visible");

        app.select_index(alpha_idx);
        app.simulate_key(KeyEvent::new(KeyCode::Char('l'), KeyModifiers::empty()))
            .expect("expand");

        assert_eq!(app.visible_row_names()[app.selected_index()], "01-intro.mp3");
    }

    #[test]
    fn filter_narrows_visible_rows() {
        let (mut app, _) = app_from_test_library();

        app.simulate_key(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::empty()))
            .expect("filter mode");
        assert!(app.is_filter_active());

        for ch in "beta".chars() {
            app.simulate_key(KeyEvent::new(KeyCode::Char(ch), KeyModifiers::empty()))
                .expect("type filter");
        }

        let names = app.visible_row_names();
        assert!(names.iter().any(|n| n == "beta"));
        assert!(!names.iter().any(|n| n == "alpha"));
    }

    #[test]
    fn filter_escape_clears_filter() {
        let (mut app, _) = app_from_test_library();

        app.simulate_key(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::empty()))
            .expect("filter");
        app.simulate_key(KeyEvent::new(KeyCode::Char('b'), KeyModifiers::empty()))
            .expect("type");
        app.simulate_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::empty()))
            .expect("clear");

        assert!(!app.is_filter_active());
        assert!(app.filter_text().is_empty());
        assert!(app.visible_row_names().contains(&"alpha".to_string()));
    }

    #[test]
    fn play_selected_replace_starts_daemon_with_path() {
        let (mut app, mock) = app_from_test_library();
        let loose_idx = app
            .visible_row_names()
            .iter()
            .position(|n| n == "loose.ogg")
            .expect("loose");

        app.select_index(loose_idx);
        app.simulate_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::empty()))
            .expect("play");

        let lines = mock.log_lines();
        assert!(lines.iter().any(|l| l.contains("--daemon") && l.contains("--auto-play")));
        assert!(lines.iter().any(|l| l.contains("loose.ogg")));
        assert!(mock.socket_path().exists());
        assert!(app.status_message().contains("Playing 1 track"));
    }

    #[test]
    fn play_selected_append_queues_when_daemon_running() {
        let (mut app, mock) = app_from_test_library();
        fs::write(mock.socket_path(), b"").expect("pretend daemon is running");

        let loose_idx = app
            .visible_row_names()
            .iter()
            .position(|n| n == "loose.ogg")
            .expect("loose");

        app.select_index(loose_idx);
        app.simulate_key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::empty()))
            .expect("append");

        let lines = mock.log_lines();
        assert!(!lines.iter().any(|l| l.contains("--daemon")));
        assert!(lines.iter().any(|l| l.contains("queue") && l.contains("loose.ogg")));
        assert!(app.status_message().contains("Queued 1 track"));
    }

    #[test]
    fn play_folder_starts_daemon_with_folder_path() {
        let (mut app, mock) = app_from_test_library();
        let alpha_idx = app
            .visible_row_names()
            .iter()
            .position(|n| n == "alpha")
            .expect("alpha");

        app.select_index(alpha_idx);
        app.simulate_key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::empty()))
            .expect("play folder");

        let lines = mock.log_lines();
        assert!(lines.iter().any(|l| l.contains("--daemon") && l.contains("alpha")));
        assert!(app.status_message().contains("Playing 2 track"));
    }

    #[test]
    fn play_empty_folder_sets_status_without_panicking() {
        let lib = TestLibrary::empty_album();
        let mock = MockCliamp::success();
        let tree = lib.scan();
        let mut app = App::from_tree(tree, test_player(&mock));

        let idx = app
            .visible_row_names()
            .iter()
            .position(|n| n == "no-tracks")
            .expect("empty album");

        app.select_index(idx);
        app.simulate_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::empty()))
            .expect("play empty");

        assert!(app.status_message().contains("No audio files"));
        assert!(mock.log_lines().is_empty());
    }

    #[test]
    fn transport_keys_delegate_to_cliamp() {
        let (mut app, mock) = app_from_test_library();
        fs::write(mock.socket_path(), b"").expect("pretend daemon is running");

        app.simulate_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::empty()))
            .expect("toggle");
        app.simulate_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::empty()))
            .expect("next");
        app.simulate_key(KeyEvent::new(KeyCode::Char('b'), KeyModifiers::empty()))
            .expect("prev");
        app.simulate_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::empty()))
            .expect("stop");

        let transport_cmds: Vec<_> = mock
            .log_lines()
            .into_iter()
            .filter(|line| !line.starts_with("status"))
            .collect();
        assert_eq!(transport_cmds, vec!["toggle", "next", "prev", "stop"]);
    }

    #[test]
    fn quit_key_sets_should_quit() {
        let (mut app, _) = app_from_test_library();
        app.simulate_key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::empty()))
            .expect("quit");
        assert!(app.should_quit());
    }

    #[test]
    fn subtree_matches_finds_nested_names() {
        let lib = TestLibrary::minimal();
        let tree = lib.scan();
        let mock = MockCliamp::success();
        let app = App::from_tree(tree.clone(), test_player(&mock));

        let gamma_node = tree.children.iter().find(|n| n.name == "gamma").expect("gamma");
        assert!(app.subtree_matches_for_test(gamma_node, "deep"));
        assert!(!app.subtree_matches_for_test(gamma_node, "nonexistent-xyz"));
    }

    #[test]
    fn select_parent_on_file() {
        let root = PathBuf::from("/music");
        let alpha = root.join("alpha");
        let tree = node(
            root.clone(),
            "music",
            vec![
                node(alpha.clone(), "alpha", vec![file_node(&alpha, "01.mp3")]),
                file_node(&root, "loose.mp3"),
            ],
        );

        let mock = MockCliamp::success();
        let mut app = App::from_tree(tree, test_player(&mock));
        app.set_expanded(alpha.clone(), true);

        let track_idx = app
            .visible_row_names()
            .iter()
            .position(|n| n == "01.mp3")
            .expect("track");
        app.select_index(track_idx);

        app.simulate_key(KeyEvent::new(KeyCode::Char('h'), KeyModifiers::empty()))
            .expect("parent");

        assert_eq!(app.visible_row_names()[app.selected_index()], "alpha");
    }

    #[test]
    fn app_new_loads_configured_library() {
        let lib = TestLibrary::minimal();
        let mock = MockCliamp::success();
        let config_path = lib._dir.path().join("config.yaml");
        crate::testsupport::write_config(&config_path, &lib.root, mock.bin.to_str().unwrap());

        temp_env::with_vars(
            [
                ("SOUNDLIB_CONFIG", Some(config_path.to_str().unwrap())),
                ("SOUNDLIB_ROOT", None::<&str>),
            ],
            || {
                let config = Config::load().expect("load config");
                let app = App::new(&config).expect("create app");
                assert!(app.visible_row_names().contains(&"alpha".to_string()));
            },
        );
    }
}

fn truncate_middle(text: &str, max_len: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    if max_len == 0 {
        return String::new();
    }
    if chars.len() <= max_len {
        return text.to_string();
    }
    if max_len <= 3 {
        return chars.into_iter().take(max_len).collect();
    }

    let keep = max_len - 3;
    let front = keep / 2;
    let back = keep - front;
    let head: String = chars.iter().take(front).collect();
    let tail: String = chars[chars.len() - back..].iter().collect();
    format!("{head}...{tail}")
}

fn parent_path(path: &NodePath) -> NodePath {
    let mut parent = path.to_vec();
    parent.pop();
    parent
}

struct NodeSnapshot {
    is_folder: bool,
    is_expanded: bool,
    show_self: bool,
    children: Vec<ChildSnapshot>,
}

struct ChildSnapshot {
    matches_filter: bool,
}

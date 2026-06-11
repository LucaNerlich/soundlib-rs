use std::path::Path;

use serde::Deserialize;

use crate::player::Player;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct PlaybackInfo {
    pub state: String,
    pub title: String,
    pub artist: String,
    pub path: String,
    pub position_secs: f64,
    pub duration_secs: f64,
    pub playlist_total: u32,
    pub shuffle: bool,
    pub repeat: String,
}

impl PlaybackInfo {
    pub fn is_active(&self) -> bool {
        !self.title.is_empty() && self.state != "stopped"
    }

    pub fn state_icon(&self) -> &'static str {
        match self.state.as_str() {
            "playing" => "▶",
            "paused" => "⏸",
            _ => "⏹",
        }
    }

    pub fn progress_ratio(&self) -> f64 {
        if self.duration_secs <= 0.0 {
            return 0.0;
        }
        (self.position_secs / self.duration_secs).clamp(0.0, 1.0)
    }
}

#[derive(Debug, Deserialize)]
struct StatusResponse {
    ok: bool,
    state: Option<String>,
    track: Option<TrackInfo>,
    position: Option<f64>,
    duration: Option<f64>,
    total: Option<u32>,
    shuffle: Option<bool>,
    repeat: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TrackInfo {
    title: Option<String>,
    artist: Option<String>,
    path: Option<String>,
}

pub fn poll_player(player: &Player) -> Option<PlaybackInfo> {
    if !player.is_daemon_running() {
        return None;
    }

    let output = player.run_for_output(&["status", "--json"]).ok()?;
    parse_status_json(&output)
}

pub fn parse_status_json(json: &str) -> Option<PlaybackInfo> {
    let response: StatusResponse = serde_json::from_str(json).ok()?;
    if !response.ok {
        return None;
    }

    let track = response.track.unwrap_or(TrackInfo {
        title: None,
        artist: None,
        path: None,
    });

    let title = track.title.unwrap_or_default();
    let path = track.path.clone().unwrap_or_default();
    let fallback_title = Path::new(&path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("")
        .to_string();

    Some(PlaybackInfo {
        state: response.state.unwrap_or_else(|| "stopped".into()),
        title: if title.is_empty() { fallback_title } else { title },
        artist: track.artist.unwrap_or_default(),
        path,
        position_secs: response.position.unwrap_or(0.0),
        duration_secs: response.duration.unwrap_or(0.0),
        playlist_total: response.total.unwrap_or(0),
        shuffle: response.shuffle.unwrap_or(false),
        repeat: response.repeat.unwrap_or_else(|| "off".into()),
    })
}

pub fn format_duration(secs: f64) -> String {
    let total = secs.max(0.0) as u64;
    let hours = total / 3600;
    let minutes = (total % 3600) / 60;
    let seconds = total % 60;

    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes}:{seconds:02}")
    }
}

pub fn progress_bar(ratio: f64, width: usize) -> String {
    if width == 0 {
        return String::new();
    }

    let filled = (ratio.clamp(0.0, 1.0) * width as f64).round() as usize;
    let filled = filled.min(width);
    format!("{}{}", "█".repeat(filled), "░".repeat(width - filled))
}

pub fn activity_wave(tick: u64, width: usize, active: bool) -> String {
    if !active || width == 0 {
        return " ".repeat(width);
    }

    const BARS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
    (0..width)
        .map(|col| {
            let phase = tick.wrapping_add(col as u64);
            let level = (phase % 8) as usize;
            BARS[level]
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_status_json_extracts_track_and_timing() {
        let json = r#"{
            "ok": true,
            "state": "playing",
            "track": {
                "title": "Narcosis",
                "artist": "Sound Of Syndrome",
                "path": "/music/01 - Narcosis.flac"
            },
            "position": 47.36,
            "duration": 960,
            "total": 37,
            "shuffle": true,
            "repeat": "All"
        }"#;

        let info = parse_status_json(json).expect("status");
        assert_eq!(info.title, "Narcosis");
        assert_eq!(info.artist, "Sound Of Syndrome");
        assert_eq!(info.state, "playing");
        assert!((info.progress_ratio() - 47.36 / 960.0).abs() < 0.001);
        assert!(info.shuffle);
        assert_eq!(info.repeat, "All");
    }

    #[test]
    fn format_duration_renders_minutes_and_hours() {
        assert_eq!(format_duration(47.0), "0:47");
        assert_eq!(format_duration(125.0), "2:05");
        assert_eq!(format_duration(3725.0), "1:02:05");
    }

    #[test]
    fn progress_bar_fills_proportionally() {
        assert_eq!(progress_bar(0.5, 10), "█████░░░░░");
        assert_eq!(progress_bar(1.0, 4), "████");
        assert_eq!(progress_bar(0.0, 4), "░░░░");
    }
}

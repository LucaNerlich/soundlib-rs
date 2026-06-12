//! The playback snapshot model and "Now Playing" render helpers.
//!
//! [`PlaybackInfo`] is the immutable view of the engine's state that the audio
//! thread publishes and the TUI renders. The free functions here ([`format_duration`],
//! [`progress_bar`], [`activity_wave`]) turn that data into the strings drawn in
//! the Now Playing pane.

/// A point-in-time snapshot of what the playback engine is doing.
///
/// Produced by the audio thread and consumed by the UI. String fields use
/// simple lowercase tokens so they can be rendered directly:
/// `state` is `"playing"`, `"paused"`, or `"stopped"`, and `repeat` is
/// `"off"`, `"all"`, or `"one"`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PlaybackInfo {
    /// Playback state: `"playing"`, `"paused"`, or `"stopped"`.
    pub state: String,
    /// Track title from tags, or the file name when no tag is present.
    pub title: String,
    /// Track artist from tags, or empty when unknown.
    pub artist: String,
    /// Absolute path of the current track.
    pub path: String,
    /// Elapsed playback position of the current track, in seconds.
    pub position_secs: f64,
    /// Total duration of the current track in seconds, or `0.0` if unknown.
    pub duration_secs: f64,
    /// Number of tracks currently in the queue.
    pub playlist_total: u32,
    /// Whether shuffle is enabled.
    pub shuffle: bool,
    /// Repeat mode: `"off"`, `"all"`, or `"one"`.
    pub repeat: String,
}

impl PlaybackInfo {
    /// Returns `true` when something is loaded and not stopped, i.e. the Now
    /// Playing pane should show track details rather than the idle message.
    pub fn is_active(&self) -> bool {
        !self.title.is_empty() && self.state != "stopped"
    }

    /// A single-character glyph for the current [`state`](Self::state):
    /// `▶` playing, `⏸` paused, `⏹` otherwise.
    pub fn state_icon(&self) -> &'static str {
        match self.state.as_str() {
            "playing" => "▶",
            "paused" => "⏸",
            _ => "⏹",
        }
    }

    /// Playback progress as a fraction in `0.0..=1.0`. Returns `0.0` when the
    /// duration is unknown (avoiding division by zero).
    pub fn progress_ratio(&self) -> f64 {
        if self.duration_secs <= 0.0 {
            return 0.0;
        }
        (self.position_secs / self.duration_secs).clamp(0.0, 1.0)
    }
}

/// Format a number of seconds as `m:ss`, or `h:mm:ss` once it reaches an hour.
///
/// Negative input is treated as zero.
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

/// Render a fixed-`width` progress bar for `ratio` (clamped to `0.0..=1.0`)
/// using `█` for the filled portion and `░` for the remainder.
pub fn progress_bar(ratio: f64, width: usize) -> String {
    if width == 0 {
        return String::new();
    }

    let filled = (ratio.clamp(0.0, 1.0) * width as f64).round() as usize;
    let filled = filled.min(width);
    format!("{}{}", "█".repeat(filled), "░".repeat(width - filled))
}

/// Render a `width`-wide animated bar-graph "visualizer" line.
///
/// `tick` advances the animation each UI refresh. When `active` is `false` (or
/// `width` is zero) a blank line of spaces is returned instead.
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
    fn progress_ratio_clamps_and_divides() {
        let info = PlaybackInfo {
            position_secs: 47.36,
            duration_secs: 960.0,
            ..PlaybackInfo::default()
        };
        assert!((info.progress_ratio() - 47.36 / 960.0).abs() < 0.001);

        let zero = PlaybackInfo::default();
        assert_eq!(zero.progress_ratio(), 0.0);
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

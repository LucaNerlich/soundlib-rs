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

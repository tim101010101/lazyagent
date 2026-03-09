use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TimingConfig {
    #[serde(default = "default_refresh_interval_ms")]
    pub refresh_interval_ms: u64,
    #[serde(default = "default_preview_interval_ms")]
    pub preview_interval_ms: u64,
    #[serde(default = "default_passthrough_preview_ms")]
    pub passthrough_preview_ms: u64,
    #[serde(default = "default_double_esc_ms")]
    pub double_esc_ms: u64,
    #[serde(default = "default_poll_tick_ms")]
    pub poll_tick_ms: u64,
}

fn default_refresh_interval_ms() -> u64 {
    2000
}
fn default_preview_interval_ms() -> u64 {
    200
}
fn default_passthrough_preview_ms() -> u64 {
    100
}
fn default_double_esc_ms() -> u64 {
    300
}
fn default_poll_tick_ms() -> u64 {
    100
}

const MIN_POLL_TICK_MS: u64 = 50;
const MAX_POLL_TICK_MS: u64 = 500;

impl Default for TimingConfig {
    fn default() -> Self {
        Self {
            refresh_interval_ms: default_refresh_interval_ms(),
            preview_interval_ms: default_preview_interval_ms(),
            passthrough_preview_ms: default_passthrough_preview_ms(),
            double_esc_ms: default_double_esc_ms(),
            poll_tick_ms: default_poll_tick_ms(),
        }
    }
}

impl TimingConfig {
    /// Poll tick clamped to 50-500ms range.
    pub fn poll_tick_clamped(&self) -> u64 {
        self.poll_tick_ms.clamp(MIN_POLL_TICK_MS, MAX_POLL_TICK_MS)
    }

    /// Number of ticks per refresh interval.
    pub fn refresh_ticks(&self) -> u32 {
        let tick = self.poll_tick_clamped();
        (self.refresh_interval_ms / tick).max(1) as u32
    }

    /// Number of ticks per preview interval.
    pub fn preview_ticks(&self) -> u32 {
        let tick = self.poll_tick_clamped();
        (self.preview_interval_ms / tick).max(1) as u32
    }

    /// Number of ticks per passthrough preview interval.
    pub fn passthrough_preview_ticks(&self) -> u32 {
        let tick = self.poll_tick_clamped();
        (self.passthrough_preview_ms / tick).max(1) as u32
    }

    /// Double-Esc timeout as Duration.
    pub fn double_esc_duration(&self) -> std::time::Duration {
        std::time::Duration::from_millis(self.double_esc_ms)
    }

    /// Poll tick as Duration.
    pub fn poll_tick_duration(&self) -> std::time::Duration {
        std::time::Duration::from_millis(self.poll_tick_clamped())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_defaults() {
        let t = TimingConfig::default();
        assert_eq!(t.refresh_interval_ms, 2000);
        assert_eq!(t.poll_tick_ms, 100);
        assert_eq!(t.refresh_ticks(), 20);
        assert_eq!(t.preview_ticks(), 2);
        assert_eq!(t.passthrough_preview_ticks(), 1);
    }

    #[test]
    fn test_poll_tick_clamped() {
        let mut t = TimingConfig::default();
        t.poll_tick_ms = 10;
        assert_eq!(t.poll_tick_clamped(), 50);
        t.poll_tick_ms = 1000;
        assert_eq!(t.poll_tick_clamped(), 500);
    }

    #[test]
    fn test_partial_toml() {
        let toml_str = r#"refresh_interval_ms = 5000"#;
        let t: TimingConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(t.refresh_interval_ms, 5000);
        assert_eq!(t.poll_tick_ms, 100); // default
    }

    #[test]
    fn test_clamp_boundaries() {
        let mut t = TimingConfig::default();
        t.poll_tick_ms = 50; // exactly at min
        assert_eq!(t.poll_tick_clamped(), 50);
        t.poll_tick_ms = 500; // exactly at max
        assert_eq!(t.poll_tick_clamped(), 500);
        t.poll_tick_ms = 0;
        assert_eq!(t.poll_tick_clamped(), 50);
    }

    #[test]
    fn test_tick_calculations_custom_poll() {
        let mut t = TimingConfig::default();
        t.poll_tick_ms = 200;
        // 2000ms / 200ms = 10 ticks
        assert_eq!(t.refresh_ticks(), 10);
        // 200ms / 200ms = 1
        assert_eq!(t.preview_ticks(), 1);
        // 100ms / 200ms = 0 → .max(1) = 1
        assert_eq!(t.passthrough_preview_ticks(), 1);
    }

    #[test]
    fn test_tick_min_one() {
        let mut t = TimingConfig::default();
        t.poll_tick_ms = 500; // max clamp
        t.passthrough_preview_ms = 100;
        // 100 / 500 = 0 → max(1) = 1
        assert_eq!(t.passthrough_preview_ticks(), 1);
    }

    #[test]
    fn test_double_esc_duration() {
        let t = TimingConfig::default();
        assert_eq!(t.double_esc_duration(), std::time::Duration::from_millis(300));
    }

    #[test]
    fn test_poll_tick_duration() {
        let mut t = TimingConfig::default();
        assert_eq!(t.poll_tick_duration(), std::time::Duration::from_millis(100));
        t.poll_tick_ms = 10; // clamps to 50
        assert_eq!(t.poll_tick_duration(), std::time::Duration::from_millis(50));
    }

    #[test]
    fn test_full_toml() {
        let toml_str = r#"
refresh_interval_ms = 5000
preview_interval_ms = 500
passthrough_preview_ms = 250
double_esc_ms = 400
poll_tick_ms = 200
"#;
        let t: TimingConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(t.refresh_interval_ms, 5000);
        assert_eq!(t.preview_interval_ms, 500);
        assert_eq!(t.passthrough_preview_ms, 250);
        assert_eq!(t.double_esc_ms, 400);
        assert_eq!(t.poll_tick_ms, 200);
        assert_eq!(t.refresh_ticks(), 25);
    }
}

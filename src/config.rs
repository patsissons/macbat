//! Persists UI preferences (color, view, sparkline scale, history window) to ~/.config/macbat.json.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum View {
    #[default]
    Sparkline,
    Gauge,
}

impl View {
    pub fn toggle(self) -> Self {
        match self {
            View::Sparkline => View::Gauge,
            View::Gauge => View::Sparkline,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            View::Sparkline => "sparkline",
            View::Gauge => "gauge",
        }
    }
}

/// How the sparklines pick their ceiling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Scale {
    /// Power In against the charger's negotiated power; System and Battery against their
    /// visible peak.
    #[default]
    Default,
    /// Peak of the visible bars, marked with a watermark line above them.
    Auto,
    /// The charger's negotiated power (or the session peak on battery).
    Full,
}

impl Scale {
    pub fn next(self) -> Self {
        match self {
            Scale::Default => Scale::Auto,
            Scale::Auto => Scale::Full,
            Scale::Full => Scale::Default,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Scale::Default => "default",
            Scale::Auto => "auto",
            Scale::Full => "full",
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub color: usize,
    pub view: View,
    pub scale: Scale,
    pub window: usize,
}

fn path() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config").join("macbat.json"))
}

impl Config {
    pub fn load() -> Self {
        path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) {
        let Some(p) = path() else { return };
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(s) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(p, s);
        }
    }
}

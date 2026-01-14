use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use anyhow::Result;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WallpaperSettings {
    pub path: String,
    pub resolution: String,
    pub fps_preset: String,
    pub scaling_mode: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PerformanceSettings {
    pub pause_on_battery: bool,
    pub battery_threshold: i32,
    pub pause_on_fullscreen: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct StartupSettings {
    pub launch_with_windows: bool,
    pub start_minimized: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Settings {
    pub version: String,
    pub wallpaper: WallpaperSettings,
    pub performance: PerformanceSettings,
    pub startup: StartupSettings,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            version: "1.0.0".to_string(),
            wallpaper: WallpaperSettings {
                path: "".to_string(),
                resolution: "1080p".to_string(),
                fps_preset: "balanced".to_string(),
                scaling_mode: "fill".to_string(),
            },
            performance: PerformanceSettings {
                pause_on_battery: true,
                battery_threshold: 20,
                pause_on_fullscreen: true,
            },
            startup: StartupSettings {
                launch_with_windows: false,
                start_minimized: true,
            },
        }
    }
}

impl Settings {
    pub fn load() -> Result<Self> {
        let path = Self::config_path()?;
        if !path.exists() {
            return Ok(Self::default());
        }
        let content = std::fs::read_to_string(path)?;
        let settings = serde_json::from_str(&content)?;
        Ok(settings)
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::config_path()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }

    fn config_path() -> Result<PathBuf> {
        let mut path = dirs::config_dir().ok_or_else(|| anyhow::anyhow!("Could not find config directory"))?;
        path.push("Mew");
        path.push("settings.json");
        Ok(path)
    }
}

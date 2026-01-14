mod config;
mod performance;
mod ui;
mod utils;
mod wallpaper;

use crate::config::Settings;
use crate::wallpaper::WallpaperPlayer;
use crate::performance::PerformanceMonitor;
use std::sync::Arc;

slint::include_modules!();

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    tracing::info!("Starting Mew - Lightweight Live Wallpaper Engine");

    // 1. Load Settings
    let settings = Settings::load().unwrap_or_default();
    
    // 2. Initialize Core Components
    let player = WallpaperPlayer::new();
    let player_state = player.get_state();
    
    // Set initial state from settings
    {
        let mut s = player_state.lock().unwrap();
        s.path = settings.wallpaper.path.clone();
        s.fps = match settings.wallpaper.fps_preset.as_str() {
            "Power Saver" => 15,
            "Balanced" => 30,
            "Performance" => 60,
            _ => 30,
        };
    }

    let monitor = PerformanceMonitor::new(player_state.clone());

    // 3. Spwan Tasks
    let player_task = tokio::spawn(async move {
        if let Err(e) = player.run().await {
            tracing::error!("Player error: {}", e);
        }
    });

    let monitor_task = tokio::spawn(async move {
        if let Err(e) = monitor.run().await {
            tracing::error!("Monitor error: {}", e);
        }
    });

    // 4. UI Setup
    let ui = AppWindow::new()?;
    ui.set_wallpaper_path(settings.wallpaper.path.clone().into());
    // ui.set_fps_preset(settings.wallpaper.fps_preset.clone().into());
    ui.set_battery_threshold(settings.performance.battery_threshold);

    let ui_handle = ui.as_weak();
    let state_for_ui = player_state.clone();
    ui.on_browse_clicked(move || {
        let ui = ui_handle.unwrap();
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("Videos", &["mp4", "webm", "avi", "mkv"])
            .pick_file() 
        {
            let path_str = path.to_string_lossy().to_string();
            ui.set_wallpaper_path(path_str.clone().into());
        }
    });

    let ui_handle = ui.as_weak();
    let state_for_ui = player_state.clone();
    ui.on_apply_clicked(move |path, fps_preset, threshold| {
        let mut s = state_for_ui.lock().unwrap();
        s.path = path.to_string();
        s.fps = match fps_preset.as_str() {
            "Power Saver" => 15,
            "Balanced" => 30,
            "Performance" => 60,
            _ => 30,
        };
        
        // Save to settings
        let mut settings = Settings::load().unwrap_or_default();
        settings.wallpaper.path = path.to_string();
        settings.wallpaper.fps_preset = fps_preset.to_string();
        settings.performance.battery_threshold = threshold;
        let _ = settings.save();
        
        tracing::info!("Applied settings: {}", path);
    });

    ui.on_exit_clicked(move || {
        std::process::exit(0);
    });

    ui.run()?;

    Ok(())
}

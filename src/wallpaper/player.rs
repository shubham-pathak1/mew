use crate::wallpaper::{VideoDecoder, WallpaperRenderer};
use anyhow::Result;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::time::sleep;

pub struct PlayerState {
    pub is_paused: bool,
    pub fps: u32,
    pub path: String,
    pub resolution: String,
}

pub struct WallpaperPlayer {
    state: Arc<Mutex<PlayerState>>,
}

impl WallpaperPlayer {
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(PlayerState {
                is_paused: false,
                fps: 30,
                path: String::new(),
                resolution: "1080p".to_string(),
            })),
        }
    }

    pub fn get_state(&self) -> Arc<Mutex<PlayerState>> {
        self.state.clone()
    }

    pub async fn run(&self) -> Result<()> {
        let mut renderer = match WallpaperRenderer::new() {
            Ok(r) => r,
            Err(e) => {
                tracing::error!("Failed to initialize initial renderer: {}", e);
                // We'll try to recover in the loop
                return Err(e); // First init is still critical for task startup
            }
        };

        let mut decoder: Option<VideoDecoder> = None;
        let mut last_path = String::new();
        let mut last_resolution = String::new();
        let mut next_frame_target_time = Instant::now();
        let mut last_heartbeat = Instant::now();

        loop {
            // Heartbeat every 10 seconds to confirm the thread is alive
            if last_heartbeat.elapsed() > Duration::from_secs(10) {
                tracing::info!("Player Heartbeat: Engine Healthy (State: {})", 
                    if decoder.is_some() { "Streaming" } else { "Idle" });
                last_heartbeat = Instant::now();
            }

            let (paused, fps, path, resolution) = {
                let s = self.state.lock().unwrap();
                (s.is_paused, s.fps, s.path.clone(), s.resolution.clone())
            };

            if path.is_empty() {
                sleep(Duration::from_millis(500)).await;
                continue;
            }

            if path != last_path || resolution != last_resolution {
                tracing::info!("Reloading wallpaper: {} (Target: {})", path, resolution);
                
                // Logical Scaling Fix: Always target the PHYSICAL screen size to avoid "invisible" mismatch
                let (screen_w, screen_h) = renderer.screen_size();
                tracing::info!("Logical Decoder Target: {}x{}", screen_w, screen_h);

                decoder = match VideoDecoder::new(&path, screen_w, screen_h) {
                    Ok(d) => Some(d),
                    Err(e) => {
                        tracing::error!("Failed to load wallpaper: {}", e);
                        None
                    }
                };
                last_path = path;
                last_resolution = resolution;
            }

            if paused {
                sleep(Duration::from_millis(200)).await;
                continue;
            }

            if let Some(ref mut dec) = decoder {
                let frame_time = Duration::from_secs_f64(1.0 / fps as f64);

                if next_frame_target_time < Instant::now() {
                    next_frame_target_time = Instant::now();
                }

                let sleep_duration = next_frame_target_time.saturating_duration_since(Instant::now());
                if sleep_duration > Duration::ZERO {
                    sleep(sleep_duration).await;
                }

                // Immersion: No ? here.
                let next_frame_res = dec.next_frame();
                match next_frame_res {
                    Ok(Some(frame_data)) => {
                        if let Err(e) = renderer.render_frame(&frame_data, dec.width(), dec.height()) {
                            tracing::error!("Render error: {}. Recovering...", e);
                            match WallpaperRenderer::new() {
                                Ok(new_renderer) => {
                                    renderer = new_renderer;
                                    tracing::info!("Renderer recovered.");
                                }
                                Err(re_err) => {
                                    tracing::error!("Recovery failed: {}. Retrying next frame.", re_err);
                                    sleep(Duration::from_millis(100)).await;
                                }
                            }
                        }
                    }
                    Ok(None) => {
                        // Loop
                        let _ = dec.seek_to_start();
                        if let Ok(Some(frame_data)) = dec.next_frame() {
                            let _ = renderer.render_frame(&frame_data, dec.width(), dec.height());
                        }
                    }
                    Err(e) => {
                        tracing::error!("Decode error: {}. Retrying...", e);
                        sleep(Duration::from_millis(100)).await;
                    }
                }

                next_frame_target_time += frame_time;

            } else {
                sleep(Duration::from_millis(500)).await;
                next_frame_target_time = Instant::now();
            }
        }
    }
}

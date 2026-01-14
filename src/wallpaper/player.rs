use crate::wallpaper::{VideoDecoder, WallpaperRenderer};
use anyhow::Result;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::time::sleep;

pub struct PlayerState {
    pub is_paused: bool,
    pub fps: u32,
    pub path: String,
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
            })),
        }
    }

    pub fn get_state(&self) -> Arc<Mutex<PlayerState>> {
        self.state.clone()
    }

    pub async fn run(&self) -> Result<()> {
        let renderer = WallpaperRenderer::new()?;
        let mut decoder: Option<VideoDecoder> = None;
        let mut last_path = String::new();

        loop {
            let (paused, fps, path) = {
                let s = self.state.lock().unwrap();
                (s.is_paused, s.fps, s.path.clone())
            };

            if path.is_empty() {
                sleep(Duration::from_millis(100)).await;
                continue;
            }

            if path != last_path {
                tracing::info!("Loading new wallpaper: {}", path);
                decoder = match VideoDecoder::new(&path) {
                    Ok(d) => Some(d),
                    Err(e) => {
                        tracing::error!("Failed to load wallpaper: {}", e);
                        None
                    }
                };
                last_path = path;
            }

            if paused {
                sleep(Duration::from_millis(100)).await;
                continue;
            }

            if let Some(ref mut dec) = decoder {
                let start = Instant::now();
                
                match dec.next_frame()? {
                    Some(frame_data) => {
                        renderer.render_frame(&frame_data, dec.width(), dec.height())?;
                    }
                    None => {
                        // Loop
                        dec.seek_to_start()?;
                    }
                }

                let target_frame_time = Duration::from_micros(1_000_000 / fps as u64);
                let elapsed = start.elapsed();
                if elapsed < target_frame_time {
                    sleep(target_frame_time - elapsed).await;
                }
            } else {
                sleep(Duration::from_millis(100)).await;
            }
        }
    }
}

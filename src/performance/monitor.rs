use battery::Manager;
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowRect};
use std::sync::{Arc, Mutex};
use crate::wallpaper::player::PlayerState;
use tokio::time::{sleep, Duration};

pub struct PerformanceMonitor {
    state: Arc<Mutex<PlayerState>>,
}

impl PerformanceMonitor {
    pub fn new(state: Arc<Mutex<PlayerState>>) -> Self {
        Self { state }
    }

    pub async fn run(&self) -> anyhow::Result<()> {
        loop {
            let mut should_pause = false;

            // 1. Check Battery
            if let Ok(battery_manager) = Manager::new() {
                if let Ok(mut batteries) = battery_manager.batteries() {
                    if let Some(Ok(battery)) = batteries.next() {
                        let state = battery.state();
                        let percentage = battery.state_of_charge().value * 100.0;
                        
                        // TODO: Use threshold from settings
                        if state == battery::State::Discharging && percentage < 20.0 {
                            should_pause = true;
                        }
                    }
                }
            }
            // battery_manager is dropped here

            // 2. Check Fullscreen (simple heuristic)
            if !should_pause {
                unsafe {
                    let hwnd = GetForegroundWindow();
                    if !hwnd.0.is_null() {
                        let mut rect = windows::Win32::Foundation::RECT::default();
                        if GetWindowRect(hwnd, &mut rect).is_ok() {
                            let screen_w = windows::Win32::UI::WindowsAndMessaging::GetSystemMetrics(windows::Win32::UI::WindowsAndMessaging::SM_CXSCREEN);
                            let screen_h = windows::Win32::UI::WindowsAndMessaging::GetSystemMetrics(windows::Win32::UI::WindowsAndMessaging::SM_CYSCREEN);
                            
                            if (rect.right - rect.left).abs() >= screen_w - 5 && 
                               (rect.bottom - rect.top).abs() >= screen_h - 5 {
                                // Probably fullscreen
                                // Note: Need to exclude the wallpaper window itself and desktop
                                should_pause = true;
                            }
                        }
                    }
                }
            }

            {
                let mut s = self.state.lock().unwrap();
                s.is_paused = should_pause; 
            }

            sleep(Duration::from_secs(3)).await;
        }
    }
}

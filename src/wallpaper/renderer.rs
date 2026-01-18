use windows::Win32::Foundation::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::Win32::Graphics::Direct3D::*;
use windows::Win32::Graphics::Direct3D11::*;
use windows::Win32::Graphics::Dxgi::*;
use windows::Win32::System::LibraryLoader::*;
use windows::core::{PCWSTR, Interface};
use anyhow::Result;
use std::sync::mpsc;

pub struct WallpaperRenderer {
    #[allow(dead_code)]
    hwnd: HWND,
    parent_workerw: HWND,
    device: ID3D11Device,
    context: ID3D11DeviceContext,
    swapchain: IDXGISwapChain,
    texture_cache: Option<ID3D11Texture2D>,
    texture_size: (u32, u32),
    physical_size: (u32, u32),
}

// Safety: HWND is a handle that can be passed between threads on Windows.
// We ensure it is only used by one thread for rendering at a time.
unsafe impl Send for WallpaperRenderer {}
unsafe impl Sync for WallpaperRenderer {}

impl WallpaperRenderer {
    pub fn new() -> Result<Self> {
        let (tx, rx) = mpsc::channel::<Result<(isize, isize, i32, i32)>>();

        // Spawn a dedicated thread for the window and its message loop
        // This ensures interactions never block the high-precision render loop
        std::thread::spawn(move || {
            let mut workerw_hwnd = HWND::default();
            let res = (|| -> Result<isize> {
                let progman = unsafe { FindWindowW(windows::core::w!("Progman"), None)? };
                
                // Aggressive Shell Spawning for Canary/Insider Builds
                unsafe {
                    // Method 1: Standard 0x052C to Progman (0xD, 0x1)
                    SendMessageTimeoutW(progman, 0x052C, WPARAM(0xD), LPARAM(0x1), SMTO_NORMAL, 1000, None);
                    std::thread::sleep(std::time::Duration::from_millis(100));

                    // Method 2: Standard 0x052C to Progman (0, 0) - classic method
                    SendMessageTimeoutW(progman, 0x052C, WPARAM(0), LPARAM(0), SMTO_NORMAL, 1000, None);
                    std::thread::sleep(std::time::Duration::from_millis(100));
                    
                    // Method 3: Blocking SendMessage (last resort force)
                    SendMessageW(progman, 0x052C, WPARAM(0), LPARAM(0));
                }
                
                std::thread::sleep(std::time::Duration::from_millis(500));
                
                let sw = unsafe { GetSystemMetrics(SM_CXSCREEN) };
                let sh = unsafe { GetSystemMetrics(SM_CYSCREEN) };
                
                // PROPER SHELL DISCOVERY:
                // 1. Find SHELLDLL_DefView (contains icons) - it can be under Progman or a WorkerW
                // 2. Get its parent
                // 3. Find the sibling WorkerW (created by 0x052C) - that's our target
                
                let mut defview_parent = HWND::default();
                let mut target_workerw = HWND::default();
                
                // First check if DefView is under Progman
                let defview_in_progman = unsafe { 
                    FindWindowExW(progman, HWND::default(), windows::core::w!("SHELLDLL_DefView"), PCWSTR::null()) 
                };
                
                if let Ok(defview) = defview_in_progman {
                    if !defview.0.is_null() {
                        // DefView is under Progman - find sibling WorkerW
                        defview_parent = progman;
                        tracing::info!("Found DefView under Progman");
                    }
                }
                
                // If not found, search all WorkerW windows for DefView
                if defview_parent.0.is_null() {
                    let mut current = HWND::default();
                    loop {
                        current = unsafe { FindWindowExW(HWND::default(), current, windows::core::w!("WorkerW"), PCWSTR::null()) }.unwrap_or_default();
                        if current.0.is_null() { break; }
                        
                        let defview = unsafe { FindWindowExW(current, HWND::default(), windows::core::w!("SHELLDLL_DefView"), PCWSTR::null()) };
                        if let Ok(dv) = defview {
                            if !dv.0.is_null() {
                                defview_parent = current;
                                tracing::info!("Found DefView under WorkerW: {:?}", current);
                                break;
                            }
                        }
                    }
                }
                
                // Now find ANY screen-sized WorkerW that doesn't contain DefView
                // This is the wallpaper layer created by 0x052C
                let mut current = HWND::default();
                loop {
                    current = unsafe { FindWindowExW(HWND::default(), current, windows::core::w!("WorkerW"), PCWSTR::null()) }.unwrap_or_default();
                    if current.0.is_null() { break; }
                    
                    // Check if this WorkerW contains DefView - if so, skip it
                    let has_defview = unsafe { 
                        FindWindowExW(current, HWND::default(), windows::core::w!("SHELLDLL_DefView"), PCWSTR::null()) 
                    };
                    if let Ok(dv) = has_defview {
                        if !dv.0.is_null() { continue; } // Skip the DefView container
                    }
                    
                    // Check if this WorkerW is screen-sized
                    let mut rect = RECT::default();
                    if unsafe { GetWindowRect(current, &mut rect) }.is_ok() {
                        let w = rect.right - rect.left;
                        let h = rect.bottom - rect.top;
                        tracing::info!("Found WorkerW: {:?} ({}x{}) - screen is {}x{}", current, w, h, sw, sh);
                        if w >= sw - 10 && h >= sh - 10 { // Allow small tolerance
                            target_workerw = current;
                            tracing::info!("Selected wallpaper WorkerW: {:?}", current);
                            break;
                        }
                    }
                }
                
                // Final fallback selection
                let (final_parent, is_defview_parent) = if !target_workerw.0.is_null() {
                    (target_workerw, false)
                } else if !defview_parent.0.is_null() && defview_parent != progman {
                     // DefView is in a WorkerW, but we couldn't find a separate wallpaper WorkerW.
                     // We can try parenting to that WorkerW directly (behind DefView)?
                     // No, that hides us. We must parent TO DefView.
                     // Actually, if DefView is in WorkerW, we want to be sibling of DefView? 
                     // No, finding DefView handle itself is better.
                     
                     // Improve logic: If we found DefView (inside progman or workerw), let's find the DefView HWND itself
                     let defview_hwnd = unsafe {
                         if defview_parent == progman {
                             FindWindowExW(progman, HWND::default(), windows::core::w!("SHELLDLL_DefView"), PCWSTR::null()).unwrap_or_default()
                         } else {
                             FindWindowExW(defview_parent, HWND::default(), windows::core::w!("SHELLDLL_DefView"), PCWSTR::null()).unwrap_or_default()
                         }
                     };

                     if !defview_hwnd.0.is_null() {
                         tracing::warn!("No split WorkerW found. Fallback: Parenting directly to SHELLDLL_DefView.");
                         (defview_hwnd, true)
                     } else {
                         tracing::warn!("Critical: DefView parent found but DefView lost? Using Progman.");
                         (progman, false)
                     }
                } else {
                     // DefView is in Progman.
                     let defview_hwnd = unsafe { FindWindowExW(progman, HWND::default(), windows::core::w!("SHELLDLL_DefView"), PCWSTR::null()).unwrap_or_default() };
                     if !defview_hwnd.0.is_null() {
                         tracing::warn!("No split WorkerW found. Fallback: Parenting directly to SHELLDLL_DefView (in Progman).");
                         (defview_hwnd, true)
                     } else {
                         tracing::warn!("No suitable WorkerW and no DefView found. Using Progman.");
                         (progman, false)
                     }
                };
                
                let instance = unsafe { GetModuleHandleW(None)? };
                let window_class = windows::core::w!("MewWallpaperClass");
                
                let wc = WNDCLASSW {
                    lpfnWndProc: Some(wnd_proc),
                    hInstance: instance.into(),
                    lpszClassName: window_class,
                    ..Default::default()
                };

                unsafe { RegisterClassW(&wc) };

                // Visible child window with mouse pass-through handled by wnd_proc
                let hwnd = unsafe {
                    CreateWindowExW(
                        WS_EX_NOACTIVATE,
                        window_class,
                        windows::core::w!("Mew Wallpaper"),
                        WS_CHILD | WS_VISIBLE | WS_CLIPSIBLINGS | WS_CLIPCHILDREN,
                        0, 0, sw, sh,
                        final_parent, 
                        HMENU::default(), 
                        instance, 
                        None,
                    )?
                };

                unsafe {
                    // Position behind icons but in front of static wallpaper
                    // HWND_BOTTOM within WorkerW puts us at the right layer
                    let _ = ShowWindow(final_parent, SW_SHOW);
                    let _ = ShowWindow(hwnd, SW_SHOW);
                    // Use HWND(1) which is HWND_BOTTOM - behind icons, in front of background
                    let _ = SetWindowPos(hwnd, HWND(1 as *mut _), 0, 0, 0, 0, SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE);
                }

                let _ = tx.send(Ok((hwnd.0 as isize, final_parent.0 as isize, sw, sh)));
                Ok(hwnd.0 as isize)
            })();

            match res {
                Ok(_) => {
                    // Message loop is already running in the background of this thread
                    unsafe {
                        let mut msg = MSG::default();
                        while GetMessageW(&mut msg, HWND::default(), 0, 0).as_bool() {
                            let _ = TranslateMessage(&msg);
                            DispatchMessageW(&msg);
                        }
                    }
                }
                Err(e) => {
                    let _ = tx.send(Err(e));
                }
            }
        });

        let (hwnd_val, workerw_val, width, height) = rx.recv()??;
        let hwnd = HWND(hwnd_val as *mut _);
        let parent_workerw = HWND(workerw_val as *mut _);
        tracing::info!("Found WorkerW: {:?}, Created Wallpaper Window: {:?} ({}x{})", parent_workerw, hwnd, width, height);

        // Create D3D11 Device and Swapchain in the player thread
        let sc_desc = DXGI_SWAP_CHAIN_DESC {
            BufferDesc: windows::Win32::Graphics::Dxgi::Common::DXGI_MODE_DESC {
                Width: width as u32,
                Height: height as u32,
                Format: windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM,
                ..Default::default()
            },
            SampleDesc: windows::Win32::Graphics::Dxgi::Common::DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
            BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
            BufferCount: 3,
            OutputWindow: hwnd,
            Windowed: BOOL(1),
            SwapEffect: DXGI_SWAP_EFFECT_FLIP_DISCARD,
            ..Default::default()
        };

        let mut device = None;
        let mut context = None;
        let mut swapchain = None;

        unsafe {
            D3D11CreateDeviceAndSwapChain(
                None,
                D3D_DRIVER_TYPE_HARDWARE,
                HMODULE::default(),
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                None,
                D3D11_SDK_VERSION,
                Some(&sc_desc),
                Some(&mut swapchain),
                Some(&mut device),
                None,
                Some(&mut context),
            )?;
        }

        let device = device.ok_or_else(|| anyhow::anyhow!("Failed to create D3D11 device"))?;
        let context = context.ok_or_else(|| anyhow::anyhow!("Failed to create D3D11 context"))?;
        let swapchain = swapchain.ok_or_else(|| anyhow::anyhow!("Failed to create D3D11 swapchain"))?;

        Ok(Self { 
            hwnd, 
            parent_workerw, 
            device, 
            context, 
            swapchain, 
            texture_cache: None, 
            texture_size: (0, 0),
            physical_size: (width as u32, height as u32),
        })
    }



    pub fn screen_size(&self) -> (u32, u32) {
        self.physical_size
    }

    pub fn render_frame(&mut self, data: &[u8], width: u32, height: u32) -> Result<()> {
        unsafe {
            // Verify parent still exists (Shell might have restarted)
            if !IsWindow(self.parent_workerw).as_bool() {
                return Err(anyhow::anyhow!("Parent WorkerW was lost. Shell may have restarted."));
            }

            // check if swapchain needs resizing
            let sc_desc = self.swapchain.GetDesc()?;
            
            if sc_desc.BufferDesc.Width != width || sc_desc.BufferDesc.Height != height {
                tracing::info!("Resizing swapchain: {}x{} -> {}x{}", sc_desc.BufferDesc.Width, sc_desc.BufferDesc.Height, width, height);
                // In D3D11, we must release ALL references to the backbuffer before resizing
                // We don't hold any long-lived references, but we'll ensure the context is flushed.
                self.context.ClearState();
                self.context.Flush();
                
                self.swapchain.ResizeBuffers(0, width, height, windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_UNKNOWN, DXGI_SWAP_CHAIN_FLAG(0))?;
                tracing::info!("Swapchain resized successfully.");
            }

            if self.texture_cache.is_none() || self.texture_size != (width, height) {
                let desc = D3D11_TEXTURE2D_DESC {
                    Width: width,
                    Height: height,
                    MipLevels: 1,
                    ArraySize: 1,
                    Format: windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM,
                    SampleDesc: windows::Win32::Graphics::Dxgi::Common::DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
                    Usage: D3D11_USAGE_DEFAULT,
                    BindFlags: (D3D11_BIND_SHADER_RESOURCE.0 | D3D11_BIND_RENDER_TARGET.0) as u32,
                    ..Default::default()
                };

                let mut texture = None;
                self.device.CreateTexture2D(&desc, None, Some(&mut texture))?;
                let texture: ID3D11Texture2D = texture.ok_or_else(|| anyhow::anyhow!("Failed to create D3D11 texture"))?;
                
                self.texture_cache = Some(texture);
                self.texture_size = (width, height);
                tracing::info!("Created new D3D11 texture: {}x{}", width, height);
            }

            let texture = self.texture_cache.as_ref().unwrap();
            let resource: ID3D11Resource = texture.cast()?;
            self.context.UpdateSubresource(&resource, 0, None, data.as_ptr() as *const _, width * 4, 0);

            let back_buffer: ID3D11Texture2D = self.swapchain.GetBuffer(0)?;
            self.context.CopyResource(&back_buffer, texture);
            
            // Use Present(0, 0) for guaranteed frame delivery
            // VSync=0 means no waiting, immediate presentation
            let _ = self.swapchain.Present(0, windows::Win32::Graphics::Dxgi::DXGI_PRESENT(0));
        }
        Ok(())
    }
    
}

unsafe extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_NCHITTEST => LRESULT(-1), // HTTRANSPARENT: let all clicks pass through
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize), // Don't activate, let input go to shell
        WM_SETCURSOR => LRESULT(1), // Handle cursor ourselves (hidden/pass-through)
        WM_ERASEBKGND => LRESULT(1), // Don't erase, we handle painting
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

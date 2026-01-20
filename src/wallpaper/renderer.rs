use windows::Win32::Foundation::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::Win32::Graphics::Direct3D::*;
use windows::Win32::Graphics::Direct3D11::*;
use windows::Win32::Graphics::Dxgi::*;
use windows::Win32::System::LibraryLoader::*;
use windows::core::{PCWSTR, Interface};
use anyhow::Result;
use ffmpeg_next as ffmpeg;
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
            let res = (|| -> Result<isize> {
                // -------------------------------------------------------------------------
                // PHASE 19: WINDOWS 11 24H2 APPROACH (Based on Lively Wallpaper research)
                // Create LAYERED child of Progman, positioned BETWEEN DefView and wallpaper
                // This is the modern approach that works on Windows 11 24H2+
                // -------------------------------------------------------------------------
                
                let progman = unsafe { FindWindowW(windows::core::w!("Progman"), None).unwrap_or_default() };
                
                if progman.0.is_null() {
                    return Err(anyhow::anyhow!("CRITICAL: Could not find Progman"));
                }
                
                tracing::info!("Found Progman: {:?}", progman);
                
                // Step 1: Send 0x052C to ensure desktop is in "raised" state
                // This is still important even on 24H2
                unsafe {
                    let _ = SendMessageTimeoutW(progman, 0x052C, WPARAM(0xD), LPARAM(0x1), SMTO_NORMAL, 1000, None);
                    tracing::info!("Sent 0x052C to Progman");
                }
                
                std::thread::sleep(std::time::Duration::from_millis(100));
                
                // Step 2: Find DefView (the icon layer)
                let mut defview_hwnd = unsafe {
                    FindWindowExW(progman, HWND::default(), windows::core::w!("SHELLDLL_DefView"), PCWSTR::null()).unwrap_or_default()
                };
                
                // Fallback: Check in WorkerW windows
                if defview_hwnd.0.is_null() {
                    let mut current = HWND::default();
                    loop {
                        current = unsafe { FindWindowExW(HWND::default(), current, windows::core::w!("WorkerW"), PCWSTR::null()) }.unwrap_or_default();
                        if current.0.is_null() { break; }
                        
                        let dv = unsafe { FindWindowExW(current, HWND::default(), windows::core::w!("SHELLDLL_DefView"), PCWSTR::null()) }.unwrap_or_default();
                        if !dv.0.is_null() {
                            defview_hwnd = dv;
                            break;
                        }
                    }
                }
                
                if defview_hwnd.0.is_null() {
                    return Err(anyhow::anyhow!("CRITICAL: Could not find SHELLDLL_DefView"));
                }
                
                tracing::info!("Found DefView: {:?}", defview_hwnd);
                
                // Step 3: Find SysListView32 and make its background transparent
                let syslistview = unsafe {
                    FindWindowExW(defview_hwnd, HWND::default(), windows::core::w!("SysListView32"), PCWSTR::null()).unwrap_or_default()
                };
                
                tracing::info!("Found SysListView32: {:?}", syslistview);
                
                // CRITICAL: Make SysListView32 background transparent so icons float over wallpaper
                if !syslistview.0.is_null() {
                    unsafe {
                        SendMessageW(syslistview, 0x1026, WPARAM(0), LPARAM(0xFFFFFFFF)); // LVM_SETTEXTBKCOLOR = CLR_NONE
                        SendMessageW(syslistview, 0x1001, WPARAM(0), LPARAM(0xFFFFFFFF)); // LVM_SETBKCOLOR = CLR_NONE
                        tracing::info!("Set SysListView32 background to CLR_NONE (transparent)");
                    }
                }
                
                let sw = unsafe { GetSystemMetrics(SM_CXSCREEN) };
                let sh = unsafe { GetSystemMetrics(SM_CYSCREEN) };
                
                // Register our window class
                let instance = unsafe { GetModuleHandleW(None)? };
                let window_class = windows::core::w!("MewWallpaperClass");
                
                let wc = WNDCLASSW {
                    lpfnWndProc: Some(wnd_proc),
                    hInstance: instance.into(),
                    lpszClassName: window_class,
                    ..Default::default()
                };
                unsafe { RegisterClassW(&wc) };
                
                // =========================================================================
                // PHASE 23: PROGMAN SIBLING + TRANSPARENT DEFVIEW
                // This is the most stable approach for Windows 11 24H2 Canary.
                // =========================================================================
                let hwnd = unsafe {
                    CreateWindowExW(
                        WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_NOACTIVATE,
                        window_class,
                        windows::core::w!("Mew Wallpaper"),
                        WS_CHILD | WS_VISIBLE,
                        0, 0, sw, sh,
                        progman, // Sibling of DefView
                        HMENU::default(),
                        instance,
                        None,
                    )?
                };
                
                tracing::info!("Created PROGMAN sibling wallpaper window: {:?}", hwnd);
                
                // Make layered window fully opaque for rendering
                unsafe {
                    let result = SetLayeredWindowAttributes(hwnd, COLORREF(0), 255, LWA_ALPHA);
                    tracing::info!("SetLayeredWindowAttributes: {:?}", result);
                }
                
                // Position our wallpaper immediately BEHIND DefView
                unsafe {
                    let result = SetWindowPos(
                        hwnd,
                        defview_hwnd, // Insert AFTER DefView -> visually BELOW it
                        0, 0, sw, sh,
                        SWP_NOACTIVATE | SWP_SHOWWINDOW
                    );
                    tracing::info!("Wallpaper positioned behind DefView: {:?}", result);
                }
                
                // Ensure DefView doesn't have an opaque background
                if !syslistview.0.is_null() {
                    unsafe {
                        SendMessageW(syslistview, 0x1026, WPARAM(0), LPARAM(0xFFFFFFFF)); // LVM_SETTEXTBKCOLOR = CLR_NONE
                        SendMessageW(syslistview, 0x1001, WPARAM(0), LPARAM(0xFFFFFFFF)); // LVM_SETBKCOLOR = CLR_NONE
                        
                        // Force Redraw of the desktop
                        let _ = windows::Win32::Graphics::Gdi::InvalidateRect(syslistview, None, BOOL(1));
                        let _ = windows::Win32::Graphics::Gdi::InvalidateRect(defview_hwnd, None, BOOL(1));
                        tracing::info!("Forced desktop redraw for icons");
                    }
                }
                
                // Force redraw of wallpaper
                unsafe {
                    let _ = ShowWindow(hwnd, SW_SHOW);
                    let _ = windows::Win32::Graphics::Gdi::InvalidateRect(progman, None, BOOL(1));
                }
                
                let _ = tx.send(Ok((hwnd.0 as isize, progman.0 as isize, sw, sh)));
                Ok(hwnd.0 as isize)
            })();

            match res {
                Ok(_) => {
                    // Message loop for our window
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
            
            // Use VSync Present(1, 0) to synchronize with display refresh
            // This reduces GPU thrashing and eliminates lag spikes
            let _ = self.swapchain.Present(1, windows::Win32::Graphics::Dxgi::DXGI_PRESENT(0));
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

unsafe fn find_correct_workerw_layer(result: &mut HWND) {
    EnumWindows(Some(find_workerw_enum_proc), LPARAM(result as *mut _ as isize));
}

unsafe extern "system" fn find_workerw_enum_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let result_ptr = lparam.0 as *mut HWND;
    
    // Check if visible
    if !IsWindowVisible(hwnd).as_bool() {
        return BOOL(1);
    }
    
    // Check class name
    let mut class_name = [0u16; 256];
    let len = GetClassNameW(hwnd, &mut class_name);
    let class = String::from_utf16_lossy(&class_name[..len as usize]);
    
    if class == "WorkerW" {
        // Check if it has DefView
        let defview = FindWindowExW(hwnd, HWND::default(), windows::core::w!("SHELLDLL_DefView"), PCWSTR::null()).unwrap_or_default();
        
        if defview.0.is_null() {
            // This is a candidate (WorkerW without DefView)
            // Ideally we want the one created by 0x052C.
            // Usually there is only one visible WorkerW without DefView after the split.
            *result_ptr = hwnd;
            return BOOL(0); // Stop enumeration
        }
    }
    
    BOOL(1)
}

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
                
                unsafe {
                    SendMessageTimeoutW(progman, 0x052C, WPARAM(0), LPARAM(0), SMTO_NORMAL, 1000, None);
                }
                
                std::thread::sleep(std::time::Duration::from_millis(1000));
                
                let sw = unsafe { GetSystemMetrics(SM_CXSCREEN) };
                let sh = unsafe { GetSystemMetrics(SM_CYSCREEN) };
                
                let mut best_workerw = HWND::default();
                let mut current = HWND::default();
                loop {
                    current = unsafe { FindWindowExW(HWND::default(), current, windows::core::w!("WorkerW"), PCWSTR::null()) }.unwrap_or_default();
                    if current.0.is_null() { break; }
                    
                    let mut rect = RECT::default();
                    if unsafe { GetWindowRect(current, &mut rect) }.is_ok() {
                        let w = rect.right - rect.left;
                        let h = rect.bottom - rect.top;
                        if w == sw && h == sh {
                            let defview = unsafe { FindWindowExW(current, HWND::default(), windows::core::w!("SHELLDLL_DefView"), PCWSTR::null()) };
                            if defview.is_err() || defview.unwrap().0.is_null() {
                                best_workerw = current;
                                break;
                            }
                        }
                    }
                }

                workerw_hwnd = if !best_workerw.0.is_null() {
                    best_workerw
                } else {
                    progman
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

                // Nuclear visibility: Opaque, clipping-aware child window
                let hwnd = unsafe {
                    CreateWindowExW(
                        WS_EX_NOACTIVATE,
                        window_class,
                        windows::core::w!("Mew Wallpaper"),
                        WS_CHILD | WS_VISIBLE | WS_CLIPSIBLINGS | WS_CLIPCHILDREN,
                        0, 0, sw, sh,
                        workerw_hwnd, 
                        HMENU::default(), 
                        instance, 
                        None,
                    )?
                };

                unsafe {
                    // Force parent and child to be visible
                    let _ = ShowWindow(workerw_hwnd, SW_SHOW);
                    let _ = ShowWindow(hwnd, SW_SHOW);
                    SetWindowPos(hwnd, HWND_TOP, 0, 0, sw, sh, SWP_NOACTIVATE | SWP_SHOWWINDOW)?;
                }

                let _ = tx.send(Ok((hwnd.0 as isize, workerw_hwnd.0 as isize, sw, sh)));
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
            self.context.Flush();
            
            // Use blocking Present for now to ensure visibility
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

use windows::Win32::Foundation::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::Graphics::Dwm::*;
use anyhow::Result;

pub struct WallpaperRenderer {
    hwnd: HWND,
}

impl WallpaperRenderer {
    pub fn new() -> Result<Self> {
        // Implementation of the "WorkerW" trick to get behind desktop icons
        let progman = unsafe { FindWindowW(windows::core::w!("Progman"), None)? };
        
        // Signal Progman to create WorkerW
        unsafe {
            SendMessageTimeoutW(
                progman,
                0x052C, // WM_ERASEBKGND or undocumented magic?
                WPARAM(0),
                LPARAM(0),
                SMTO_NORMAL,
                1000,
                None,
            );
        }

        let mut workerw = HWND::default();
        unsafe {
            EnumWindows(Some(Self::enum_window), LPARAM(&mut workerw as *mut _ as isize))?;
        }

        if workerw.0 == 0 {
            return Err(anyhow::anyhow!("Failed to find WorkerW window"));
        }

        // Create our layered window
        let instance = unsafe { GetModuleHandleW(None)? };
        let window_class = windows::core::w!("MewWallpaperClass");
        
        let wc = WNDCLASSW {
            lpfnWndProc: Some(DefWindowProcW),
            hInstance: instance.into(),
            lpszClassName: window_class,
            ..Default::default()
        };

        unsafe { RegisterClassW(&wc) };

        let hwnd = unsafe {
            CreateWindowExW(
                WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOOLWINDOW,
                window_class,
                windows::core::w!("Mew Wallpaper"),
                WS_POPUP | WS_VISIBLE,
                0, 0, GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN),
                None, None, instance, None,
            )?
        };

        // Set parent to WorkerW to be behind icons
        unsafe { SetParent(hwnd, workerw)? };

        Ok(Self { hwnd })
    }

    unsafe extern "system" fn enum_window(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let shell_dll_view = FindWindowExW(hwnd, None, windows::core::w!("SHELLDLL_DefView"), None);
        if let Ok(shell_dll_view) = shell_dll_view {
            if shell_dll_view.0 != 0 {
                let workerw = FindWindowExW(None, hwnd, windows::core::w!("WorkerW"), None);
                if let Ok(workerw) = workerw {
                    if workerw.0 != 0 {
                        let ptr = lparam.0 as *mut HWND;
                        *ptr = workerw;
                        return BOOL(0); // Stop enumerating
                    }
                }
            }
        }
        BOOL(1) // Continue enumerating
    }

    pub fn render_frame(&self, data: &[u8], width: u32, height: u32) -> Result<()> {
        unsafe {
            let hdc = GetDC(self.hwnd);
            let mem_dc = CreateCompatibleDC(hdc);
            
            let mut bmi = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: width as i32,
                    biHeight: -(height as i32), // Top-down
                    biPlanes: 1,
                    biBitCount: 24,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };

            let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
            let bitmap = CreateDIBSection(
                hdc,
                &bmi,
                DIB_RGB_COLORS,
                &mut bits,
                None,
                0,
            )?;

            std::ptr::copy_nonoverlapping(data.as_ptr(), bits as *mut u8, data.len());

            SelectObject(mem_dc, bitmap);
            
            let screen_width = GetSystemMetrics(SM_CXSCREEN);
            let screen_height = GetSystemMetrics(SM_CYSCREEN);

            StretchBlt(
                hdc, 0, 0, screen_width, screen_height,
                mem_dc, 0, 0, width as i32, height as i32,
                SRCCOPY,
            )?;

            DeleteObject(bitmap);
            DeleteDC(mem_dc);
            ReleaseDC(self.hwnd, hdc);
        }
        Ok(())
    }
}

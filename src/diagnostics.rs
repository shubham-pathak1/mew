use windows::Win32::Foundation::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use std::ffi::c_void;

pub fn dump_desktop_hierarchy() {
    tracing::info!("--- DESKTOP HIERARCHY DUMP START (ENUM MODE) ---");

    unsafe {
        EnumWindows(Some(enum_window_callback), LPARAM(0));
    }

    tracing::info!("--- DESKTOP HIERARCHY DUMP END ---");
}

unsafe extern "system" fn enum_window_callback(hwnd: HWND, _lparam: LPARAM) -> BOOL {
    if !IsWindowVisible(hwnd).as_bool() {
        return BOOL(1); // Continue but skip invisible
    }
    
    // Get Class Name
    let mut class_name = [0u16; 256];
    let len = GetClassNameW(hwnd, &mut class_name);
    let class = String::from_utf16_lossy(&class_name[..len as usize]);

    // Filter for interesting windows
    if class == "Progman" || class == "WorkerW" || class == "SHELLDLL_DefView" {
        log_window(&class, hwnd);

        // Check children for DefView specifically if this is a container
        let defview = FindWindowExW(hwnd, HWND::default(), windows::core::w!("SHELLDLL_DefView"), windows::core::PCWSTR::null()).unwrap_or_default();
        if !defview.0.is_null() {
            tracing::info!("    -> FOUND SHELLDLL_DefView CHILD: {:?}", defview);
            let mut rect = RECT::default();
            let _ = GetWindowRect(defview, &mut rect);
            tracing::info!("       Size: {}x{}", rect.right - rect.left, rect.bottom - rect.top);
        }
    }

    BOOL(1)
}

unsafe fn log_window(name: &str, hwnd: HWND) {
    let mut rect = RECT::default();
    let _ = GetWindowRect(hwnd, &mut rect);
    let width = rect.right - rect.left;
    let height = rect.bottom - rect.top;
    
    let style = GetWindowLongW(hwnd, GWL_STYLE);
    let ex_style = GetWindowLongW(hwnd, GWL_EXSTYLE);
    
    // Get Window Text (Title)
    let mut title_buf = [0u16; 512];
    let t_len = GetWindowTextW(hwnd, &mut title_buf);
    let title = String::from_utf16_lossy(&title_buf[..t_len as usize]);

    tracing::info!(
        "[{}] HWND: {:?}, Title: '{}', Size: {}x{}, Pos: ({},{}), Style: {:X}, ExStyle: {:X}",
        name, hwnd, title, width, height, rect.left, rect.top, style, ex_style
    );
}

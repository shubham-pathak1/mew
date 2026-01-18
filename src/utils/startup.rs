use windows::core::HSTRING;
use windows::Win32::System::Registry::*;
use anyhow::Result;

#[allow(dead_code)]
pub fn set_launch_on_startup(enable: bool) -> Result<()> {
    let mut hkey = HKEY::default();
    let sub_key = windows::core::w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
    let name = windows::core::w!("Mew");

    unsafe {
        if enable {
            RegOpenKeyExW(HKEY_CURRENT_USER, sub_key, 0, KEY_WRITE, &mut hkey).ok()?;
            let exe_path = std::env::current_exe()?;
            let path_str = HSTRING::from(exe_path.to_string_lossy().as_ref());
            let data = std::slice::from_raw_parts(path_str.as_ptr() as *const u8, path_str.len() as usize * 2);
            RegSetValueExW(hkey, name, 0, REG_SZ, Some(data)).ok()?;
            RegCloseKey(hkey).ok()?;
        } else {
            RegOpenKeyExW(HKEY_CURRENT_USER, sub_key, 0, KEY_WRITE, &mut hkey).ok()?;
            let _ = RegDeleteValueW(hkey, name);
            RegCloseKey(hkey).ok()?;
        }
    }
    Ok(())
}

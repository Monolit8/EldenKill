//! Where the mod's files live: next to eldenkill.dll (the mod folder). eldenkill.ini there holds
//! the settings (er-mario's paths.rs, same idea).

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use windows::Win32::Foundation::HMODULE;
use windows::Win32::System::LibraryLoader::{
    GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS, GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT, GetModuleFileNameW, GetModuleHandleExW,
};
use windows::core::PCWSTR;

pub const CONFIG: &str = "eldenkill.ini";

/// The folder this module (the DLL, or the fake host's exe) was loaded from.
pub fn mod_dir() -> &'static Path {
    static DIR: OnceLock<PathBuf> = OnceLock::new();
    DIR.get_or_init(|| {
        let mut module = HMODULE::default();
        let mut buf = [0u16; 1024];
        let len = unsafe {
            let anchor = mod_dir as *const () as *const u16;
            let flags = GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT;
            if GetModuleHandleExW(flags, PCWSTR(anchor), &mut module).is_err() {
                return PathBuf::from(".");
            }
            GetModuleFileNameW(Some(module), &mut buf) as usize
        };
        let dll = PathBuf::from(String::from_utf16_lossy(&buf[..len]));
        dll.parent().map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from("."))
    })
}

pub fn file(name: &str) -> PathBuf {
    mod_dir().join(name)
}

/// `key = value` from eldenkill.ini (lines starting with # or ; are comments).
pub fn config(key: &str) -> Option<String> {
    let text = std::fs::read_to_string(file(CONFIG)).ok()?;
    text.lines().find_map(|line| {
        let line = line.trim();
        if line.starts_with('#') || line.starts_with(';') {
            return None;
        }
        let (k, v) = line.split_once('=')?;
        (k.trim().eq_ignore_ascii_case(key)).then(|| v.trim().trim_matches('"').to_string())
    })
}

pub fn config_f32(key: &str, default: f32) -> f32 {
    config(key).and_then(|v| v.parse().ok()).filter(|v: &f32| v.is_finite()).unwrap_or(default)
}

pub fn config_bool(key: &str, default: bool) -> bool {
    config(key).map(|v| matches!(v.to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on")).unwrap_or(default)
}

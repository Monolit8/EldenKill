//! EldenKill, the Elden Ring half: V1 from ULTRAKILL in the Lands Between.
//!
//! Elden Ring is the host world (Killcraft's ULTRAKILL role) and ULTRAKILL, started hidden, runs
//! V1 (Killcraft's Minecraft role). See docs/DESIGN.md.

pub mod er;
pub mod link;
pub mod paths;
pub mod proto;

use std::io::Write;

/// eldenkill.log next to the DLL (started fresh every launch, the last one kept as .prev), and
/// stderr (the fake host's console).
pub fn log(msg: impl AsRef<str>) {
    static FRESH: std::sync::Once = std::sync::Once::new();
    let path = paths::file("eldenkill.log");
    FRESH.call_once(|| {
        let _ = std::fs::rename(&path, paths::file("eldenkill.prev.log"));
    });
    eprintln!("{}", msg.as_ref());
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(f, "{}", msg.as_ref());
    }
}

#[unsafe(no_mangle)]
/// # Safety
/// Called by the Windows loader only (me3 loads eldenkill.dll into Elden Ring).
pub unsafe extern "C" fn DllMain(hmodule: usize, reason: u32) -> bool {
    if reason == 1 {
        er::start(hmodule);
    }
    true
}

//! Starts ULTRAKILL (hidden, with the EldenKill plugin in BepInEx) when Elden Ring starts, the way
//! Killcraft starts SkyCraft's Minecraft. `ultrakill = <path to ULTRAKILL.exe>` in eldenkill.ini;
//! without it the usual places are tried.

use std::path::PathBuf;

use crate::link::Link;
use crate::log;

fn find() -> Option<PathBuf> {
    if let Some(p) = crate::paths::config("ultrakill").filter(|p| !p.is_empty()) {
        let p = PathBuf::from(p);
        let p = if p.is_dir() { p.join("ULTRAKILL.exe") } else { p };
        return p.is_file().then_some(p);
    }
    let home = std::env::var("USERPROFILE").unwrap_or_default();
    [
        format!("{home}\\Desktop\\ULTRAKILL\\ULTRAKILL.exe"),
        "C:\\Program Files (x86)\\Steam\\steamapps\\common\\ULTRAKILL\\ULTRAKILL.exe".to_string(),
        "D:\\SteamLibrary\\steamapps\\common\\ULTRAKILL\\ULTRAKILL.exe".to_string(),
    ]
    .into_iter()
    .map(PathBuf::from)
    .find(|p| p.is_file())
}

pub fn start() {
    if !crate::paths::config_bool("start_ultrakill", true) {
        log("launcher: start_ultrakill = 0; start ULTRAKILL yourself");
        return;
    }
    if Link::guest_running() {
        log("launcher: ULTRAKILL is already running");
        return;
    }
    let Some(exe) = find() else {
        log("launcher: ULTRAKILL.exe not found; set `ultrakill = C:\\path\\to\\ULTRAKILL.exe` in eldenkill.ini");
        super::overlay::set_status(Some("EldenKill: ULTRAKILL.exe not found (set ultrakill = ... in eldenkill.ini)".into()));
        return;
    };
    if !exe.parent().is_some_and(|d| d.join("BepInEx").join("plugins").join("EldenKill").join("EldenKill.Guest.dll").is_file()) {
        log(format!("launcher: {} has no BepInEx\\plugins\\EldenKill\\EldenKill.Guest.dll; install the ULTRAKILL half first", exe.display()));
    }
    let hidden = !crate::paths::config_bool("show_ultrakill", false);
    let mut args = vec!["-screen-fullscreen".to_string(), "0".into(), "-screen-width".into(), "1280".into(), "-screen-height".into(), "720".into()];
    if hidden {
        args.push("-eldenkill-hidden".into());
    }
    match std::process::Command::new(&exe).args(&args).current_dir(exe.parent().unwrap()).spawn() {
        Ok(child) => log(format!("launcher: started {} (pid {}){}", exe.display(), child.id(), if hidden { ", hidden" } else { "" })),
        Err(e) => log(format!("launcher: couldn't start {}: {e}", exe.display())),
    }
}

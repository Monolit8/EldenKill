//! The in-game debug tool:
//!  - F8: a panel over Elden Ring with V1's state, who has the player, collision and the last
//!    inputs sent to ULTRAKILL;
//!  - a launch detector: V1 suddenly very fast -> logged with the keys held and the weapon;
//!  - F7: unstick (V1 back to the last spot it stood on);
//!  - F6: a snapshot of all of it into a text file next to the DLL;
//!  - F5: kill (the Tarnished dies, and V1 with him: tests death and respawn).

use std::collections::VecDeque;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use crate::proto::*;

pub static SHOW: AtomicBool = AtomicBool::new(false);

/// Isolation switches (numpad; those keys don't reach ULTRAKILL), one mechanic at a time:
/// Num1 collision drawn, Num2 enemy hitboxes drawn, Num3 the Tarnished shown, Num4 Elden Ring's own
/// camera, Num5 god mode (V1 takes no damage).
pub static SHOW_COLLISION: AtomicBool = AtomicBool::new(false);
pub static SHOW_HITBOXES: AtomicBool = AtomicBool::new(false);
pub static SHOW_TARNISHED: AtomicBool = AtomicBool::new(false);
pub static NO_CAMERA: AtomicBool = AtomicBool::new(false);
pub static GOD: AtomicBool = AtomicBool::new(false);

pub fn switches() -> [(&'static str, &'static AtomicBool); 5] {
    [
        ("Num1 collision drawn", &SHOW_COLLISION),
        ("Num2 enemy hitboxes drawn", &SHOW_HITBOXES),
        ("Num3 Tarnished shown", &SHOW_TARNISHED),
        ("Num4 Elden Ring camera", &NO_CAMERA),
        ("Num5 god mode", &GOD),
    ]
}
static PANEL: Mutex<Vec<String>> = Mutex::new(Vec::new());
static INPUTS: Mutex<VecDeque<String>> = Mutex::new(VecDeque::new());
static EVENTS: Mutex<VecDeque<String>> = Mutex::new(VecDeque::new());
/// keys and buttons held right now (as sent to V1)
static HELD: Mutex<Vec<String>> = Mutex::new(Vec::new());
static MOUSE: Mutex<(i64, i64, u32)> = Mutex::new((0, 0, 0));
static START: Mutex<Option<Instant>> = Mutex::new(None);

fn t() -> f32 {
    START.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(Instant::now).elapsed().as_secs_f32()
}

pub fn key_name(vk: u16) -> String {
    match vk {
        0x30..=0x39 | 0x41..=0x5A => (vk as u8 as char).to_string(),
        0x70..=0x7B => format!("F{}", vk - 0x6F),
        0x08 => "Backspace".into(),
        0x09 => "Tab".into(),
        0x0D => "Enter".into(),
        0x10 | 0xA0 | 0xA1 => "Shift".into(),
        0x11 | 0xA2 | 0xA3 => "Ctrl".into(),
        0x12 | 0xA4 | 0xA5 => "Alt".into(),
        0x14 => "CapsLock".into(),
        0x1B => "Esc".into(),
        0x20 => "Space".into(),
        0x25 => "Left".into(),
        0x26 => "Up".into(),
        0x27 => "Right".into(),
        0x28 => "Down".into(),
        _ => format!("vk{vk:#04x}"),
    }
}

fn button_name(b: u16) -> &'static str {
    ["LMB", "RMB", "MMB", "Mouse4", "Mouse5"].get(b as usize).copied().unwrap_or("Mouse?")
}

pub fn weapon_name(slot: u32) -> &'static str {
    match slot {
        1 => "revolver",
        2 => "shotgun",
        3 => "nailgun",
        4 => "railcannon",
        5 => "rocket launcher",
        6 => "spawner arm",
        _ => "?",
    }
}

fn push(list: &Mutex<VecDeque<String>>, max: usize, line: String) {
    let mut l = list.lock().unwrap_or_else(|e| e.into_inner());
    l.push_back(line);
    while l.len() > max {
        l.pop_front();
    }
}

/// Every input event sent to ULTRAKILL passes through here.
pub fn note_input(kind: u16, code: u16, a: i32, b: i32) {
    let (name, down) = match kind {
        IN_KEY => (key_name(code), a != 0),
        IN_MOUSE_BUTTON => (button_name(code).to_string(), a != 0),
        IN_MOUSE_MOVE => {
            let mut m = MOUSE.lock().unwrap_or_else(|e| e.into_inner());
            m.0 += a as i64;
            m.1 += b as i64;
            m.2 += 1;
            return;
        }
        IN_SCROLL => {
            push(&INPUTS, 16, format!("{:7.2}s  scroll {a}", t()));
            return;
        }
        IN_RELEASE => {
            HELD.lock().unwrap_or_else(|e| e.into_inner()).clear();
            push(&INPUTS, 16, format!("{:7.2}s  (release all)", t()));
            return;
        }
        _ => return,
    };
    {
        let mut held = HELD.lock().unwrap_or_else(|e| e.into_inner());
        held.retain(|h| *h != name);
        if down {
            held.push(name.clone());
        }
    }
    push(&INPUTS, 16, format!("{:7.2}s  {name} {}", t(), if down { "down" } else { "up" }));
}

pub fn held() -> String {
    let h = HELD.lock().unwrap_or_else(|e| e.into_inner());
    if h.is_empty() { "nothing".into() } else { h.join("+") }
}

/// Something worth remembering (launches, teleports, unsticks, deaths): also goes to the log.
pub fn note_event(text: impl AsRef<str>) {
    let line = format!("{:7.2}s  {}", t(), text.as_ref());
    crate::log(format!("debug: {}", text.as_ref()));
    push(&EVENTS, 14, line);
}

pub fn set_panel(lines: Vec<String>) {
    *PANEL.lock().unwrap_or_else(|e| e.into_inner()) = lines;
}

/// Launch detector state (per frame from the frame task).
pub struct Launch {
    last: Option<Instant>,
}

impl Launch {
    pub const fn new() -> Self {
        Launch { last: None }
    }

    /// `vel` in ULTRAKILL units/s (2 per metre by default).
    pub fn check(&mut self, vel: [f32; 3], g_flags: u32, weapon: u32, at: glam::Vec3) {
        let speed = (vel[0] * vel[0] + vel[1] * vel[1] + vel[2] * vel[2]).sqrt();
        // a dash is ~50, falling tops out near 100: well over either, or straight up fast
        let launched = speed > 120.0 || vel[1] > 45.0;
        if !launched || self.last.is_some_and(|l| l.elapsed().as_secs_f32() < 1.5) {
            return;
        }
        self.last = Some(Instant::now());
        note_event(format!(
            "LAUNCH: speed {speed:.0} (up {:.0}) holding {} with {} [{}] at ({:.1}, {:.1}, {:.1})",
            vel[1],
            held(),
            weapon_name(weapon),
            flags_text(g_flags),
            at.x,
            at.y,
            at.z
        ));
    }
}

pub fn flags_text(f: u32) -> String {
    let mut s = Vec::new();
    for (bit, name) in [
        (GUEST_IN_LEVEL, "level"),
        (GUEST_ON_GROUND, "ground"),
        (GUEST_DEAD, "DEAD"),
        (GUEST_SLIDING, "slide"),
        (GUEST_DASHING, "dash"),
        (GUEST_JUMPING, "jump"),
        (GUEST_DRIVING, "driving"),
        (GUEST_MENU, "uk-menu"),
    ] {
        if f & bit != 0 {
            s.push(name);
        }
    }
    s.join(" ")
}

/// Mouse movement since the last call (counts, events).
pub fn take_mouse() -> (i64, i64, u32) {
    std::mem::take(&mut *MOUSE.lock().unwrap_or_else(|e| e.into_inner()))
}

/// Writes the panel, inputs and events to debug-<seconds>.txt next to the DLL.
pub fn snapshot(bodies: Vec<String>) -> String {
    let mut out = String::from("EldenKill debug snapshot\n\n== state ==\n");
    for l in PANEL.lock().unwrap_or_else(|e| e.into_inner()).iter() {
        out += l;
        out.push('\n');
    }
    out += "\n== events ==\n";
    for l in EVENTS.lock().unwrap_or_else(|e| e.into_inner()).iter() {
        out += l;
        out.push('\n');
    }
    out += "\n== last inputs ==\n";
    for l in INPUTS.lock().unwrap_or_else(|e| e.into_inner()).iter() {
        out += l;
        out.push('\n');
    }
    out += "\n== collision bodies within 4 m (layer, class, triangles, used, box) ==\n";
    for l in bodies {
        out += &l;
        out.push('\n');
    }
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let path = crate::paths::file(&format!("debug-{secs}.txt"));
    let _ = std::fs::write(&path, out);
    path.display().to_string()
}

/// Draws the panel (overlay thread).
pub fn draw(ui: &hudhook::imgui::Ui, at: [f32; 2]) {
    if !SHOW.load(Ordering::Relaxed) {
        return;
    }
    use hudhook::imgui::Condition;
    let panel = PANEL.lock().unwrap_or_else(|e| e.into_inner()).clone();
    let events: Vec<String> = EVENTS.lock().unwrap_or_else(|e| e.into_inner()).iter().cloned().collect();
    let inputs: Vec<String> = INPUTS.lock().unwrap_or_else(|e| e.into_inner()).iter().rev().take(8).cloned().collect();
    ui.window("EldenKill debug  (F8 hide, F7 unstick, F6 snapshot, F5 kill)")
        .position([at[0] + 12.0, at[1] + 40.0], Condition::Always)
        .always_auto_resize(true)
        .bg_alpha(0.72)
        .collapsible(false)
        .movable(false)
        .no_inputs()
        .build(|| {
            for l in &panel {
                if let Some(head) = l.strip_prefix("# ") {
                    ui.text_colored([1.0, 0.8, 0.3, 1.0], head);
                } else {
                    ui.text(l);
                }
            }
            ui.separator();
            ui.text_colored([1.0, 0.8, 0.3, 1.0], "isolation switches");
            for (name, flag) in switches() {
                if flag.load(Ordering::Relaxed) {
                    ui.text_colored([0.4, 1.0, 0.5, 1.0], format!("  ON   {name}"));
                } else {
                    ui.text(format!("  off  {name}"));
                }
            }
            ui.separator();
            ui.text_colored([1.0, 0.8, 0.3, 1.0], "events");
            if events.is_empty() {
                ui.text("  (none yet)");
            }
            for l in events.iter().rev().take(8) {
                if l.contains("LAUNCH") || l.contains("fell") {
                    ui.text_colored([1.0, 0.45, 0.4, 1.0], l);
                } else {
                    ui.text(l);
                }
            }
            ui.separator();
            ui.text_colored([1.0, 0.8, 0.3, 1.0], format!("last inputs to V1 (holding {})", held()));
            for l in &inputs {
                ui.text(l);
            }
        });
}

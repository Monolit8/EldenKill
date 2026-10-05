//! Elden Ring's mouse look and mouse buttons, when DirectInput doesn't carry them.
//!
//! Elden Ring's mouse manager reads the cursor (GetCursorPos) and puts it back (SetCursorPos)
//! rather than DirectInput's mouse. So the game's import of GetCursorPos is swapped (er-mario's
//! import-table patch, used there for XInput): while V1 has the mouse, the cursor's offset from the
//! window centre goes to ULTRAKILL, the real cursor goes back to the centre, and the game is told
//! it never moved. Skipped while DirectInput already delivers mouse moves (no double counting).
//! Buttons are polled while DirectInput delivers none.

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

use windows::Win32::Foundation::{POINT, RECT};
use windows::core::BOOL;
use windows::Win32::Graphics::Gdi::ClientToScreen;
use windows::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress};
use windows::Win32::System::Memory::{PAGE_PROTECTION_FLAGS, PAGE_READWRITE, VirtualProtect};
use windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
use windows::Win32::UI::WindowsAndMessaging::{GetClientRect, GetForegroundWindow};
use windows::core::{s, w};

use super::input::CAPTURE;
use crate::log;
use crate::proto::*;

static DI_MOUSE_MS: AtomicU64 = AtomicU64::new(0);
static DI_BUTTONS_MS: AtomicU64 = AtomicU64::new(0);
static SET_CURSOR_ORIGINAL: AtomicUsize = AtomicUsize::new(0);
static POLLED: std::sync::Mutex<[bool; 5]> = std::sync::Mutex::new([false; 5]);
static DI_KEYS_MS: AtomicU64 = AtomicU64::new(0);
static POLLED_KEYS: std::sync::Mutex<[bool; 256]> = std::sync::Mutex::new([false; 256]);
/// pixels of mouse look sent to ULTRAKILL (the status line shows it)
pub static MOUSE_SENT: AtomicU64 = AtomicU64::new(0);
pub static KEY_POLLING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// DirectInput delivered the keyboard just now (input.rs).
pub fn note_di_keyboard() {
    DI_KEYS_MS.store(now_ms(), Ordering::Relaxed);
}

/// Keys V1 can use (virtual-key codes): letters, digits, F-keys, space, shift, ctrl, alt, tab,
/// enter, arrows, punctuation.
fn polled_vk(vk: usize) -> bool {
    matches!(vk, 0x08 | 0x09 | 0x0D | 0x10 | 0x11 | 0x12 | 0x14 | 0x1B | 0x20..=0x28 | 0x30..=0x39 | 0x41..=0x5A | 0x71..=0x73 | 0x79..=0x7B | 0xA0..=0xA5 | 0xBA..=0xC0 | 0xDB..=0xDE)
}

/// The keyboard by polling, while DirectInput delivers none (seen in game: the hook never got a
/// keyboard call, so V1 couldn't move). Every frame while V1 has the keyboard.
pub fn poll_keys() {
    let mut polled = POLLED_KEYS.lock().unwrap_or_else(|e| e.into_inner());
    let di = now_ms().saturating_sub(DI_KEYS_MS.load(Ordering::Relaxed)) < 2000;
    if !CAPTURE.load(Ordering::Relaxed) || di {
        if polled.iter().any(|&d| d) {
            for vk in 0..256 {
                if polled[vk] {
                    send(IN_KEY, vk as u16, 0, 0);
                }
            }
            *polled = [false; 256];
        }
        KEY_POLLING.store(false, Ordering::Relaxed);
        return;
    }
    KEY_POLLING.store(true, Ordering::Relaxed);
    // F6-F9 are EldenKill's (snapshot, unstick, debug panel, V1 on/off); Elden Ring's own keys
    // (Esc, E) aren't V1's either
    for vk in 0..256usize {
        if !polled_vk(vk) || vk == 0x78 || vk == 0x1B || vk == 0x45 {
            continue;
        }
        let down = unsafe { GetAsyncKeyState(vk as i32) } as u16 & 0x8000 != 0;
        if down != polled[vk] {
            polled[vk] = down;
            send(IN_KEY, vk as u16, down as i32, 0);
        }
    }
}

fn now_ms() -> u64 {
    crate::link::now_ms()
}

/// DirectInput delivered a mouse move just now (input.rs).
pub fn note_di_mouse_move() {
    DI_MOUSE_MS.store(now_ms(), Ordering::Relaxed);
}

/// DirectInput delivered a mouse button just now (input.rs).
pub fn note_di_buttons() {
    DI_BUTTONS_MS.store(now_ms(), Ordering::Relaxed);
}

fn di_mouse_recent() -> bool {
    now_ms().saturating_sub(DI_MOUSE_MS.load(Ordering::Relaxed)) < 1000
}

fn send(kind: u16, code: u16, a: i32, b: i32) {
    super::debug::note_input(kind, code, a, b);
    if let Some(link) = super::link() {
        link.push_input(kind, code, a, b, 0);
    }
}

/// The game window's client centre in screen pixels.
fn window_centre() -> Option<POINT> {
    let hwnd = unsafe { GetForegroundWindow() };
    let mut r = RECT::default();
    unsafe { GetClientRect(hwnd, &mut r) }.ok()?;
    let mut p = POINT { x: (r.right - r.left) / 2, y: (r.bottom - r.top) / 2 };
    if !unsafe { ClientToScreen(hwnd, &mut p) }.as_bool() {
        return None;
    }
    Some(p)
}

/// Mouse look. user32's GetCursorPos itself is hooked (the game's import-table entry was, but
/// Elden Ring reads the cursor through a pointer of its own too: the mouse often did nothing for V1
/// while the game's camera turned with it and fought V1's, a wobble). While V1 has the mouse every
/// reader gets the window centre, the real offset goes to ULTRAKILL and the cursor goes back.
static INNER_ORIGINAL: AtomicUsize = AtomicUsize::new(0);

/// What user32's GetCursorPos jumps to (it's `mov edx, 1; lea r8d, [rdx+7E]; jmp [slot]`): the
/// slot is swapped for this. (Patching GetCursorPos's own 16 bytes crashed the game.)
unsafe extern "system" fn cursor_inner(out: *mut POINT, a: u32, b: u32) -> BOOL {
    let f: unsafe extern "system" fn(*mut POINT, u32, u32) -> BOOL = unsafe { std::mem::transmute(INNER_ORIGINAL.load(Ordering::Relaxed)) };
    let ok = unsafe { f(out, a, b) };
    if ok.as_bool() && !out.is_null() && CAPTURE.load(Ordering::Relaxed) && super::focused() && !di_mouse_recent() {
        if let Some(c) = window_centre() {
            let p = unsafe { &mut *out };
            let (dx, dy) = (p.x - c.x, p.y - c.y);
            if dx != 0 || dy != 0 {
                MOUSE_SENT.fetch_add((dx.abs() + dy.abs()) as u64, Ordering::Relaxed);
                if MENU_MODE.load(Ordering::Relaxed) {
                    // ULTRAKILL's menu: the movement moves the menu pointer instead of V1's view
                    let mut m = MENU_PTR.lock().unwrap_or_else(|e| e.into_inner());
                    m[0] += dx as f32;
                    m[1] += dy as f32;
                } else {
                    send(IN_MOUSE_MOVE, 0, dx, dy);
                }
                let set: unsafe extern "system" fn(i32, i32) -> BOOL = unsafe { std::mem::transmute(SET_CURSOR_ORIGINAL.load(Ordering::Relaxed)) };
                let _ = unsafe { set(c.x, c.y) };
            }
            *p = c;
        }
    }
    ok
}

/// Every frame too, in case nothing else asked for the cursor (the hook does the work).
/// ULTRAKILL's menu is open (F1): the cursor moves freely and points into it; Elden Ring is told
/// it stays in the centre.
pub static MENU_MODE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// The menu pointer (window client pixels), moved by the mouse while ULTRAKILL's menu is open
/// (Elden Ring keeps putting the real cursor back in the centre).
pub static MENU_PTR: std::sync::Mutex<[f32; 2]> = std::sync::Mutex::new([0.0, 0.0]);

/// The menu pointer, kept inside `rect` (top left, size).
pub fn menu_pointer(rect: ([f32; 2], [f32; 2])) -> [f32; 2] {
    let mut m = MENU_PTR.lock().unwrap_or_else(|e| e.into_inner());
    let (at, size) = rect;
    m[0] = m[0].clamp(at[0], at[0] + size[0]);
    m[1] = m[1].clamp(at[1], at[1] + size[1]);
    *m
}

/// Puts the menu pointer in the middle of `rect`.
pub fn centre_menu_pointer(rect: ([f32; 2], [f32; 2])) {
    *MENU_PTR.lock().unwrap_or_else(|e| e.into_inner()) = [rect.0[0] + rect.1[0] * 0.5, rect.0[1] + rect.1[1] * 0.5];
}

pub fn poll_mouse() {
    if CAPTURE.load(Ordering::Relaxed) && super::focused() {
        let mut p = POINT::default();
        let _ = unsafe { windows::Win32::UI::WindowsAndMessaging::GetCursorPos(&mut p) };
    }
}

pub unsafe fn install() {
    let set = unsafe { GetModuleHandleW(w!("user32.dll")) }.ok().and_then(|m| unsafe { GetProcAddress(m, s!("SetCursorPos")) });
    let Some(set) = set else {
        log("input: no SetCursorPos; no cursor mouse look");
        return;
    };
    SET_CURSOR_ORIGINAL.store(set as usize, Ordering::Relaxed);
    let get = unsafe { GetModuleHandleW(w!("user32.dll")) }.ok().and_then(|m| unsafe { GetProcAddress(m, s!("GetCursorPos")) });
    let Some(get) = get else {
        log("input: no GetCursorPos; no cursor mouse look");
        return;
    };
    // GetCursorPos: ba 01 00 00 00 | 44 8d 42 7e | 48 ff 25 <rel32>  (jmp [rip+rel32])
    let code = get as usize;
    let head: [u8; 12] = unsafe { std::ptr::read_unaligned(code as *const [u8; 12]) };
    // (the first 5 bytes may already be a jmp: Steam's overlay hooks GetCursorPos; it comes back to
    // +5 and so still ends in the same jmp [slot])
    if !matches!(head[0], 0xba | 0xe9) || head[5..] != [0x44, 0x8d, 0x42, 0x7e, 0x48, 0xff, 0x25] {
        log(format!("input: GetCursorPos looks different here ({head:02x?}); no cursor mouse look"));
        return;
    }
    let rel = unsafe { std::ptr::read_unaligned((code + 12) as *const i32) };
    let slot = (code as isize + 16 + rel as isize) as *mut usize;
    let mut old = PAGE_PROTECTION_FLAGS(0);
    if unsafe { VirtualProtect(slot as *const _, 8, PAGE_READWRITE, &mut old) }.is_err() {
        log("input: couldn't unprotect GetCursorPos's slot; no cursor mouse look");
        return;
    }
    INNER_ORIGINAL.store(unsafe { slot.read() }, Ordering::Relaxed);
    unsafe { slot.write(cursor_inner as *const () as usize) };
    let mut back = PAGE_PROTECTION_FLAGS(0);
    let _ = unsafe { VirtualProtect(slot as *const _, 8, old, &mut back) };
    log("input: hooked GetCursorPos (mouse look)");
}

pub fn poll_buttons() {
    let mut polled = POLLED.lock().unwrap_or_else(|e| e.into_inner());
    if !CAPTURE.load(Ordering::Relaxed) || now_ms().saturating_sub(DI_BUTTONS_MS.load(Ordering::Relaxed)) < 5000 {
        *polled = [false; 5];
        return;
    }
    const VKS: [i32; 5] = [0x01, 0x02, 0x04, 0x05, 0x06];
    for (b, vk) in VKS.iter().enumerate() {
        let down = unsafe { GetAsyncKeyState(*vk) } as u16 & 0x8000 != 0;
        if down != polled[b] {
            polled[b] = down;
            send(IN_MOUSE_BUTTON, b as u16, down as i32, 0);
        }
    }
}

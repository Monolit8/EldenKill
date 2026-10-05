//! Keyboard and mouse for V1. Elden Ring reads both through DirectInput; hooks on the device
//! methods (er-mario's kbd.rs, extended to the mouse) see every key and mouse move first, send
//! them to ULTRAKILL over the input ring, and hand Elden Ring a released keyboard and a still
//! mouse, so the Tarnished doesn't also walk, roll or swing, and Elden Ring's camera stays put.
//! A few keys stay Elden Ring's (Esc for its menu, E to interact: `er_keys` in eldenkill.ini).

use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use windows::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress, LoadLibraryW};
use windows::Win32::UI::Input::KeyboardAndMouse::{MAPVK_VSC_TO_VK_EX, MapVirtualKeyW};
use windows::core::{GUID, s, w};

use crate::log;
use crate::proto::*;

/// V1 takes the keyboard and mouse now (set every frame by the frame task).
pub static CAPTURE: AtomicBool = AtomicBool::new(false);

const IID_IDIRECTINPUT8W: GUID = GUID::from_u128(0xBF798031_483A_4DA2_AA99_5D64ED369700);
const GUID_SYSKEYBOARD: GUID = GUID::from_u128(0x6F1D2B61_D5A0_11CF_BFC7_444553540000);
const DI8DEVTYPE_MOUSE: u32 = 0x12;
const DI8DEVTYPE_KEYBOARD: u32 = 0x13;
/// DirectInput's offsets in DIMOUSESTATE / buffered mouse data
const DIMOFS_X: u32 = 0;
const DIMOFS_Y: u32 = 4;
const DIMOFS_Z: u32 = 8;
const DIMOFS_BUTTON0: u32 = 12;

type DirectInput8Create = unsafe extern "system" fn(*mut c_void, u32, *const GUID, *mut *mut c_void, *mut c_void) -> i32;
type CreateDevice = unsafe extern "system" fn(*mut c_void, *const GUID, *mut *mut c_void, *mut c_void) -> i32;
type Release = unsafe extern "system" fn(*mut c_void) -> u32;
type GetDeviceInfo = unsafe extern "system" fn(*mut c_void, *mut u8) -> i32;

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Keyboard,
    Mouse,
    Other,
}

struct State {
    kinds: HashMap<usize, Kind>,
    get_info: usize,
    /// last keyboard seen (scancode -> down), to send changes only
    keys: [bool; 256],
    buttons: [bool; 8],
    /// Elden Ring keeps these (scancodes)
    er_keys: Vec<u32>,
    /// V1 had input last time (a release goes out when it stops)
    had: bool,
}

static STATE: Mutex<Option<State>> = Mutex::new(None);

fn with<R>(f: impl FnOnce(&mut State) -> R) -> Option<R> {
    STATE.lock().unwrap_or_else(|e| e.into_inner()).as_mut().map(f)
}

fn kind_of(st: &mut State, device: usize) -> Kind {
    let get_info = st.get_info;
    *st.kinds.entry(device).or_insert_with(|| {
        if get_info == 0 {
            return Kind::Other;
        }
        let f: GetDeviceInfo = unsafe { std::mem::transmute(get_info) };
        // DIDEVICEINSTANCEW: dwSize, guidInstance, guidProduct, dwDevType, ...
        let mut buf = [0u8; 0x400];
        let size = 4 + 16 + 16 + 4 + 260 * 2 * 2 + 16 + 16 + 4 + 2;
        buf[..4].copy_from_slice(&(size as u32).to_le_bytes());
        if unsafe { f(device as *mut c_void, buf.as_mut_ptr()) } < 0 {
            return Kind::Other;
        }
        match u32::from_le_bytes(buf[36..40].try_into().unwrap()) & 0xFF {
            DI8DEVTYPE_KEYBOARD => Kind::Keyboard,
            DI8DEVTYPE_MOUSE => Kind::Mouse,
            _ => Kind::Other,
        }
    })
}

/// DirectInput scancode (DIK_*) -> Windows virtual key.
fn scancode_to_vk(sc: u32) -> u16 {
    // DIK codes >= 0x80 are the E0-prefixed (extended) keys
    let code = if sc & 0x80 != 0 { 0xE000 | (sc & 0x7F) } else { sc };
    unsafe { MapVirtualKeyW(code, MAPVK_VSC_TO_VK_EX) as u16 }
}

fn send(kind: u16, code: u16, a: i32, b: i32) {
    super::debug::note_input(kind, code, a, b);
    if let Some(link) = super::link() {
        link.push_input(kind, code, a, b, 0);
    }
}

/// Called by the frame task when V1 stops taking input (menu, cutscene, V1 off): everything up.
pub fn release_all() {
    let had = with(|st| {
        let had = st.had;
        st.had = false;
        st.keys = [false; 256];
        st.buttons = [false; 8];
        had
    })
    .unwrap_or(false);
    if had {
        send(IN_RELEASE, 0, 0, 0);
    }
}

/// DirectInput calls seen (keyboard state, keyboard buffered, mouse state, mouse buffered, other)
pub static CALLS: [std::sync::atomic::AtomicU32; 5] = [const { std::sync::atomic::AtomicU32::new(0) }; 5];

fn keyboard_state(data: &mut [u8; 256]) {
    super::cursor::note_di_keyboard();
    let capture = CAPTURE.load(Ordering::Relaxed);
    with(|st| {
        if !capture {
            return;
        }
        st.had = true;
        for sc in 0..256u32 {
            let down = data[sc as usize] & 0x80 != 0;
            if down != st.keys[sc as usize] {
                st.keys[sc as usize] = down;
                let vk = scancode_to_vk(sc);
                if vk != 0 {
                    send(IN_KEY, vk, down as i32, 0);
                }
            }
            if !st.er_keys.contains(&sc) {
                data[sc as usize] = 0;
            }
        }
    });
}

fn mouse_state(data: *mut u8, size: u32) {
    if !CAPTURE.load(Ordering::Relaxed) || size < 16 {
        return;
    }
    let read = |o: usize| unsafe { (data.add(o) as *const i32).read_unaligned() };
    let (dx, dy, dz) = (read(0), read(4), read(8));
    if dx != 0 || dy != 0 {
        super::cursor::note_di_mouse_move();
        send(IN_MOUSE_MOVE, 0, dx, dy);
    }
    if dz != 0 {
        send(IN_SCROLL, 0, dz, 0);
    }
    let buttons = if size >= 20 { 8 } else { 4 };
    with(|st| {
        st.had = true;
        for b in 0..buttons {
            let down = unsafe { *data.add(12 + b) } & 0x80 != 0;
            if down {
                super::cursor::note_di_buttons();
            }
            if down != st.buttons[b] {
                st.buttons[b] = down;
                if b < 5 {
                    send(IN_MOUSE_BUTTON, b as u16, down as i32, 0);
                }
            }
        }
    });
    unsafe { std::ptr::write_bytes(data, 0, size as usize) };
}

/// Buffered events (DIDEVICEOBJECTDATA: dwOfs, dwData, ...): forwarded, then neutralised.
fn buffered(kind: Kind, data: *mut u8, obj: u32, count: u32) {
    if !CAPTURE.load(Ordering::Relaxed) {
        return;
    }
    CALLS[match kind { Kind::Keyboard => 1, Kind::Mouse => 3, Kind::Other => 4 }].fetch_add(1, Ordering::Relaxed);
    if kind == Kind::Keyboard && count > 0 {
        super::cursor::note_di_keyboard();
    }
    for i in 0..count as usize {
        let e = unsafe { data.add(i * obj as usize) };
        let ofs = unsafe { *(e as *const u32) };
        let value = unsafe { *(e.add(4) as *const u32) };
        match kind {
            Kind::Keyboard => {
                let keep = with(|st| {
                    st.had = true;
                    let down = value & 0x80 != 0;
                    if (ofs as usize) < 256 && st.keys[ofs as usize] != down {
                        st.keys[ofs as usize] = down;
                        let vk = scancode_to_vk(ofs);
                        if vk != 0 {
                            send(IN_KEY, vk, down as i32, 0);
                        }
                    }
                    st.er_keys.contains(&ofs)
                })
                .unwrap_or(true);
                if !keep {
                    unsafe { *(e.add(4) as *mut u32) = 0 };
                }
            }
            Kind::Mouse => {
                match ofs {
                    DIMOFS_X => {
                        super::cursor::note_di_mouse_move();
                        send(IN_MOUSE_MOVE, 0, value as i32, 0)
                    }
                    DIMOFS_Y => {
                        super::cursor::note_di_mouse_move();
                        send(IN_MOUSE_MOVE, 0, 0, value as i32)
                    }
                    DIMOFS_Z => send(IN_SCROLL, 0, value as i32, 0),
                    o if (DIMOFS_BUTTON0..DIMOFS_BUTTON0 + 8).contains(&o) => {
                        let b = (o - DIMOFS_BUTTON0) as usize;
                        let down = value & 0x80 != 0;
                        super::cursor::note_di_buttons();
                        with(|st| {
                            if st.buttons[b] != down {
                                st.buttons[b] = down;
                                if b < 5 {
                                    send(IN_MOUSE_BUTTON, b as u16, down as i32, 0);
                                }
                            }
                        });
                    }
                    _ => {}
                }
                unsafe { *(e.add(4) as *mut u32) = 0 };
            }
            Kind::Other => {}
        }
    }
}

/// Elden Ring's keys from the config: names like Escape, E, Tab (DirectInput scancodes).
fn er_keys() -> Vec<u32> {
    let text = crate::paths::config("er_keys").unwrap_or_else(|| "Escape,E".into());
    text.split(',')
        .filter_map(|k| {
            let k = k.trim().to_ascii_uppercase();
            Some(match k.as_str() {
                "ESCAPE" | "ESC" => 0x01,
                "TAB" => 0x0F,
                "ENTER" | "RETURN" => 0x1C,
                "BACKSPACE" => 0x0E,
                "UP" => 0xC8,
                "DOWN" => 0xD0,
                "LEFT" => 0xCB,
                "RIGHT" => 0xCD,
                _ if k.len() == 1 => {
                    let c = k.as_bytes()[0];
                    // the QWERTY scancode rows
                    const ROWS: [(&[u8], u32); 4] = [(b"1234567890", 0x02), (b"QWERTYUIOP", 0x10), (b"ASDFGHJKL", 0x1E), (b"ZXCVBNM", 0x2C)];
                    ROWS.iter().find_map(|(row, first)| row.iter().position(|&r| r == c).map(|i| first + i as u32))?
                }
                _ => return None,
            })
        })
        .collect()
}

/// Hooks GetDeviceState and GetDeviceData of DirectInput devices (shared by all of them: the
/// methods live in dinput8.dll; the device kind decides what's done).
pub unsafe fn install_hooks() {
    use ilhook::x64::{CallbackOption, HookFlags, hook_closure_retn};
    let result = (|| -> Result<(), String> {
        let dll = unsafe { GetModuleHandleW(w!("dinput8.dll")).or_else(|_| LoadLibraryW(w!("dinput8.dll"))) }.map_err(|e| e.to_string())?;
        let create: DirectInput8Create =
            unsafe { std::mem::transmute(GetProcAddress(dll, s!("DirectInput8Create")).ok_or("no DirectInput8Create")?) };
        let hinst = unsafe { GetModuleHandleW(None) }.map_err(|e| e.to_string())?;
        let mut di: *mut c_void = std::ptr::null_mut();
        if unsafe { create(hinst.0, 0x0800, &IID_IDIRECTINPUT8W, &mut di, std::ptr::null_mut()) } < 0 || di.is_null() {
            return Err("DirectInput8Create failed".into());
        }
        let di_vtbl = unsafe { *(di as *const *const usize) };
        let create_device: CreateDevice = unsafe { std::mem::transmute(*di_vtbl.add(3)) };
        let mut dev: *mut c_void = std::ptr::null_mut();
        if unsafe { create_device(di, &GUID_SYSKEYBOARD, &mut dev, std::ptr::null_mut()) } < 0 || dev.is_null() {
            return Err("CreateDevice(keyboard) failed".into());
        }
        let vtbl = unsafe { *(dev as *const *const usize) };
        let (get_state, get_data, get_info) = unsafe { (*vtbl.add(9), *vtbl.add(10), *vtbl.add(15)) };
        *STATE.lock().unwrap_or_else(|e| e.into_inner()) =
            Some(State { kinds: HashMap::new(), get_info, keys: [false; 256], buttons: [false; 8], er_keys: er_keys(), had: false });
        unsafe {
            (std::mem::transmute::<usize, Release>(*vtbl.add(2)))(dev);
            (std::mem::transmute::<usize, Release>(*di_vtbl.add(2)))(di);
        }

        // GetDeviceState(this, size, data)
        let state_hook = |reg: *mut ilhook::x64::Registers, original: usize| -> usize {
            let (this, size, data) = unsafe { ((*reg).rcx, (*reg).rdx as u32, (*reg).r8 as *mut u8) };
            let f: unsafe extern "system" fn(u64, u32, *mut u8) -> i32 = unsafe { std::mem::transmute(original) };
            let rc = unsafe { f(this, size, data) };
            if rc >= 0 && !data.is_null() && CAPTURE.load(Ordering::Relaxed) {
                let kind = with(|st| kind_of(st, this as usize)).unwrap_or(Kind::Other);
                CALLS[match kind { Kind::Keyboard => 0, Kind::Mouse => 2, Kind::Other => 4 }].fetch_add(1, Ordering::Relaxed);
                match kind {
                    Kind::Keyboard if size == 256 => keyboard_state(unsafe { &mut *(data as *mut [u8; 256]) }),
                    Kind::Mouse => mouse_state(data, size),
                    _ => {}
                }
            }
            rc as u32 as usize
        };
        // GetDeviceData(this, object size, data, in/out count, flags)
        let data_hook = |reg: *mut ilhook::x64::Registers, original: usize| -> usize {
            let (this, obj, data, count, flags) =
                unsafe { ((*reg).rcx, (*reg).rdx as u32, (*reg).r8 as *mut u8, (*reg).r9 as *mut u32, *(((*reg).rsp + 0x28) as *const u32)) };
            let f: unsafe extern "system" fn(u64, u32, *mut u8, *mut u32, u32) -> i32 = unsafe { std::mem::transmute(original) };
            let rc = unsafe { f(this, obj, data, count, flags) };
            if rc >= 0 && !data.is_null() && !count.is_null() && obj >= 8 && CAPTURE.load(Ordering::Relaxed) {
                let kind = with(|st| kind_of(st, this as usize)).unwrap_or(Kind::Other);
                buffered(kind, data, obj, unsafe { *count });
            }
            rc as u32 as usize
        };
        let h1 = unsafe { hook_closure_retn(get_state, state_hook, CallbackOption::None, HookFlags::empty()) }.map_err(|e| format!("GetDeviceState: {e:?}"))?;
        let h2 = unsafe { hook_closure_retn(get_data, data_hook, CallbackOption::None, HookFlags::empty()) }.map_err(|e| format!("GetDeviceData: {e:?}"))?;
        std::mem::forget(h1);
        std::mem::forget(h2);
        Ok(())
    })();
    match result {
        Ok(()) => log("input: hooked Elden Ring's keyboard and mouse (DirectInput)"),
        Err(e) => log(format!("input: DirectInput hook failed ({e}); V1 gets no input")),
    }
}

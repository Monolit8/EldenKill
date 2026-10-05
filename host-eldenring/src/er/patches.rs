//! Elden Ring's frame-rate limit and its 16:9-only picture, patched in memory at start-up (the exe
//! on disk isn't touched). Both byte patterns and patches are er-patcher's
//! (https://github.com/gurrgur/er-patcher, MIT, (c) 2022 gurrgur), checked against exe 2.7.1.0:
//!  - rate: `mov [rbx+0x1c], 0x3c888889` (1/60 s) -> 1/fps;
//!  - ultrawide: the `je` that keeps the picture 16:9 -> `jmp`.
//! eldenkill.ini: `fps = 80` (uncapped or 0: no limit; 60: unchanged), `ultrawide = 1`.

use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Memory::{PAGE_EXECUTE_READWRITE, PAGE_PROTECTION_FLAGS, VirtualProtect};

use crate::log;

/// The game executable's image in memory.
fn image() -> Option<&'static mut [u8]> {
    let base = unsafe { GetModuleHandleW(None) }.ok()?.0 as usize;
    let u32_at = |a: usize| unsafe { (a as *const u32).read_unaligned() };
    let nt = base + u32_at(base + 0x3C) as usize;
    if u32_at(nt) != 0x4550 {
        return None;
    }
    // PE32+ optional header +0x38: SizeOfImage
    let size = u32_at(nt + 0x18 + 0x38) as usize;
    Some(unsafe { std::slice::from_raw_parts_mut(base as *mut u8, size) })
}

/// The image's readable sections (offsets into it), from the PE section table.
fn sections(image: &[u8]) -> Vec<(usize, usize)> {
    let u16_at = |o: usize| u16::from_le_bytes([image[o], image[o + 1]]) as usize;
    let u32_at = |o: usize| u32::from_le_bytes(image[o..o + 4].try_into().unwrap()) as usize;
    let nt = u32_at(0x3C);
    let count = u16_at(nt + 6);
    let table = nt + 0x18 + u16_at(nt + 0x14);
    (0..count)
        .map(|i| table + i * 40)
        .filter(|&s| u32_at(s + 36) & 0x4000_0000 != 0) // IMAGE_SCN_MEM_READ
        .map(|s| (u32_at(s + 12), u32_at(s + 8)))
        .filter(|&(start, len)| start + len <= image.len())
        .collect()
}

/// First match of `pattern` (None = any byte) in the readable sections, walked region by
/// region (VirtualQuery): fixed-size chunks that crossed a region boundary were skipped, and the
/// frame rate pattern sat in one of them.
fn find(image: &[u8], pattern: &[Option<u8>]) -> Option<usize> {
    use windows::Win32::System::Memory::{MEM_COMMIT, MEMORY_BASIC_INFORMATION, PAGE_GUARD, PAGE_NOACCESS, VirtualQuery};
    let first = pattern[0]?;
    let base = image.as_ptr() as usize;
    for (start, len) in sections(image) {
        let mut c = start;
        while c < start + len {
            let mut info = MEMORY_BASIC_INFORMATION::default();
            if unsafe { VirtualQuery(Some((base + c) as *const _), &mut info, size_of::<MEMORY_BASIC_INFORMATION>()) } == 0 {
                break;
            }
            let region_end = (info.BaseAddress as usize + info.RegionSize - base).min(start + len);
            let ok = info.State == MEM_COMMIT && info.Protect.0 & (PAGE_NOACCESS.0 | PAGE_GUARD.0) == 0;
            if ok {
                // (a match may run into the next region by a few bytes: allowed if that's readable too,
                // which the next VirtualQuery would show; keep it simple and stay inside)
                let hay = &image[c..region_end];
                let mut i = 0;
                while i + pattern.len() <= hay.len() {
                    if hay[i] == first && pattern.iter().enumerate().all(|(k, b)| b.is_none_or(|b| hay[i + k] == b)) {
                        return Some(c + i);
                    }
                    i += 1;
                }
            }
            c = region_end.max(c + 1);
        }
    }
    None
}

fn parse(text: &str) -> Vec<Option<u8>> {
    text.split_whitespace().map(|t| u8::from_str_radix(t, 16).ok()).collect()
}

fn write(image: &mut [u8], at: usize, bytes: &[u8]) -> bool {
    let p = image[at..].as_mut_ptr();
    let mut old = PAGE_PROTECTION_FLAGS(0);
    if unsafe { VirtualProtect(p as *const _, bytes.len(), PAGE_EXECUTE_READWRITE, &mut old) }.is_err() {
        return false;
    }
    image[at..at + bytes.len()].copy_from_slice(bytes);
    let mut back = PAGE_PROTECTION_FLAGS(0);
    let _ = unsafe { VirtualProtect(p as *const _, bytes.len(), old, &mut back) };
    true
}

pub fn apply() {
    let Some(image) = image() else {
        log("patches: no game image");
        return;
    };
    // `fps = uncapped` (or 0): a 1 ms frame limit, i.e. none in practice; 60: the game's own
    let fps = match crate::paths::config("fps").map(|v| v.trim().to_ascii_lowercase()) {
        Some(v) if v == "uncapped" || v == "0" => 1000.0,
        _ => crate::paths::config_f32("fps", 80.0),
    };
    if fps > 0.0 && (fps - 60.0).abs() > 0.5 {
        match find(image, &parse("c7 43 1c 89 88 88 3c eb 6d 89 73 18 eb c7 89 73 18")) {
            Some(at) => {
                let ok = write(image, at + 3, &(1.0f32 / fps).to_le_bytes());
                log(format!("patches: frame rate limit {fps} fps ({})", if ok { "patched" } else { "write failed" }));
            }
            None => log("patches: frame rate pattern not found; 60 fps"),
        }
    }
    if crate::paths::config_bool("ultrawide", true) {
        match find(image, &parse("74 4f 45 8b 94 cc")) {
            Some(at) => {
                let ok = write(image, at, &[0xEB]);
                log(format!("patches: ultrawide ({})", if ok { "patched" } else { "write failed" }));
            }
            None => log("patches: ultrawide pattern not found; 16:9 with black bars"),
        }
    }
}

//! The host end of the link: creates the shared memory, writes the host's blocks and reads the
//! guest's. Used by the Elden Ring DLL and by the fake host (src/bin/fake_host.rs).

use std::sync::atomic::{AtomicI32, AtomicI64, AtomicU32, AtomicU64, AtomicUsize, Ordering, fence};

use windows::Win32::Foundation::{CloseHandle, GetLastError, HANDLE, INVALID_HANDLE_VALUE};
use windows::Win32::System::Memory::{CreateFileMappingW, FILE_MAP_ALL_ACCESS, MEMORY_MAPPED_VIEW_ADDRESS, MapViewOfFile, PAGE_READWRITE};
use windows::Win32::System::SystemInformation::GetTickCount64;
use windows::Win32::System::Threading::{GetCurrentProcessId, OpenMutexW, SYNCHRONIZATION_SYNCHRONIZE};
use windows::core::HSTRING;

use crate::proto::*;

#[derive(Clone, Copy, Default, Debug)]
pub struct HostState {
    pub flags: u32,
    pub world_id: u32,
    pub epoch: u32,
    pub origin: [f64; 3],
    pub pos: [f32; 3],
    pub teleport_seq: u32,
    pub viewport: [u32; 2],
    pub units_per_metre: f32,
    pub hp: i32,
    pub hp_max: i32,
    pub yaw: f32,
    pub game_hour: f32,
}

#[derive(Clone, Copy, Default, Debug)]
pub struct GuestState {
    pub flags: u32,
    pub teleport_ack: u32,
    pub epoch_ack: u32,
    pub pos: [f32; 3],
    pub vel: [f32; 3],
    pub eye: [f32; 3],
    pub fwd: [f32; 3],
    pub up: [f32; 3],
    pub fov: f32,
    pub hp: i32,
    pub hard_damage: f32,
    pub stamina: f32,
    pub weapon: u32,
    pub style_rank: u32,
    pub frame: u32,
    pub frame_qpc: i64,
}

#[derive(Clone, Copy, Default, Debug)]
pub struct Event {
    pub kind: u32,
    pub actor: u32,
    pub a: f32,
    pub b: f32,
    pub c: f32,
    pub d: f32,
    pub flags: u32,
    pub weapon: u32,
}

#[derive(Clone, Copy, Default, Debug)]
pub struct ActorRecord {
    pub id: u32,
    pub flags: u32,
    pub pos: [f32; 3],
    pub yaw: f32,
    pub radius: f32,
    pub height: f32,
    pub hp_frac: f32,
    pub team: u16,
    pub name: [u8; 24],
}

/// A finished overlay frame (the host's front slot).
pub struct OverlayFrame<'a> {
    pub width: usize,
    pub height: usize,
    pub flags: u32,
    pub frame_id: u64,
    pub pixels: &'a [u8],
}

pub struct Link {
    base: *mut u8,
    _mapping: HANDLE,
    /// the overlay slot the host reads (only the overlay thread touches it)
    overlay_front: AtomicUsize,
}

unsafe impl Send for Link {}
unsafe impl Sync for Link {}

pub fn now_ms() -> u64 {
    unsafe { GetTickCount64() }
}

impl Link {
    /// Creates (or re-opens, after a crash of ours) the mapping and resets everything the host owns.
    pub fn create() -> Result<Link, String> {
        let size = MAPPING_BYTES as u64;
        let name = HSTRING::from(MAPPING_NAME);
        let mapping = unsafe {
            CreateFileMappingW(INVALID_HANDLE_VALUE, None, PAGE_READWRITE, (size >> 32) as u32, size as u32, &name)
        }
        .map_err(|e| format!("CreateFileMapping: {e}"))?;
        let reused = unsafe { GetLastError() }.0 == 183;
        let view: MEMORY_MAPPED_VIEW_ADDRESS = unsafe { MapViewOfFile(mapping, FILE_MAP_ALL_ACCESS, 0, 0, 0) };
        if view.Value.is_null() {
            let _ = unsafe { CloseHandle(mapping) };
            return Err("MapViewOfFile failed".into());
        }
        let link = Link { base: view.Value as *mut u8, _mapping: mapping, overlay_front: AtomicUsize::new(2) };
        // a mapping left from an earlier run (ULTRAKILL still holds it): start from a known state
        link.zero(OFF_HOST_STATE, 0x100);
        link.zero(OFF_OVERLAY_CTL, 0x100);
        link.zero(OFF_INPUT_RING, RING_DATA);
        link.zero(OFF_ACTOR_TABLE, AT_RECORDS + ACTOR_BYTES * MAX_ACTORS);
        link.zero(OFF_EVENT_RING, RING_DATA);
        link.zero(OFF_COLLISION, RING_DATA);
        link.reset_overlay();
        link.u32(OFF_HEADER + H_VERSION).store(VERSION, Ordering::Relaxed);
        link.u32(OFF_HEADER + H_HOST_PID).store(unsafe { GetCurrentProcessId() }, Ordering::Relaxed);
        link.heartbeat();
        link.u32(OFF_HEADER + H_MAGIC).store(MAGIC, Ordering::Release);
        crate::log(format!("link: shared memory {MAPPING_NAME} ({} MB, {})", size >> 20, if reused { "reused" } else { "created" }));
        Ok(link)
    }

    fn zero(&self, off: usize, len: usize) {
        unsafe { std::ptr::write_bytes(self.base.add(off), 0, len) };
    }

    fn u32(&self, off: usize) -> &AtomicU32 {
        unsafe { &*(self.base.add(off) as *const AtomicU32) }
    }

    fn i32(&self, off: usize) -> &AtomicI32 {
        unsafe { &*(self.base.add(off) as *const AtomicI32) }
    }

    fn i64(&self, off: usize) -> &AtomicI64 {
        unsafe { &*(self.base.add(off) as *const AtomicI64) }
    }

    fn u64(&self, off: usize) -> &AtomicU64 {
        unsafe { &*(self.base.add(off) as *const AtomicU64) }
    }

    fn put<T: Copy>(&self, off: usize, v: T) {
        unsafe { (self.base.add(off) as *mut T).write_unaligned(v) };
    }

    fn get<T: Copy>(&self, off: usize) -> T {
        unsafe { (self.base.add(off) as *const T).read_unaligned() }
    }

    fn get3(&self, off: usize) -> [f32; 3] {
        [self.get(off), self.get(off + 4), self.get(off + 8)]
    }

    pub fn heartbeat(&self) {
        self.u64(OFF_HEADER + H_HOST_BEAT).store(now_ms(), Ordering::Release);
    }

    pub fn guest_alive(&self) -> bool {
        let last = self.u64(OFF_HEADER + H_GUEST_BEAT).load(Ordering::Acquire);
        last != 0 && now_ms().saturating_sub(last) < 3000
    }

    pub fn guest_pid(&self) -> u32 {
        self.u32(OFF_HEADER + H_GUEST_PID).load(Ordering::Relaxed)
    }

    /// Whether a guest process is running (it holds a named mutex while it does).
    pub fn guest_running() -> bool {
        let name = HSTRING::from(GUEST_MUTEX);
        match unsafe { OpenMutexW(SYNCHRONIZATION_SYNCHRONIZE, false, &name) } {
            Ok(h) => {
                let _ = unsafe { CloseHandle(h) };
                true
            }
            Err(_) => false,
        }
    }

    pub fn write_host_state(&self, s: &HostState) {
        let o = OFF_HOST_STATE;
        let seq = self.u32(o + HS_SEQ);
        let n = seq.load(Ordering::Relaxed);
        seq.store(n.wrapping_add(1), Ordering::Relaxed);
        fence(Ordering::Release);
        self.put(o + HS_FLAGS, s.flags);
        self.put(o + HS_WORLD_ID, s.world_id);
        self.put(o + HS_EPOCH, s.epoch);
        for k in 0..3 {
            self.put(o + HS_ORIGIN + k * 8, s.origin[k]);
            self.put(o + HS_POS + k * 4, s.pos[k]);
        }
        self.put(o + HS_TELEPORT_SEQ, s.teleport_seq);
        self.put(o + HS_VIEWPORT_W, s.viewport[0]);
        self.put(o + HS_VIEWPORT_H, s.viewport[1]);
        self.put(o + HS_UNITS_PER_METRE, s.units_per_metre);
        self.put(o + HS_HP, s.hp);
        self.put(o + HS_HP_MAX, s.hp_max);
        self.put(o + HS_YAW, s.yaw);
        self.put(o + HS_GAME_HOUR, s.game_hour);
        fence(Ordering::Release);
        seq.store(n.wrapping_add(2), Ordering::Release);
    }

    /// The guest's state, or None on a torn read (keep last frame's).
    pub fn read_guest_state(&self) -> Option<GuestState> {
        let o = OFF_GUEST_STATE;
        let seq = self.u32(o + GS_SEQ);
        for _ in 0..64 {
            let s1 = seq.load(Ordering::Acquire);
            if s1 & 1 != 0 {
                std::hint::spin_loop();
                continue;
            }
            let g = GuestState {
                flags: self.get(o + GS_FLAGS),
                teleport_ack: self.get(o + GS_TELEPORT_ACK),
                epoch_ack: self.get(o + GS_EPOCH_ACK),
                pos: self.get3(o + GS_POS),
                vel: self.get3(o + GS_VEL),
                eye: self.get3(o + GS_EYE),
                fwd: self.get3(o + GS_FWD),
                up: self.get3(o + GS_UP),
                fov: self.get(o + GS_FOV),
                hp: self.get(o + GS_HP),
                hard_damage: self.get(o + GS_HARD_DAMAGE),
                stamina: self.get(o + GS_STAMINA),
                weapon: self.get(o + GS_WEAPON),
                style_rank: self.get(o + GS_STYLE_RANK),
                frame: self.get(o + GS_FRAME),
                frame_qpc: self.get(o + GS_FRAME_QPC),
            };
            fence(Ordering::Acquire);
            if seq.load(Ordering::Acquire) == s1 {
                return Some(g);
            }
        }
        None
    }

    /// One input event for the guest (dropped if the guest isn't reading).
    pub fn push_input(&self, kind: u16, code: u16, a: i32, b: i32, c: i32) {
        let ring = OFF_INPUT_RING;
        let head = self.i64(ring + RING_HEAD).load(Ordering::Relaxed);
        let tail = self.i64(ring + RING_TAIL).load(Ordering::Acquire);
        if head - tail >= INPUT_ENTRIES as i64 {
            return;
        }
        let e = ring + RING_DATA + (head as usize & (INPUT_ENTRIES - 1)) * 16;
        self.put(e, kind);
        self.put(e + 2, code);
        self.put(e + 4, a);
        self.put(e + 8, b);
        self.put(e + 12, c);
        self.i64(ring + RING_HEAD).store(head + 1, Ordering::Release);
    }

    pub fn pop_event(&self) -> Option<Event> {
        let ring = OFF_EVENT_RING;
        let head = self.i64(ring + RING_HEAD).load(Ordering::Acquire);
        let mut tail = self.i64(ring + RING_TAIL).load(Ordering::Relaxed);
        if tail >= head {
            return None;
        }
        if head - tail > EVENT_ENTRIES as i64 {
            tail = head - EVENT_ENTRIES as i64;
        }
        let p = ring + RING_DATA + (tail as usize & (EVENT_ENTRIES - 1)) * EVENT_BYTES;
        let e = Event {
            kind: self.get(p),
            actor: self.get(p + 4),
            a: self.get(p + 8),
            b: self.get(p + 12),
            c: self.get(p + 16),
            d: self.get(p + 20),
            flags: self.get(p + 24),
            weapon: self.get(p + 28),
        };
        self.i64(ring + RING_TAIL).store(tail + 1, Ordering::Release);
        Some(e)
    }

    pub fn write_actors(&self, actors: &[ActorRecord]) {
        let o = OFF_ACTOR_TABLE;
        let seq = self.u32(o + AT_SEQ);
        let n = seq.load(Ordering::Relaxed);
        seq.store(n.wrapping_add(1), Ordering::Relaxed);
        fence(Ordering::Release);
        let count = actors.len().min(MAX_ACTORS);
        self.put(o + AT_COUNT, count as i32);
        for (i, a) in actors.iter().take(count).enumerate() {
            let r = o + AT_RECORDS + i * ACTOR_BYTES;
            self.put(r, a.id);
            self.put(r + 4, a.flags);
            for k in 0..3 {
                self.put(r + 8 + k * 4, a.pos[k]);
            }
            self.put(r + 20, a.yaw);
            self.put(r + 24, a.radius);
            self.put(r + 28, a.height);
            self.put(r + 32, a.hp_frac);
            self.put(r + 36, a.team);
            self.put(r + 38, 0u16);
            unsafe { std::ptr::copy_nonoverlapping(a.name.as_ptr(), self.base.add(r + 40), 24) };
        }
        fence(Ordering::Release);
        seq.store(n.wrapping_add(2), Ordering::Release);
    }

    /// Appends one collision message. False when the ring is too full (try again next frame).
    /// Single producer: only one thread may call this.
    pub fn write_collision(&self, kind: u32, payload: &[u8]) -> bool {
        let ring = OFF_COLLISION;
        let size = COL_DATA_BYTES as i64;
        let msg = ((8 + payload.len() + 7) & !7) as i64;
        if msg > size / 2 {
            crate::log(format!("link: collision message too large ({msg} bytes)"));
            return false;
        }
        let mut head = self.i64(ring + RING_HEAD).load(Ordering::Relaxed);
        let tail = self.i64(ring + RING_TAIL).load(Ordering::Acquire);
        let mut pos = head % size;
        let pad = if pos + msg > size { size - pos } else { 0 };
        if size - (head - tail) < msg + pad {
            return false;
        }
        let data = ring + RING_DATA;
        if pad > 0 {
            self.put(data + pos as usize, COL_PAD);
            self.put(data + pos as usize + 4, 0u32);
            head += pad;
            pos = 0;
        }
        self.put(data + pos as usize, kind);
        self.put(data + pos as usize + 4, payload.len() as u32);
        unsafe { std::ptr::copy_nonoverlapping(payload.as_ptr(), self.base.add(data + pos as usize + 8), payload.len()) };
        self.i64(ring + RING_HEAD).store(head + msg, Ordering::Release);
        true
    }

    /// Bytes of collision the guest hasn't read yet.
    pub fn collision_backlog(&self) -> i64 {
        self.i64(OFF_COLLISION + RING_HEAD).load(Ordering::Acquire) - self.i64(OFF_COLLISION + RING_TAIL).load(Ordering::Acquire)
    }

    /// A new guest process: both sides start the overlay rotation over.
    pub fn reset_overlay(&self) {
        self.i32(OFF_OVERLAY_CTL + OC_STATE).store(1, Ordering::Release);
        self.overlay_front.store(2, Ordering::Release);
    }

    /// The newest finished overlay frame, if there is one we haven't taken yet. Only one thread
    /// (the overlay's) may take frames.
    pub fn take_overlay(&self) -> Option<OverlayFrame<'_>> {
        let state = self.i32(OFF_OVERLAY_CTL + OC_STATE);
        if state.load(Ordering::Acquire) & OVERLAY_DIRTY == 0 {
            return None;
        }
        let front = self.overlay_front.load(Ordering::Acquire);
        let old = state.swap(front as i32, Ordering::AcqRel);
        let front = (old & 3) as usize;
        if front >= OVERLAY_SLOTS {
            self.overlay_front.store(2, Ordering::Release);
            return None;
        }
        self.overlay_front.store(front, Ordering::Release);
        self.front_frame()
    }

    /// The frame in the front slot (the last one taken).
    pub fn front_frame(&self) -> Option<OverlayFrame<'_>> {
        let front = self.overlay_front.load(Ordering::Acquire);
        let h = OFF_OVERLAY_HDR + SLOT_HDR_BYTES * front;
        let (width, height) = (self.get::<u32>(h + SH_WIDTH) as usize, self.get::<u32>(h + SH_HEIGHT) as usize);
        if width == 0 || height == 0 || width > OVERLAY_MAX_W || height > OVERLAY_MAX_H {
            return None;
        }
        let start = OFF_OVERLAY_PIXELS + OVERLAY_SLOT_BYTES * front;
        Some(OverlayFrame {
            width,
            height,
            flags: self.get(h + SH_FLAGS),
            frame_id: self.get(h + SH_FRAME_ID),
            pixels: unsafe { std::slice::from_raw_parts(self.base.add(start), width * height * 4) },
        })
    }

    pub fn restart_overlay(&self) {
        self.reset_overlay();
    }

    /// ULTRAKILL's HUD settings, once it sent them: hudType, crossHair, crossHairColor, crossHairHud,
    /// styleMeter, hudBackgroundOpacity.
    pub fn hud_prefs(&self) -> Option<(i32, i32, i32, i32, bool, f32)> {
        let o = OFF_HUD_PREFS;
        (self.get::<u32>(o) == 1).then(|| (self.get(o + 4), self.get(o + 8), self.get(o + 12), self.get(o + 16), self.get::<i32>(o + 20) != 0, self.get(o + 24)))
    }

    /// ULTRAKILL's style feed text, when it changed since `seen` (sequence number).
    pub fn style_text(&self, seen: u32) -> Option<(u32, String)> {
        let seq: u32 = self.get(OFF_STYLE_TEXT);
        if seq == seen {
            return None;
        }
        let len = (self.get::<u32>(OFF_STYLE_TEXT + 4) as usize).min(1016);
        let bytes = unsafe { std::slice::from_raw_parts(self.base.add(OFF_STYLE_TEXT + 8), len) };
        Some((seq, String::from_utf8_lossy(bytes).into_owned()))
    }

    pub fn overlay_frames(&self) -> u64 {
        self.u64(OFF_OVERLAY_CTL + OC_FRAMES).load(Ordering::Relaxed)
    }
}

/// Collision payloads.
pub mod col {
    use super::*;

    pub fn clear(epoch: u32) -> Vec<u8> {
        epoch.to_le_bytes().to_vec()
    }

    /// A region: one body's triangles (9 floats each, guest units) and their flags.
    pub fn region(id: u64, epoch: u32, tris: &[([f32; 9], u32)]) -> Vec<u8> {
        let mut out = Vec::with_capacity(16 + tris.len() * COL_TRI_BYTES);
        out.extend_from_slice(&id.to_le_bytes());
        out.extend_from_slice(&epoch.to_le_bytes());
        out.extend_from_slice(&(tris.len() as u32).to_le_bytes());
        for (v, flags) in tris {
            for f in v {
                out.extend_from_slice(&f.to_le_bytes());
            }
            out.extend_from_slice(&flags.to_le_bytes());
        }
        out
    }

    pub fn remove(id: u64, epoch: u32) -> Vec<u8> {
        let mut out = id.to_le_bytes().to_vec();
        out.extend_from_slice(&epoch.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out
    }
}

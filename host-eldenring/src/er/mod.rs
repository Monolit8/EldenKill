//! Elden Ring's half of the link, every frame (Killcraft's Host.cs with the roles swapped):
//!  - tells ULTRAKILL where the Tarnished is, whether a menu, cutscene or load has the player,
//!    and streams the Havok collision around him;
//!  - once V1 is there, puts the Tarnished (hidden) where V1 is every frame and Elden Ring's
//!    camera at V1's eyes: ULTRAKILL is authoritative for movement, as Minecraft is in Killcraft;
//!  - Elden Ring moving the player itself (graces, warps, respawn, elevators far enough) is a
//!    teleport V1 follows;
//!  - sends the keyboard and mouse to ULTRAKILL, draws V1's view on top, and connects the fights.

mod camera;
mod combat;
mod cursor;
mod debug;
pub mod explore;
pub mod havok_col;
mod input;
mod launcher;
mod overlay;
mod patches;
mod stream;
mod version;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use eldenring::cs::{CSTaskGroupIndex, CSTaskImp, WorldChrMan};
use eldenring::fd4::FD4TaskData;
use eldenring::position::HavokPosition;
use eldenring::rotation::Quaternion;
use fromsoftware_shared::{FromStatic, SharedTaskImpExt};
use glam::Vec3;
use windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;

use crate::link::{GuestState, HostState, Link};
use crate::log;
use crate::proto::*;

static LINK: OnceLock<Link> = OnceLock::new();
static MODULE: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
/// V1 mode (F9 toggles it).
static ENABLED: AtomicBool = AtomicBool::new(true);
/// The Tarnished is hidden (V1 has him).
static HIDE_TARNISHED: AtomicBool = AtomicBool::new(false);
static HOST: Mutex<Option<Host>> = Mutex::new(None);

pub fn link() -> Option<&'static Link> {
    LINK.get()
}

/// `debug = 1` in eldenkill.ini: detailed logging.
pub fn debug() -> bool {
    static DEBUG: OnceLock<bool> = OnceLock::new();
    *DEBUG.get_or_init(|| crate::paths::config_bool("debug", false))
}

pub(crate) fn dlog(msg: impl AsRef<str>) {
    if debug() {
        log(msg);
    }
}

struct Host {
    stream: stream::Stream,
    combat: combat::Combat,
    guest: GuestState,
    guest_was_alive: bool,
    last_guest_pid: u32,
    teleport_seq: u32,
    /// where the Tarnished was put last frame (a big jump from there is Elden Ring moving him)
    last_set: Option<Vec3>,
    in_world_time: f32,
    /// seconds V1 has been waiting for its floor under a Tarnished who stands on Elden Ring ground
    realign_wait: f32,
    /// re-placings in a row without V1 getting in: after 3, only every 30 s (each one sends all the
    /// collision again)
    realign_tries: u32,
    /// a loading screen or a death since the collision was last placed: Elden Ring rebuilds its
    /// physics in a new frame then, so the collision is placed again from scratch
    reanchor: bool,
    prev_hp: i32,
    driving: bool,
    hud_hidden: bool,
    f9_was: bool,
    log_timer: f32,
    /// debug tool: F6 / F7 / F8 edges, the launch detector, the last spot V1 stood on
    fkeys_was: [bool; 4],
    /// F5: frames left of "the Tarnished is dead" (the game needs a moment to notice)
    kill_frames: u32,
    launch: debug::Launch,
    last_safe: Option<Vec3>,
    safe_timer: f32,
    mouse_window: (i64, i64, u32, f32),
    mouse_shown: (i64, i64, u32),
    /// Elden Ring had a reason to move the player since V1 last drove (cutscene, death, a load):
    /// only then does V1 follow the Tarnished. Otherwise a gap means he drifted or fell through the
    /// ground (seen in game: V1 was sent under the map after him), and he goes back to V1.
    er_moved_ok: bool,
    /// the Tarnished's origin block and position last frame (a re-base shows as the block changing)
    origin_block: i32,
    prev_p: Option<Vec3>,
    /// how long ULTRAKILL has reported another epoch than ours (it lost the collision)
    epoch_mismatch: f32,
    /// a place V1 is being sent to by EldenKill itself (F7), instead of the Tarnished's
    teleport_target: Option<Vec3>,
    /// E pressed while V1 drove: (when, the Tarnished's animation then)
    interact_at: Option<(std::time::Instant, i32)>,
    /// an animation started by E is playing (a fog gate, a waygate, a lever...): Elden Ring has
    /// the player until it's over
    interact_hold: Option<std::time::Instant>,
    e_was: bool,
    /// ULTRAKILL's own menu (options: sound, HUD, controls...) open over Elden Ring (F1)
    uk_menu: bool,
    uk_menu_since: f32,
    f1_was: bool,
    /// frames until the Escape sent for F1 is let go (down and up in one frame never registered)
    esc_up_in: u32,
    anim_same_for: f32,
}

impl Host {
    fn new() -> Self {
        Host {
            stream: stream::Stream::new(crate::paths::config_f32("units_per_metre", 2.0)),
            combat: combat::Combat::new(),
            guest: GuestState::default(),
            guest_was_alive: false,
            last_guest_pid: 0,
            teleport_seq: 1,
            last_set: None,
            in_world_time: 0.0,
            realign_wait: 0.0,
            realign_tries: 0,
            reanchor: false,
            prev_hp: 1,
            driving: false,
            hud_hidden: false,
            f9_was: false,
            log_timer: 0.0,
            fkeys_was: [false; 4],
            kill_frames: 0,
            launch: debug::Launch::new(),
            last_safe: None,
            safe_timer: 0.0,
            mouse_window: (0, 0, 0, 0.0),
            mouse_shown: (0, 0, 0),
            er_moved_ok: true,
            interact_at: None,
            interact_hold: None,
            e_was: false,
            uk_menu: false,
            uk_menu_since: 0.0,
            f1_was: false,
            esc_up_in: 0,
            anim_same_for: 0.0,
            origin_block: 0,
            prev_p: None,
            epoch_mismatch: 0.0,
            teleport_target: None,
        }
    }
}

/// Whether Elden Ring has a menu or prompt up (er-mario: the popup menu's current top menu job).
fn game_menu_open() -> bool {
    unsafe { eldenring::cs::CSMenuManImp::instance() }
        .ok()
        .and_then(|m| m.popup_menu)
        .map(|p| unsafe { *(((p.as_ptr() as usize) + 0xB0) as *const usize) })
        .unwrap_or(0)
        != 0
}

fn current_anim(chr: &eldenring::cs::ChrIns) -> i32 {
    let t = &chr.modules.time_act;
    t.anim_queue[(t.read_idx % 10) as usize].anim_id
}

/// Game-driven animations (er-mario): events 6xxxx (fog walls, doors, levers, graces...) and
/// ladders. Elden Ring has the player for those; V1 waits and follows afterwards.
fn game_driven(anim: i32) -> bool {
    (60000..70000).contains(&anim) || (28000..29000).contains(&anim) || (51100..51200).contains(&anim)
}

pub(crate) fn focused() -> bool {
    use windows::Win32::System::Threading::GetCurrentProcessId;
    use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};
    let mut pid = 0u32;
    unsafe { GetWindowThreadProcessId(GetForegroundWindow(), Some(&mut pid)) };
    pid == unsafe { GetCurrentProcessId() }
}

fn frame(data: &FD4TaskData) {
    let Some(link) = link() else { return };
    let mut guard = HOST.lock().unwrap_or_else(|e| e.into_inner());
    let h = guard.get_or_insert_with(Host::new);
    let dt = data.delta_time.time;
    link.heartbeat();

    // ULTRAKILL (re)connected: everything again from a fresh epoch
    let alive = link.guest_alive();
    let pid = link.guest_pid();
    if alive && (!h.guest_was_alive || pid != h.last_guest_pid) {
        log(format!("ULTRAKILL linked (pid {pid})"));
        h.last_guest_pid = pid;
        link.restart_overlay();
        let origin = h.stream.origin;
        h.stream.restart(origin);
        h.teleport_seq = h.teleport_seq.wrapping_add(1);
    } else if !alive && h.guest_was_alive {
        log("ULTRAKILL stopped answering");
    }
    h.guest_was_alive = alive;
    if let Some(g) = link.read_guest_state() {
        h.guest = g;
    }
    let g = h.guest;

    // F9: V1 on / off
    let f9 = focused() && unsafe { GetAsyncKeyState(0x78) } as u16 & 0x8000 != 0;
    if f9 && !h.f9_was {
        let on = !ENABLED.load(Ordering::Relaxed);
        ENABLED.store(on, Ordering::Relaxed);
        log(format!("F9: V1 {}", if on { "on" } else { "off (plain Elden Ring)" }));
        h.teleport_seq = h.teleport_seq.wrapping_add(1);
    }
    h.f9_was = f9;
    let enabled = ENABLED.load(Ordering::Relaxed);
    // debug tool keys: F6 snapshot, F7 unstick, F8 panel
    let mut fkey = [false; 4];
    for (i, vk) in [0x75, 0x76, 0x77, 0x74].into_iter().enumerate() {
        let down = focused() && unsafe { GetAsyncKeyState(vk) } as u16 & 0x8000 != 0;
        fkey[i] = down && !h.fkeys_was[i];
        h.fkeys_was[i] = down;
    }
    if fkey[2] {
        let on = !debug::SHOW.load(Ordering::Relaxed);
        debug::SHOW.store(on, Ordering::Relaxed);
    }
    // numpad isolation switches
    {
        static WAS: Mutex<[bool; 5]> = Mutex::new([false; 5]);
        let mut was = WAS.lock().unwrap_or_else(|e| e.into_inner());
        for (i, (name, flag)) in debug::switches().into_iter().enumerate() {
            let down = focused() && unsafe { GetAsyncKeyState(0x61 + i as i32) } as u16 & 0x8000 != 0;
            if down && !was[i] {
                let on = !flag.load(Ordering::Relaxed);
                flag.store(on, Ordering::Relaxed);
                debug::note_event(format!("{name}: {}", if on { "ON" } else { "off" }));
            }
            was[i] = down;
        }
    }
    if fkey[3] && enabled {
        h.kill_frames = 30;
        debug::note_event("F5 kill: the Tarnished (and V1) die");
    }
    if fkey[0] {
        let at = (unsafe { WorldChrMan::instance() }).ok().and_then(|w| w.main_player.as_ref()).map(|pl| {
            let q = &pl.chr_ins.modules.physics.position;
            Vec3::new(q.0, q.1 + 1.0, q.2)
        });
        let path = debug::snapshot(at.map(|a| h.stream.bodies_around(a, 4.0)).unwrap_or_default());
        debug::note_event(format!("snapshot written: {path}"));
    }

    // the player, if one is in the world
    let player_info = (unsafe { WorldChrMan::instance() }).ok().and_then(|w| w.main_player.as_ref()).map(|p| {
        let ph = &p.chr_ins.modules.physics;
        let q = ph.orientation;
        (
            Vec3::new(ph.position.0, ph.position.1, ph.position.2),
            glam::Quat::from_xyzw(q.0, q.1, q.2, q.3),
            p.chr_ins.modules.data.hp,
            p.chr_ins.modules.data.max_hp,
            current_anim(&p.chr_ins),
            {
                use eldenring::cs::ChrInsExt;
                p.chr_ins.block_id_origin().0
            },
        )
    });
    // position trace: last frame's write vs what the game has now (and whether it took the request)
    if let Some(player) = (unsafe { WorldChrMan::instance() }).ok().and_then(|w| w.main_player.as_ref()) {
        let ph = &player.chr_ins.modules.physics;
        let mut tr = TRACE.lock().unwrap_or_else(|e| e.into_inner());
        tr.start = [ph.position.0, ph.position.1, ph.position.2];
        tr.last_update = [ph.last_update_position.0, ph.last_update_position.1, ph.last_update_position.2];
        tr.flag_still_set = ph.chr_proxy_pos_update_requested;
    }
    let Some((p, facing, hp, max_hp, anim, origin_block)) = player_info else {
        // title screen, loading
        h.in_world_time = 0.0;
        h.reanchor = true;
        h.er_moved_ok = true;
        h.origin_block = 0;
        h.prev_p = None;
        h.last_set = None;
        h.driving = false;
        h.combat.forget_hp();
        stand_down(h, link);
        link.write_host_state(&HostState { flags: HOST_LOADING | if enabled { HOST_ENABLED } else { 0 }, epoch: h.stream.epoch, ..Default::default() });
        return;
    };
    h.in_world_time += dt;

    // Elden Ring shifts its Havok space now and then (a floating origin): every position, the
    // Tarnished's and every collision body's, moves by a whole multiple of 8 m (seen in game:
    // (-32, 0, -8), (32, -16, -16)). A static body is watched for it (stream.rs), and EldenKill's
    // whole frame moves with it (the guest's space stays put): without this V1, the Tarnished and
    // the collision came apart by the shift and V1 got launched or fell through the map.
    // A real shift moves the Tarnished too: he must have jumped by the same vector since last frame
    // (from where EldenKill put him, or where he was). Without this check, bodies that move on their
    // own were read as a shift 120 times in a row in Stormveil (+32 m up each time): the collision
    // ended up far above V1, who fell through the map.
    let shift = h.stream.detect_shift().filter(|r| {
        let Some(before) = h.last_set.or(h.prev_p) else { return false };
        let ok = (p - before - *r).length() < 3.0;
        if !ok {
            dlog(format!("Havok shift ({:.0}, {:.0}, {:.0}) ignored: the Tarnished didn't move with it", r.x, r.y, r.z));
        }
        ok
    });
    if let Some(r) = shift {
        h.stream.rebase(r, link);
        for v in [&mut h.last_set, &mut h.last_safe, &mut h.teleport_target] {
            if let Some(x) = v.as_mut() {
                *x += r;
            }
        }
        debug::note_event(format!(
            "Havok shifted by ({:.0}, {:.0}, {:.0}) (seen on a static body; origin block {:#x} -> {:#x}): followed",
            r.x, r.y, r.z, h.origin_block, origin_block
        ));
    }
    h.origin_block = origin_block;
    h.prev_p = Some(p);

    // the guest's (0, 0, 0): re-centred when the player is far from it (floats stay precise)
    if h.stream.epoch == 0 || (p - h.stream.origin).length() > 1500.0 {
        let snap = (p / 64.0).round() * 64.0;
        h.stream.restart(snap);
        h.teleport_seq = h.teleport_seq.wrapping_add(1);
        log(format!("collision: new origin {snap:.0?} (epoch {})", h.stream.epoch));
    }
    // While V1 drives, where the Tarnished is doesn't matter: he's put at V1 every frame (and lags
    // behind a fast fall: treating that gap as a teleport snapped V1 back up a cliff, again and
    // again). Elden Ring moves the player itself only while it has him (doors, ladders, lifts in
    // cutscenes, respawns): so when V1 is about to take over again and isn't where the Tarnished is,
    // V1 goes there first.
    let anim_now = anim;
    if game_driven(anim_now) || hp <= 0 {
        h.er_moved_ok = true;
    }
    if !h.driving && h.teleport_target.is_none() && h.guest.flags & GUEST_DRIVING != 0 && h.guest.teleport_ack == h.teleport_seq {
        let v1 = h.stream.to_havok(Vec3::from(h.guest.pos));
        let gap = (p - v1).length();
        if gap > 4.0 && h.in_world_time > 1.5 {
            if h.er_moved_ok {
                debug::note_event(format!("Elden Ring moved the player {gap:.1} m from V1: V1 follows"));
                h.teleport_seq = h.teleport_seq.wrapping_add(1);
            } else {
                debug::note_event(format!("the Tarnished drifted {gap:.1} m from V1 (fell?): put back at V1"));
                if let Some(player) = (unsafe { WorldChrMan::instance_mut() }).ok().and_then(|w| w.main_player.as_mut()) {
                    let ph = &mut player.chr_ins.modules.physics;
                    ph.position = HavokPosition(v1.x, v1.y + 0.1, v1.z, 0.0);
                    ph.chr_proxy_pos_update_requested = true;
                }
            }
        }
    }
    if let Some(last) = h.last_set {
        if (p - last).length() > 3.0 {
            dlog(format!("the Tarnished is {:.1} m behind V1", (p - last).length()));
        }
    }

    // ULTRAKILL lost the collision (an arena reload) and says so with another epoch: all again
    if alive && g.flags & GUEST_IN_LEVEL != 0 && g.epoch_ack != h.stream.epoch {
        h.epoch_mismatch += dt;
        if h.epoch_mismatch > 2.0 {
            h.epoch_mismatch = 0.0;
            let origin = h.stream.origin;
            h.stream.restart(origin);
            h.teleport_seq = h.teleport_seq.wrapping_add(1);
            debug::note_event(format!("ULTRAKILL lost the collision: sending it again (epoch {})", h.stream.epoch));
        }
    } else {
        h.epoch_mismatch = 0.0;
    }

    let menu = game_menu_open();
    // E, then the Tarnished starts an animation within 1.5 s: something Elden Ring plays itself
    // (walking through a fog gate, a waygate, a lever, a chest). The game-driven animation ids
    // don't cover all of them (a fog gate didn't hand over: the Tarnished was held at V1 and never
    // went through), so this catches the rest.
    {
        let e = focused() && unsafe { GetAsyncKeyState(0x45) } as u16 & 0x8000 != 0;
        if e && !h.e_was && h.driving {
            h.interact_at = Some((std::time::Instant::now(), anim));
            dlog(format!("interact: E pressed (anim {anim})"));
        }
        h.e_was = e;
        if let Some((at, before)) = h.interact_at {
            // (animations under 30000 are standing, walking and the like: 20110 -> 0 after E was
            // taken for an interaction and froze V1)
            if h.interact_hold.is_none() && anim != before && anim >= 30000 && at.elapsed().as_secs_f32() < 1.5 {
                h.interact_hold = Some(std::time::Instant::now());
                h.anim_same_for = 0.0;
                debug::note_event(format!("E: Elden Ring plays animation {anim} (was {before}): it has the player"));
            } else if h.interact_hold.is_none() && at.elapsed().as_secs_f32() >= 1.5 {
                h.interact_at = None;
            }
        }
        if let (Some(since), Some((_, before))) = (h.interact_hold, h.interact_at) {
            h.anim_same_for = if anim == before || anim < 30000 { h.anim_same_for + dt } else { 0.0 };
            if h.anim_same_for > 0.4 || since.elapsed().as_secs_f32() > 25.0 {
                debug::note_event(format!("interaction over (anim {anim}): V1 again, where the Tarnished is now"));
                // V1 goes to where the Tarnished ended up (a fog gate moves him 2-4 m: under the
                // 4 m follow threshold V1 stayed outside and pulled him back out)
                h.teleport_seq = h.teleport_seq.wrapping_add(1);
                h.interact_hold = None;
                h.interact_at = None;
            }
        }
    }
    let cutscene = game_driven(anim) || h.interact_hold.is_some();
    if h.interact_hold.is_some() {
        h.er_moved_ok = true;
    }
    let dead = hp <= 0;
    let settled = h.in_world_time > 1.5;
    let guest_ready = alive
        && g.flags & GUEST_DRIVING != 0
        && g.teleport_ack == h.teleport_seq
        && g.epoch_ack == h.stream.epoch;
    let drive = enabled && guest_ready && settled && !cutscene && !dead;
    if drive {
        h.er_moved_ok = false;
    }
    if h.teleport_target.is_some() && g.teleport_ack == h.teleport_seq {
        h.teleport_target = None;
    }
    if drive != h.driving {
        log(format!(
            "{} (guest flags {:#x}, teleport {}/{}, epoch {}/{}, cutscene {cutscene}, dead {dead})",
            if drive { "V1 has the player" } else { "Elden Ring has the player" },
            g.flags,
            g.teleport_ack,
            h.teleport_seq,
            g.epoch_ack,
            h.stream.epoch
        ));
        h.driving = drive;
        h.combat.forget_hp();
    }

    // V1 far below the last ground it stood on (fell through the world): unstick by itself
    let auto_unstick = h.driving
        && h.teleport_target.is_none()
        && g.flags & GUEST_NO_GROUND != 0
        && h.last_safe.is_some_and(|s| s.y - h.stream.to_havok(Vec3::from(g.pos)).y > 30.0);
    // three of those in 20 s: the collision itself is off (a coordinate mix-up): everything is
    // built again from scratch, around the Tarnished, in Elden Ring's space as it is now
    let mut auto_unstick = auto_unstick;
    if auto_unstick {
        static RECENT: Mutex<Vec<std::time::Instant>> = Mutex::new(Vec::new());
        let mut recent = RECENT.lock().unwrap_or_else(|e| e.into_inner());
        recent.retain(|t| t.elapsed().as_secs_f32() < 20.0);
        recent.push(std::time::Instant::now());
        if recent.len() >= 3 {
            recent.clear();
            auto_unstick = false;
            // around the last ground V1 stood on (the Tarnished fell with V1: not there)
            let home = h.last_safe.unwrap_or(p);
            let snap = (home / 64.0).round() * 64.0;
            h.stream.restart(snap);
            h.teleport_target = h.last_safe;
            if let (Some(t), Some(player)) = (h.last_safe, (unsafe { WorldChrMan::instance_mut() }).ok().and_then(|w| w.main_player.as_mut())) {
                let ph = &mut player.chr_ins.modules.physics;
                ph.position = HavokPosition(t.x, t.y + 0.1, t.z, 0.0);
                ph.chr_proxy_pos_update_requested = true;
                ph.gravity_disabled = true;
            }
            h.teleport_seq = h.teleport_seq.wrapping_add(1);
            debug::note_event(format!("V1 kept falling through: collision rebuilt from scratch (epoch {})", h.stream.epoch));
        } else {
            debug::note_event("V1 fell with no ground under it: unstuck by itself");
        }
    }
    // F7: unstick. The Tarnished goes to the last spot V1 stood on, and V1 is sent there.
    if fkey[1] || auto_unstick {
        match h.last_safe {
            Some(safe) => {
                if let Some(player) = (unsafe { WorldChrMan::instance_mut() }).ok().and_then(|w| w.main_player.as_mut()) {
                    let ph = &mut player.chr_ins.modules.physics;
                    ph.position = HavokPosition(safe.x, safe.y + 0.1, safe.z, 0.0);
                    ph.chr_proxy_pos_update_requested = true;
                    ph.gravity_disabled = true;
                }
                h.teleport_target = Some(safe);
                h.teleport_seq = h.teleport_seq.wrapping_add(1);
                debug::note_event(format!("F7 unstick: V1 back to ({:.1}, {:.1}, {:.1})", safe.x, safe.y, safe.z));
            }
            None => debug::note_event("F7 unstick: no safe spot recorded yet"),
        }
    }

    // V1 -> the Tarnished and the camera
    let v1_feet = h.stream.to_havok(Vec3::from(g.pos));
    if drive {
        if let Some(player) = (unsafe { WorldChrMan::instance_mut() }).ok().and_then(|w| w.main_player.as_mut()) {
            let ph = &mut player.chr_ins.modules.physics;
            ph.position = HavokPosition(v1_feet.x, v1_feet.y, v1_feet.z, 0.0);
            ph.chr_proxy_pos_update_requested = true;
            TRACE.lock().unwrap_or_else(|e| e.into_inner()).written = Some(v1_feet.to_array());
            // V1 owns falling: no gravity, fall timer or fall damage for the Tarnished
            ph.gravity_disabled = true;
            ph.is_falling = false;
            if g.flags & GUEST_ON_GROUND != 0 {
                // "standing", so the game allows interactions (doors, chests, graces)
                ph.is_touching_ground = true;
                ph.standing_on_solid_ground = true;
                ph.touching_solid_ground = true;
            }
            let fwd = Vec3::from(g.fwd);
            // (an Elden Ring character faces its rotation's -Z (er-mario: q * (0, 0, -1)): turned
            // half round, or the Tarnished faced away from where V1 looks, and a fog gate's
            // "traverse" prompt only came looking back)
            let yaw = fwd.x.atan2(fwd.z) + std::f32::consts::PI;
            let q = glam::Quat::from_rotation_y(yaw);
            ph.orientation = Quaternion(q.x, q.y, q.z, q.w);
        }
        h.last_set = Some(v1_feet);
        // between ULTRAKILL's frames the eye moves on with V1's velocity (it runs at its own
        // rate: holding the last eye made the view step)
        let ahead = {
            use windows::Win32::System::Performance::{QueryPerformanceCounter, QueryPerformanceFrequency};
            let (mut now, mut freq) = (0i64, 0i64);
            unsafe {
                let _ = QueryPerformanceCounter(&mut now);
                let _ = QueryPerformanceFrequency(&mut freq);
            }
            if g.frame_qpc > 0 && freq > 0 { ((now - g.frame_qpc) as f32 / freq as f32).clamp(0.0, 0.05) } else { 0.0 }
        };
        let eye = Vec3::from(g.eye) + Vec3::from(g.vel) * ahead;
        if debug::NO_CAMERA.load(Ordering::Relaxed) {
            camera::release();
        } else {
            camera::set(h.stream.to_havok(eye), Vec3::from(g.fwd), Vec3::from(g.up), g.fov);
        }
    } else {
        h.last_set = None;
        camera::release();
        // V1 is only on its way (a teleport, F7, ULTRAKILL reconnecting): the Tarnished is held
        // where he is, without gravity, so he can't drop through the ground meanwhile. Elden Ring
        // gets him back (gravity on) for its own things: cutscenes, death, V1 off.
        let hold = enabled && alive && !cutscene && !dead && settled;
        if let Some(player) = (unsafe { WorldChrMan::instance_mut() }).ok().and_then(|w| w.main_player.as_mut()) {
            let ph = &mut player.chr_ins.modules.physics;
            ph.gravity_disabled = hold;
            if let (true, Some(t)) = (hold, h.teleport_target) {
                ph.position = HavokPosition(t.x, t.y + 0.1, t.z, 0.0);
                ph.chr_proxy_pos_update_requested = true;
            }
        }
    }
    HIDE_TARNISHED.store(drive && !debug::SHOW_TARNISHED.load(Ordering::Relaxed), Ordering::Relaxed);
    let capture = drive && !menu && focused();
    input::CAPTURE.store(capture, Ordering::Relaxed);
    cursor::poll_keys();
    cursor::poll_mouse();
    if capture {
        cursor::poll_buttons();
    } else {
        input::release_all();
    }
    overlay::SHOW.store(drive && g.flags & GUEST_IN_LEVEL != 0, Ordering::Relaxed);
    // F1: ULTRAKILL's menu (its Escape menu: options, sound, HUD...). The cursor points into it;
    // F1 again, or closing it in ULTRAKILL, gives the mouse back to V1.
    {
        let f1 = focused() && unsafe { GetAsyncKeyState(0x70) } as u16 & 0x8000 != 0;
        let toggle = f1 && !h.f1_was;
        h.f1_was = f1;
        let open = g.flags & GUEST_MENU != 0;
        if h.esc_up_in > 0 {
            h.esc_up_in -= 1;
            if h.esc_up_in == 0 {
                link.push_input(IN_KEY, 0x1B, 0, 0, 0);
            }
        }
        if toggle && drive && !menu {
            link.push_input(IN_KEY, 0x1B, 1, 0, 0);
            h.esc_up_in = 4;
            h.uk_menu = !h.uk_menu;
            h.uk_menu_since = 0.0;
            if h.uk_menu {
                cursor::centre_menu_pointer(overlay::picture_rect());
            }
            debug::note_event(if h.uk_menu { "F1: ULTRAKILL's menu" } else { "F1: back to V1" });
        }
        if h.uk_menu {
            h.uk_menu_since += dt;
            if !drive || (h.uk_menu_since > 0.7 && !open) {
                h.uk_menu = false;
            }
        }
        cursor::MENU_MODE.store(h.uk_menu, Ordering::Relaxed);
        if h.uk_menu {
            let (at, size) = overlay::picture_rect();
            let p = cursor::menu_pointer((at, size));
            let x = ((p[0] - at[0]) / size[0].max(1.0)).clamp(0.0, 1.0);
            let y = ((p[1] - at[1]) / size[1].max(1.0)).clamp(0.0, 1.0);
            overlay::MENU_POINTER.store(((p[0] as u32) << 16) | (p[1] as u32 & 0xFFFF), Ordering::Relaxed);
            link.push_input(IN_MOUSE_POS, 0, (x * 10000.0) as i32, (y * 10000.0) as i32, 0);
        }
    }
    hide_hud(h, drive && !menu);
    overlay::STYLE.store(g.style_rank, Ordering::Relaxed);
    combat::read_boss_bars();
    overlay::read_style_text(link);
    if let Some(p) = link.hud_prefs() {
        *overlay::HUD_PREFS.lock().unwrap_or_else(|e| e.into_inner()) = p;
    }
    *overlay::V1_HUD.lock().unwrap_or_else(|e| e.into_inner()) = (drive && !menu).then_some((g.hp, g.hard_damage, g.stamina));

    elden_ring_volumes(alive);

    // fights
    let v1_alive = g.flags & GUEST_DEAD == 0;
    if let Ok(flags) = unsafe { eldenring::cs::WorldChrManDbgFlags::instance_mut() } {
        // while V1 lives the Tarnished can't die: his damage is V1's
        flags.player_no_dead = drive && v1_alive && h.kill_frames == 0;
    }
    if h.kill_frames > 0 {
        h.kill_frames -= 1;
        if let Some(player) = (unsafe { WorldChrMan::instance_mut() }).ok().and_then(|w| w.main_player.as_mut()) {
            player.chr_ins.modules.data.hp = 0;
        }
    }
    if drive && h.kill_frames == 0 {
        if let Some(player) = (unsafe { WorldChrMan::instance_mut() }).ok().and_then(|w| w.main_player.as_mut()) {
            let data = &mut player.chr_ins.modules.data;
            let mut hp_now = data.hp;
            let uk = h.combat.tarnished_hurt(&mut hp_now, max_hp, v1_alive);
            data.hp = hp_now;
            if uk > 0 && debug::GOD.load(Ordering::Relaxed) {
                dlog(format!("combat: the Tarnished was hit ({uk}); god mode: V1 takes nothing"));
            } else if uk > 0 {
                link.push_input(IN_HURT, HURT_MELEE, uk, 0, 0);
                dlog(format!("combat: the Tarnished was hit: V1 takes {uk}"));
            }
        }
    }
    while let Some(e) = link.pop_event() {
        match e.kind {
            EV_HIT_ACTOR => {
                overlay::note_hit();
                if let Some(boss) = h.combat.hit(&e) {
                    link.push_input(IN_KILLED, 0, boss as i32, e.actor as i32, 0);
                    debug::note_event(format!("V1 killed {:#x}{}", e.actor, if boss { " (a boss)" } else { "" }));
                }
            }
            EV_DIED if enabled => {
                debug::note_event("V1 died: so does the Tarnished");
                if let Ok(flags) = unsafe { eldenring::cs::WorldChrManDbgFlags::instance_mut() } {
                    flags.player_no_dead = false;
                }
                if let Some(player) = (unsafe { WorldChrMan::instance_mut() }).ok().and_then(|w| w.main_player.as_mut()) {
                    player.chr_ins.modules.data.hp = 0;
                }
            }
            _ => {}
        }
    }
    let at = if drive { v1_feet } else { p };
    if enabled && alive {
        h.combat.publish(link, &h.stream, at);
    }

    // Placing the collision again around the Tarnished, in Elden Ring's current frame:
    //  - after a loading screen or a respawn (the physics world is rebuilt in a new frame: the
    //    collision kept the old one, ended up off by tens of metres, and V1 fell into the void);
    //  - when V1 has waited 4 s for its floor under a Tarnished standing on Elden Ring's ground
    //    (any other way the two came apart).
    if h.prev_hp <= 0 && hp > 0 {
        h.reanchor = true;
    }
    h.prev_hp = hp;
    // (not gated on the Tarnished "standing on solid ground": after a respawn that never read true,
    // the collision was never placed again, and V1 clipped into the map)
    let waiting = !drive && alive && hp > 0 && g.teleport_ack != h.teleport_seq && h.teleport_target.is_none();
    h.realign_wait = if waiting && h.in_world_time > 2.0 && !menu { h.realign_wait + dt } else { 0.0 };
    if drive {
        h.realign_tries = 0;
    }
    let realign_after = if h.realign_tries < 3 { 4.0 } else { 30.0 };
    if (h.reanchor && h.in_world_time > 2.0 && hp > 0) || h.realign_wait > realign_after {
        h.realign_tries += 1;
        let why = if h.reanchor { "after a loading screen / respawn" } else { "V1 found no floor under the Tarnished for 4 s" };
        h.reanchor = false;
        h.realign_wait = 0.0;
        let snap = (p / 64.0).round() * 64.0;
        h.stream.restart(snap);
        h.last_safe = None;
        h.teleport_seq = h.teleport_seq.wrapping_add(1);
        debug::note_event(format!("collision placed again around the Tarnished ({why}; epoch {})", h.stream.epoch));
    }

    // collision around V1 (or the Tarnished while V1 waits)
    let collision_done = if enabled && alive { h.stream.update(link, at) } else { true };

    // the way the Tarnished faces (his rotation's -Z), as a yaw for V1
    let tarnished_yaw = facing.to_euler(glam::EulerRot::YXZ).0.to_degrees() + 180.0;
    let g_pos = h.stream.to_guest(h.teleport_target.unwrap_or(p));
    let (vw, vh) = (overlay::VIEW_W.load(Ordering::Relaxed), overlay::VIEW_H.load(Ordering::Relaxed));
    link.write_host_state(&HostState {
        flags: HOST_IN_WORLD
            | if menu { HOST_MENU } else { 0 }
            | if cutscene { HOST_CUTSCENE } else { 0 }
            | if dead { HOST_DEAD } else { 0 }
            | if enabled { HOST_ENABLED } else { 0 }
            | if settled { 0 } else { HOST_LOADING }
            | if debug::SHOW_COLLISION.load(Ordering::Relaxed) { HOST_SHOW_COLLISION } else { 0 }
            | if debug::SHOW_HITBOXES.load(Ordering::Relaxed) { HOST_SHOW_HITBOXES } else { 0 },
        world_id: 0,
        epoch: h.stream.epoch,
        origin: h.stream.origin.to_array().map(|v| v as f64),
        pos: g_pos.to_array(),
        teleport_seq: h.teleport_seq,
        viewport: if vw > 0 && vh > 0 { [vw, vh] } else { [1920, 1080] },
        units_per_metre: h.stream.units,
        hp,
        hp_max: max_hp,
        yaw: tarnished_yaw,
        game_hour: 12.0,
    });

    overlay::set_status(if !enabled {
        None
    } else if !alive {
        Some("EldenKill: waiting for ULTRAKILL (it starts hidden; the first start takes a while)".into())
    } else if g.flags & GUEST_IN_LEVEL == 0 {
        Some("EldenKill: ULTRAKILL is loading V1...".into())
    } else if !drive && !collision_done && !cutscene && !menu {
        Some(format!("EldenKill: sending the Lands Between to ULTRAKILL ({} cells)...", h.stream.cells()))
    } else {
        None
    });

    // the last spot V1 stood on (for F7), the launch detector
    if drive && g.flags & GUEST_ON_GROUND != 0 && g.flags & (GUEST_DASHING | GUEST_SLIDING) == 0 {
        h.safe_timer += dt;
        if h.safe_timer > 0.5 {
            h.last_safe = Some(v1_feet);
        }
    } else {
        h.safe_timer = 0.0;
    }
    if drive {
        h.launch.check(g.vel, g.flags, g.weapon, v1_feet);
    }
    {
        let (dx, dy, n) = debug::take_mouse();
        let w = &mut h.mouse_window;
        w.0 += dx;
        w.1 += dy;
        w.2 += n;
        w.3 += dt;
        if w.3 >= 0.5 {
            h.mouse_shown = (w.0, w.1, w.2);
            *w = (0, 0, 0, 0.0);
        }
    }
    if debug::SHOW.load(Ordering::Relaxed) {
        let speed = Vec3::from(g.vel).length();
        let units = h.stream.units.max(0.01);
        debug::set_panel(vec![
            "# V1".into(),
            format!("  feet ({:.1}, {:.1}, {:.1}) m   speed {:.0} u/s ({:.1} m/s)   up {:.0}", v1_feet.x, v1_feet.y, v1_feet.z, speed, speed / units, g.vel[1]),
            format!("  {}   weapon {} ({})   hp {}   dashes {:.1}", debug::flags_text(g.flags), g.weapon, debug::weapon_name(g.weapon), g.hp, g.stamina / 100.0),
            format!("  mouse last 0.5 s: dx {} dy {} ({} moves)   ULTRAKILL overlay {:.0} fps", h.mouse_shown.0, h.mouse_shown.1, h.mouse_shown.2, overlay_fps(link)),
            "# who has the player".into(),
            format!("  {}   capture {capture}   menu {menu}   cutscene {cutscene} (anim {anim})   focused {}", if drive { "V1 drives" } else { "Elden Ring has him" }, focused()),
            format!("  teleport {}/{}   epoch {}/{}   key polling {}   V1 {}", g.teleport_ack, h.teleport_seq, g.epoch_ack, h.stream.epoch, cursor::KEY_POLLING.load(Ordering::Relaxed), if enabled { "on" } else { "OFF (F9)" }),
            "# Tarnished".into(),
            format!("  at ({:.1}, {:.1}, {:.1})   {:.1} m from V1   hp {hp}/{max_hp}   dead {dead}", p.x, p.y, p.z, (p - v1_feet).length()),
            format!("  unstick spot: {}", h.last_safe.map(|s| format!("({:.1}, {:.1}, {:.1})", s.x, s.y, s.z)).unwrap_or_else(|| "none yet".into())),
            "# collision".into(),
            format!("  {} cells, {} triangles   origin ({:.0}, {:.0}, {:.0})   hits on enemies {}", h.stream.cells(), h.stream.triangles, h.stream.origin.x, h.stream.origin.y, h.stream.origin.z, h.combat.hits),
        ]);
    }

    // alignment probe: where Elden Ring itself finds the ground under V1 (its raycast, in the
    // characters' space) against where V1 stands (on the collision streamed from Havok bodies).
    // Logged only, to find out whether the two spaces ever disagree.
    {
        static PROBE: Mutex<(f32, u32)> = Mutex::new((0.0, 0));
        let mut probe = PROBE.lock().unwrap_or_else(|e| e.into_inner());
        probe.0 += dt;
        if drive && g.flags & GUEST_ON_GROUND != 0 && probe.0 > 1.0 {
            probe.0 = 0.0;
            if let (Ok(havok), Some(player)) = (unsafe { eldenring::cs::CSHavokMan::instance() }, (unsafe { WorldChrMan::instance() }).ok().and_then(|w| w.main_player.as_ref())) {
                // from just above the feet (a ray from high up hit ceilings indoors first), then,
                // if that finds nothing, a long one down
                let from = HavokPosition(v1_feet.x, v1_feet.y + 1.5, v1_feet.z, 0.0);
                let hit = havok
                    .phys_world
                    .cast_ray(0x08, &from, eldenring::position::PositionDelta(0.0, -4.0, 0.0), player)
                    .or_else(|| havok.phys_world.cast_ray(0x08, &from, eldenring::position::PositionDelta(0.0, -120.0, 0.0), player));
                match hit {
                    Some(h2) if (h2.1 - v1_feet.y).abs() < 1.0 => probe.1 = 0,
                    Some(h2) => {
                        probe.1 += 1;
                        debug::note_event(format!(
                            "align: Elden Ring's ground is {:.1} m {} V1's feet (at {:.1}, {:.1}, {:.1})",
                            (h2.1 - v1_feet.y).abs(),
                            if h2.1 > v1_feet.y { "above" } else { "below" },
                            v1_feet.x, v1_feet.y, v1_feet.z
                        ));
                    }
                    None => {
                        probe.1 += 1;
                        debug::note_event(format!("align: Elden Ring finds no ground within 120 m under V1's feet (at {:.1}, {:.1}, {:.1})", v1_feet.x, v1_feet.y, v1_feet.z));
                    }
                }
            }
        }
    }

    h.log_timer += dt;
    if debug() && h.log_timer > 5.0 {
        h.log_timer = 0.0;
        log(format!(
            "status: drive {drive}, capture {capture}, menu {menu}, focused {}, key polling {}, mouse look {} px, shape cache {:?}, DI calls {:?}, guest flags {:#x}, V1 hp {}, cells {} / {} tris, hits {}, overlay frames {}, at {at:.1?}",
            focused(),
            cursor::KEY_POLLING.load(Ordering::Relaxed),
            cursor::MOUSE_SENT.load(Ordering::Relaxed),
            h.stream.cache_stats(),
            input::CALLS.iter().map(|c| c.load(Ordering::Relaxed)).collect::<Vec<_>>(),
            g.flags,
            g.hp,
            h.stream.cells(),
            h.stream.triangles,
            h.combat.hits,
            link.overlay_frames()
        ));
    }
}

/// ULTRAKILL's overlay frames per second (over the last second or so).
fn overlay_fps(link: &Link) -> f32 {
    static LAST: Mutex<Option<(std::time::Instant, u64, f32)>> = Mutex::new(None);
    let mut last = LAST.lock().unwrap_or_else(|e| e.into_inner());
    let now = link.overlay_frames();
    match *last {
        Some((t, _, fps)) if t.elapsed().as_secs_f32() < 1.0 => fps,
        Some((t, n, _)) => {
            let fps = (now.saturating_sub(n)) as f32 / t.elapsed().as_secs_f32();
            *last = Some((std::time::Instant::now(), now, fps));
            fps
        }
        None => {
            *last = Some((std::time::Instant::now(), now, 0.0));
            0.0
        }
    }
}

fn stand_down(h: &mut Host, link: &Link) {
    let _ = link;
    HIDE_TARNISHED.store(false, Ordering::Relaxed);
    input::CAPTURE.store(false, Ordering::Relaxed);
    input::release_all();
    camera::release();
    overlay::SHOW.store(false, Ordering::Relaxed);
    hide_hud(h, false);
}

/// Elden Ring's HP / FP / stamina bars mean nothing while V1 plays (ULTRAKILL's HUD is in the
/// overlay); menus keep theirs.
fn hide_hud(h: &mut Host, hide: bool) {
    use eldenring::cs::CSFeManHudState as Hud;
    let Ok(fe) = (unsafe { eldenring::cs::CSFeManImp::instance_mut() }) else { return };
    if hide {
        if matches!(fe.hud_state, Hud::Default) {
            fe.hud_state = Hud::HideAll;
            h.hud_hidden = true;
        }
    } else if h.hud_hidden {
        h.hud_hidden = false;
        if matches!(fe.hud_state, Hud::HideAll) {
            fe.hud_state = Hud::Default;
        }
    }
}

/// The Tarnished isn't drawn while V1 has him (the camera is inside his head), written at every
/// point the game re-applies his opacity (er-mario's pose_task_late groups).
fn hide_task() {
    static WAS: AtomicBool = AtomicBool::new(false);
    let hide = HIDE_TARNISHED.load(Ordering::Relaxed);
    let was = WAS.swap(hide, Ordering::Relaxed);
    if !hide && !was {
        return;
    }
    if let Some(p) = (unsafe { WorldChrMan::instance_mut() }).ok().and_then(|w| w.main_player.as_mut()) {
        if hide {
            p.chr_ins.opacity_keyframes_multiplier = 0.0;
            p.chr_ins.opacity_keyframes_multiplier_previous = 0.0;
        }
        p.chr_ins.chr_flags1c5.set_enable_render(!hide);
    }
}

#[derive(Default)]
struct Trace {
    written: Option<[f32; 3]>,
    start: [f32; 3],
    last_update: [f32; 3],
    flag_still_set: bool,
    late: Option<[f32; 3]>,
    timer: Option<std::time::Instant>,
}

static TRACE: Mutex<Trace> = Mutex::new(Trace { written: None, start: [0.0; 3], last_update: [0.0; 3], flag_still_set: false, late: None, timer: None });

/// Late in the frame (Draw_Pre): where the Tarnished is after everything ran, and every 2 s a
/// trace line: written -> late this frame -> next frame's start.
fn trace_task() {
    let Some(player) = (unsafe { WorldChrMan::instance() }).ok().and_then(|w| w.main_player.as_ref()) else { return };
    let ph = &player.chr_ins.modules.physics;
    let mut tr = TRACE.lock().unwrap_or_else(|e| e.into_inner());
    let Some(w) = tr.written else { return };
    let late = [ph.position.0, ph.position.1, ph.position.2];
    let due = tr.timer.is_none_or(|t| t.elapsed().as_secs_f32() > 2.0);
    if due {
        tr.timer = Some(std::time::Instant::now());
        let d = |a: [f32; 3], b: [f32; 3]| ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt();
        use eldenring::cs::ChrInsExt;
        log(format!(
            "trace: wrote ({:.1}, {:.1}, {:.1}); late this frame {:.2} m off; next start ({:.1}, {:.1}, {:.1}) = {:.2} m off, delta ({:.2}, {:.2}, {:.2}); last_update {:.2} m off; request still set: {}; block {:?} origin {:?}",
            w[0], w[1], w[2],
            d(late, w),
            tr.start[0], tr.start[1], tr.start[2],
            d(tr.start, w),
            tr.start[0] - w[0], tr.start[1] - w[1], tr.start[2] - w[2],
            d(tr.last_update, w),
            tr.flag_still_set,
            player.chr_ins.block_id(),
            player.chr_ins.block_id_origin()
        ));
    }
    tr.late = Some(late);
}

/// Right after the game turns input into the Tarnished's actions (ChrIns_PreBehaviorSafe): while
/// V1 has him, every action but interact is taken away, so mouse buttons or keys that still reach
/// Elden Ring never make the invisible Tarnished attack, roll or jump (er-mario's input_task).
fn strip_actions() {
    if !HIDE_TARNISHED.load(Ordering::Relaxed) {
        return;
    }
    let Some(player) = (unsafe { WorldChrMan::instance_mut() }).ok().and_then(|w| w.main_player.as_mut()) else { return };
    let req: &mut eldenring::cs::CSChrActionRequestModule = &mut player.chr_ins.modules.action_request;
    let bits = |a: &mut eldenring::cs::ChrActions| unsafe { &mut *(a as *mut _ as *mut u64) };
    const INTERACT: u64 = 1 << 4;
    for a in [&mut req.action_requests, &mut req.new_action_presses, &mut req.queued_action_inputs, &mut req.cancel_ready_actions] {
        *bits(a) &= INTERACT;
    }
    req.movement_request_duration = 0.0;
}

/// Called from DllMain: everything else happens on a thread once the game's task system is up.
pub fn start(hmodule: usize) {
    MODULE.store(hmodule, Ordering::Relaxed);
    std::thread::spawn(|| {
        log(format!("EldenKill {} loaded", env!("CARGO_PKG_VERSION")));
        // frame rate and ultrawide: as early as possible, before the game reads those values
        if version::check().is_ok() {
            patches::apply();
        }
        // (a failed lookup used to unwrap: the thread died silently before the panic hook, and
        // nothing of EldenKill ran, ULTRAKILL included; some launches out of several)
        std::panic::set_hook(Box::new(|info| log(format!("PANIC: {info}"))));
        let mut tries = 0u32;
        let cs_task = loop {
            match CSTaskImp::wait_for_instance(Duration::from_secs(5)) {
                Ok(t) => break t,
                Err(e) => {
                    tries += 1;
                    if tries % 6 == 1 {
                        log(format!("waiting for the game's task system ({e:?}), {}s", tries * 5));
                    }
                    std::thread::sleep(Duration::from_millis(250));
                }
            }
        };
        if let Err(e) = version::check() {
            log(format!("this game version is not supported ({e}); EldenKill stays off"));
            return;
        }
        std::panic::set_hook(Box::new(|info| log(format!("PANIC: {info}"))));
        match Link::create() {
            Ok(l) => {
                let _ = LINK.set(l);
            }
            Err(e) => {
                log(format!("link: {e}; EldenKill stays off"));
                return;
            }
        }
        // the heartbeat can't wait for the frame task: it doesn't run on the title screen or
        // during loading screens, and ULTRAKILL would think Elden Ring is gone
        std::thread::spawn(|| {
            loop {
                if let Some(link) = link() {
                    link.heartbeat();
                }
                std::thread::sleep(Duration::from_millis(250));
            }
        });
        launcher::start();
        // high priority against stutter (what the "PC Stutter Fix" mod does without administrator
        // rights). Only once EldenKill is up: set at load, the game's task system never showed up for
        // EldenKill (2 launches out of 3) and nothing of it ran
        if crate::paths::config_bool("high_priority", true) {
            use windows::Win32::System::Threading::{GetCurrentProcess, HIGH_PRIORITY_CLASS, SetPriorityClass};
            let ok = unsafe { SetPriorityClass(GetCurrentProcess(), HIGH_PRIORITY_CLASS) }.is_ok();
            log(format!("process priority high: {}", if ok { "set" } else { "failed" }));
        }
        unsafe { input::install_hooks() };
        unsafe { cursor::install() };
        std::thread::spawn(|| {
            if !std::panic::catch_unwind(|| overlay::install(MODULE.load(Ordering::Relaxed))).unwrap_or(false) {
                log("overlay: not running; V1's view won't show");
            }
        });
        cs_task.run_recurring(
            |d: &FD4TaskData| {
                // a bug in the mod must never take the game down: log it and switch V1 off
                if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| frame(d))).is_err() {
                    ENABLED.store(false, Ordering::Relaxed);
                    HIDE_TARNISHED.store(false, Ordering::Relaxed);
                    input::CAPTURE.store(false, Ordering::Relaxed);
                }
            },
            CSTaskGroupIndex::ChrIns_PostPhysics,
        );
        let guarded = |f: fn()| {
            move |_: &FD4TaskData| {
                let _ = std::panic::catch_unwind(f);
            }
        };
        cs_task.run_recurring(guarded(strip_actions), CSTaskGroupIndex::ChrIns_PreBehaviorSafe);
        cs_task.run_recurring(guarded(trace_task), CSTaskGroupIndex::Draw_Pre);
        for group in [
            CSTaskGroupIndex::CameraStep,
            CSTaskGroupIndex::DrawParamUpdate,
            CSTaskGroupIndex::ChrIns_PostPhysicsSafe,
            CSTaskGroupIndex::CSDistViewManager_Update,
            CSTaskGroupIndex::WorldChrMan_PostPhysics,
            CSTaskGroupIndex::GameFlowStep_Post,
            CSTaskGroupIndex::Draw_Pre,
        ] {
            cs_task.run_recurring(guarded(camera::reapply), group);
        }
        for group in [
            CSTaskGroupIndex::ChrIns_PrePhysics_End,
            CSTaskGroupIndex::ChrIns_PostPhysics,
            CSTaskGroupIndex::LocationUpdate_PostCloth,
            CSTaskGroupIndex::WorldChrMan_PostPhysics,
            CSTaskGroupIndex::Draw_Pre,
        ] {
            cs_task.run_recurring(guarded(hide_task), group);
        }
        log("frame task registered");
    });
}

/// Elden Ring's volumes while ULTRAKILL plays along (its own Sound options, 0-10): effects and
/// voice as eldenkill.ini says (-1 = untouched, the default).
/// The player's settings come back once ULTRAKILL is gone.
fn elden_ring_volumes(linked: bool) {
    static SAVED: Mutex<Option<(u8, u8)>> = Mutex::new(None);
    let Ok(gdm) = (unsafe { eldenring::cs::GameDataMan::instance_mut() }) else { return };
    let gs = &mut *gdm.game_settings;
    let mut saved = SAVED.lock().unwrap_or_else(|e| e.into_inner());
    if linked {
        if saved.is_none() {
            *saved = Some((gs.sfx_volume, gs.voice_volume));
        }
        let want = |key: &str, default: f32, cur: u8| {
            let v = crate::paths::config_f32(key, default);
            if v < 0.0 { cur } else { v.clamp(0.0, 10.0) as u8 }
        };
        let (s, v) = saved.unwrap();
        gs.sfx_volume = want("er_sfx_volume", -1.0, s);
        gs.voice_volume = want("er_voice_volume", -1.0, v);
    } else if let Some((s, v)) = saved.take() {
        gs.sfx_volume = s;
        gs.voice_volume = v;
    }
}

//! Fights between V1 and Elden Ring's characters (Killcraft's Combat.cs, roles swapped):
//!  - characters near the player go to ULTRAKILL as hitboxes (the actor table);
//!  - V1's hits on them come back as HIT_ACTOR events and take Elden Ring HP;
//!  - what hurts the Tarnished (who stands where V1 is) hurts V1 instead: his HP loss each frame
//!    becomes ULTRAKILL damage, and his HP is put back while V1 lives (er-mario does the same for
//!    Mario's health).

use std::collections::HashMap;

use eldenring::cs::{ChrIns, ChrType, FieldInsHandle, WorldChrMan};
use fromsoftware_shared::FromStatic;
use glam::Vec3;

use super::stream::Stream;
use crate::link::{ActorRecord, Event, Link};
use crate::log;
use crate::proto::*;

/// Characters within this many metres go to ULTRAKILL.
const RANGE: f32 = 150.0;
/// The boss bars Elden Ring itself has up (its HUD is hidden while V1 plays, so the overlay draws
/// them): name, HP, max HP, the damage number, the HP before the last hits.
pub struct BossBar {
    pub name: String,
    pub hp: i32,
    pub max: i32,
    pub damage: i32,
    pub before: i32,
}

pub static BOSSES: std::sync::Mutex<Vec<BossBar>> = std::sync::Mutex::new(Vec::new());

/// A boss's name from Elden Ring's text (the NpcName FMG; its HUD fills the bar's name only while
/// shown, and EldenKill hides it). Layout as in er-mario's names.rs: [[MsgRepository+8]] = FMGs by
/// binder id; FMG +0xC group count, +0x18 string offsets, +0x28 groups (index, first id, last id).
fn npc_name(id: i32) -> Option<String> {
    use super::explore::{read_u64, readable};
    if id <= 0 {
        return None;
    }
    let repo = unsafe { eldenring::cs::MsgRepositoryImp::instance() }.ok()?;
    let table = read_u64(repo as *const _ as usize + 8).and_then(|l| read_u64(l as usize))? as usize;
    // NpcName: the patch's, the base game's, then the DLCs'
    for binder in [118usize, 18, 318, 418] {
        let Some(fmg) = read_u64(table + binder * 8).filter(|&p| p != 0).map(|p| p as usize) else { continue };
        if !readable(fmg, 0x40) {
            continue;
        }
        let groups = unsafe { *((fmg + 0xC) as *const u32) } as usize;
        let offsets = unsafe { *((fmg + 0x18) as *const usize) };
        if groups > 100_000 || !readable(offsets, 8) {
            continue;
        }
        for g in 0..groups {
            let e = fmg + 0x28 + g * 0x10;
            if !readable(e, 0x10) {
                break;
            }
            let (index, first, last) = unsafe { (*(e as *const i32), *((e + 4) as *const i32), *((e + 8) as *const i32)) };
            if id < first || id > last {
                continue;
            }
            let slot = offsets + (index + (id - first)) as usize * 8;
            let off = read_u64(slot)?;
            if off == 0 {
                break;
            }
            let p = fmg + off as usize;
            let mut text = Vec::new();
            while text.len() < 128 && readable(p + text.len() * 2, 2) {
                let c = unsafe { *((p + text.len() * 2) as *const u16) };
                if c == 0 {
                    break;
                }
                text.push(c);
            }
            if !text.is_empty() {
                return Some(String::from_utf16_lossy(&text));
            }
        }
    }
    None
}

/// Reads Elden Ring's boss bar list (CSFeMan's boss tags): only the bosses it shows a bar for.
pub fn read_boss_bars() {
    let mut out = Vec::new();
    if let (Ok(fe), Ok(wcm)) = (unsafe { eldenring::cs::CSFeManImp::instance() }, unsafe { WorldChrMan::instance() }) {
        for (i, tag) in fe.frontend_values.boss_list_tag_data.iter().enumerate() {
            let entry = &fe.boss_health_displays[i];
            if !tag.is_visible && entry.fmg_id <= 0 {
                continue;
            }
            let handle = if tag.is_visible { &tag.field_ins_handle } else { &entry.field_ins_handle };
            let Some(chr) = wcm.chr_ins_by_handle(handle) else { continue };
            let data = &chr.modules.data;
            let name = tag.chr_name.to_string();
            let name = if name.is_empty() {
                static NAMES: std::sync::Mutex<Option<HashMap<i32, String>>> = std::sync::Mutex::new(None);
                let mut names = NAMES.lock().unwrap_or_else(|e| e.into_inner());
                names
                    .get_or_insert_with(HashMap::new)
                    .entry(entry.fmg_id)
                    .or_insert_with(|| {
                        let n = npc_name(entry.fmg_id).unwrap_or_default();
                        log(format!("boss bar: text {} is {n:?}", entry.fmg_id));
                        n
                    })
                    .clone()
            } else {
                name
            };
            out.push(BossBar {
                name: if name.is_empty() { "BOSS".into() } else { name },
                hp: data.hp.max(0),
                max: data.max_hp.max(1),
                damage: entry.damage_taken.max(tag.last_damage_taken),
                before: tag.last_hp_value as i32,
            });
        }
    }
    *BOSSES.lock().unwrap_or_else(|e| e.into_inner()) = out;
}

pub struct Combat {
    /// actor id (low 32 bits of the handle) -> handle, from the last table
    handles: HashMap<u32, FieldInsHandle>,
    last_hp: Option<i32>,
    /// Elden Ring HP per point of ULTRAKILL damage (a revolver shot is 1)
    pub er_per_uk: f32,
    pub boss_factor: f32,
    /// ULTRAKILL damage per 1% of the Tarnished's max HP lost
    pub uk_per_percent: f32,
    /// ULTRAKILL health a normal / elite (over 1000 HP) / boss (over 3000 HP) Elden Ring enemy
    /// counts as (0 = the old flat damage_to_elden_ring per point)
    pub uk_hp_normal: f32,
    pub uk_hp_elite: f32,
    pub uk_hp_boss: f32,
    pub hits: u32,
}

fn handle_key(h: &FieldInsHandle) -> u64 {
    unsafe { std::mem::transmute_copy::<FieldInsHandle, u64>(h) }
}

/// Enemies and NPCs (er-mario's list); never the player, phantoms or the ghost kinds.
fn hittable(t: ChrType) -> bool {
    matches!(t, ChrType::Npc | ChrType::Unk6 | ChrType::Unk7 | ChrType::Unk9 | ChrType::Unk12 | ChrType::BloodyFingerNpc | ChrType::RecusantNpc)
}

impl Combat {
    pub fn new() -> Self {
        Combat {
            handles: HashMap::new(),
            last_hp: None,
            er_per_uk: crate::paths::config_f32("damage_to_elden_ring", 60.0),
            boss_factor: crate::paths::config_f32("boss_damage_factor", 0.5),
            uk_per_percent: crate::paths::config_f32("damage_to_v1", 1.5),
            uk_hp_normal: crate::paths::config_f32("uk_hp_normal", 3.0),
            uk_hp_elite: crate::paths::config_f32("uk_hp_elite", 10.0),
            uk_hp_boss: crate::paths::config_f32("uk_hp_boss", 80.0),
            hits: 0,
        }
    }

    /// The actor table: living characters near `at` (Havok metres), in the guest's space.
    pub fn publish(&mut self, link: &Link, stream: &Stream, at: Vec3) {
        let Ok(wcm) = (unsafe { WorldChrMan::instance() }) else { return };
        let mut out = Vec::new();
        self.handles.clear();
        for set in wcm.chr_sets.iter().flatten() {
            for chr in set.characters() {
                let chr: &ChrIns = chr;
                if !hittable(chr.chr_type) || chr.team_type == 0 || chr.modules.data.hp <= 0 {
                    continue;
                }
                let p = chr.modules.physics.position;
                let p = Vec3::new(p.0, p.1, p.2);
                if (p - at).length() > RANGE {
                    continue;
                }
                let ph = &chr.modules.physics;
                let r = ph.hit_radius.max(ph.chr_hit_radius);
                let h = ph.hit_height.max(ph.chr_hit_height);
                let r = if r.is_finite() && r > 0.1 { r.min(15.0) } else { 0.5 };
                let h = if h.is_finite() && h > 0.3 { h.min(40.0) } else { 1.8 };
                let key = handle_key(&chr.field_ins_handle);
                let id = (key as u32) ^ ((key >> 32) as u32);
                self.handles.insert(id, chr.field_ins_handle.clone());
                let g = stream.to_guest(p);
                let q = ph.orientation;
                // (characters face their rotation's -Z: half round for the hitbox's forward)
                let yaw = glam::Quat::from_xyzw(q.0, q.1, q.2, q.3).to_euler(glam::EulerRot::YXZ).0.to_degrees() + 180.0;
                let max = chr.modules.data.max_hp.max(1);
                out.push(ActorRecord {
                    id,
                    flags: ACTOR_HOSTILE,
                    pos: [g.x, g.y, g.z],
                    yaw,
                    radius: r * stream.units,
                    height: h * stream.units,
                    hp_frac: chr.modules.data.hp as f32 / max as f32,
                    team: chr.team_type as u16,
                    name: [0; 24],
                });
                if out.len() >= MAX_ACTORS {
                    break;
                }
            }
        }
        link.write_actors(&out);
    }

    /// V1 hit actor `e.actor` for `e.a` ULTRAKILL damage. Some(boss) when that killed it.
    pub fn hit(&mut self, e: &Event) -> Option<bool> {
        let Some(handle) = self.handles.get(&e.actor) else {
            log(format!("combat: V1 hit {:#x}, which isn't in the actor list any more (out of range or gone): no damage", e.actor));
            return None;
        };
        let Ok(wcm) = (unsafe { WorldChrMan::instance_mut() }) else { return None };
        let Some(chr) = wcm.chr_ins_by_handle_mut(handle) else {
            log(format!("combat: V1 hit {:#x}, but Elden Ring has no such character now: no damage", e.actor));
            return None;
        };
        let data = &mut chr.modules.data;
        let boss = data.max_hp > 3000;
        // ULTRAKILL's damage against an ULTRAKILL-sized health: each Elden Ring enemy counts as
        // an ULTRAKILL enemy of its class (a revolver shot is 1; a Stray has ~3, a Hideous Mass
        // ~10, a boss ~80), so its share of the shot comes off its own max HP. (A flat 60 HP a
        // shot made bosses take ~140 shots and big enemies feel like sponges.)
        let uk_hp = if boss {
            self.uk_hp_boss
        } else if data.max_hp > 1000 {
            self.uk_hp_elite
        } else {
            self.uk_hp_normal
        };
        let dmg = if uk_hp > 0.0 { e.a * data.max_hp as f32 / uk_hp } else { e.a * self.er_per_uk * if boss { self.boss_factor } else { 1.0 } };
        let dmg = dmg.round().max(1.0) as i32;
        let before = data.hp;
        data.hp = (data.hp - dmg).max(0);
        self.hits += 1;
        if before <= 0 {
            log(format!("combat: V1 hit {:#x}, already dead (0 HP)", e.actor));
        }
        if crate::er::debug() {
            log(format!("combat: V1 hit {:#x} for {:.2} -> {dmg} HP ({before} -> {}/{})", e.actor, e.a, data.hp, data.max_hp));
        }
        (before > 0 && data.hp <= 0).then_some(boss)
    }

    /// The Tarnished's HP this frame: losses become V1's damage and are put back. Returns the
    /// ULTRAKILL damage to send (0 for none).
    pub fn tarnished_hurt(&mut self, hp: &mut i32, max_hp: i32, v1_alive: bool) -> i32 {
        let Some(last) = self.last_hp else {
            self.last_hp = Some(*hp);
            return 0;
        };
        let mut uk = 0;
        if *hp < last && v1_alive {
            let percent = (last - *hp) as f32 * 100.0 / max_hp.max(1) as f32;
            uk = (percent * self.uk_per_percent).round().max(1.0) as i32;
            *hp = last; // the Tarnished doesn't keep it: V1 took it
        }
        self.last_hp = Some(*hp);
        uk
    }

    pub fn forget_hp(&mut self) {
        self.last_hp = None;
    }
}

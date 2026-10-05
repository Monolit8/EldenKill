//! Elden Ring's collision for ULTRAKILL, streamed in 32 m world cells around V1 (Killcraft's
//! Collision.cs the other way round). Each cell is one REGION: every triangle of the game's
//! static Havok bodies that touches it, converted to the guest's units. Cells are sent once per
//! epoch, nearest first, and dropped again when V1 is far away.

use std::collections::{HashMap, HashSet};

use glam::Vec3;

use super::havok_col::HavokCollision;
use crate::link::{Link, col};
use crate::proto::*;

/// metres per cell side
pub const CELL: f32 = 32.0;
/// cells this far (m, horizontally) from V1 are loaded...
const LOAD: f32 = 56.0;
/// ...and dropped past this
const DROP: f32 = 120.0;
/// vertical cells loaded above and below V1's
const UP_CELLS: i32 = 1;
const DOWN_CELLS: i32 = 3;
/// at most this many new cells a frame (each is a Havok scan)
const PER_FRAME: usize = 2;
/// (60 000 cut Stormveil's dense cells: the triangles far from a cell's centre were dropped,
/// leaving holes along the cell edges that V1 fell through. 300 000 x 40 bytes = 12 MB, under
/// the ring's 16 MB per message.)
const MAX_TRIS_PER_CELL: usize = 300_000;
/// Havok layers that are world collision (er-mario's list: terrain, buildings, props, ...)
pub const LAYERS: [u32; 11] = [0x1e, 0x2e, 0x37, 0x38, 0x39, 0x3a, 0x46, 0x47, 0x48, 0x49, 0x51];

pub struct Stream {
    havok: HavokCollision,
    sent: HashSet<(i32, i32, i32)>,
    /// cells that came back empty, and when: asked again a moment later (a cell asked while the
    /// map was still loading came back empty and was never asked again: V1 fell through there)
    empty: HashMap<(i32, i32, i32), std::time::Instant>,
    /// the bodies each sent cell was made of: (body, shape, where it was). A gate or door that
    /// opens moves or leaves the physics world; its cell is sent again (V1 hit an invisible wall
    /// where a gate had been).
    cell_bodies: HashMap<(i32, i32, i32), Vec<(u32, usize, Vec3)>>,
    /// when each cell was last sent again for a change
    cell_history: HashMap<(i32, i32, i32), Vec<std::time::Instant>>,
    recheck: std::time::Instant,
    pub epoch: u32,
    pub origin: Vec3,
    pub units: f32,
    clear_sent: bool,
    pub triangles: usize,
    /// cells from before a Havok re-base, removed a moment later
    stale: Vec<(u64, std::time::Instant)>,
    /// static bodies to watch, each with a different shape: (body index, its shape, where it was)
    references: Vec<(u32, usize, Vec3)>,
}

fn cell_of(p: Vec3) -> (i32, i32, i32) {
    ((p.x / CELL).floor() as i32, (p.y / CELL).floor() as i32, (p.z / CELL).floor() as i32)
}

fn region_id(c: (i32, i32, i32)) -> u64 {
    // 18 bits per axis (cells of 32 m: +-4000 km), and a 10-bit generation on top
    let f = |v: i32| (v as i64 & 0x3_FFFF) as u64;
    f(c.0) | (f(c.1) << 18) | (f(c.2) << 36) | ((GENERATION.load(std::sync::atomic::Ordering::Relaxed) & 0x3FF) << 54)
}

/// Counts up with every re-base, so the new keys of a cell never equal old ones still waiting
/// to be removed. (A flip between two generations did: two opposite shifts within the removal
/// delay, common going up and down hills, gave the new cells the old ids, and the delayed
/// removal took the fresh ground away: V1 fell.)
static GENERATION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

impl Stream {
    pub fn new(units: f32) -> Self {
        Stream { havok: HavokCollision::new(LAYERS.to_vec()), sent: HashSet::new(), empty: HashMap::new(), cell_bodies: HashMap::new(), cell_history: HashMap::new(), recheck: std::time::Instant::now(), epoch: 0, origin: Vec3::ZERO, units, clear_sent: false, triangles: 0, stale: Vec::new(), references: Vec::new() }
    }

    /// A new epoch: new origin (Havok metres), everything sent again.
    pub fn restart(&mut self, origin: Vec3) {
        self.epoch = self.epoch.wrapping_add(1);
        self.origin = origin;
        self.sent.clear();
        self.empty.clear();
        self.cell_bodies.clear();
        self.cell_history.clear();
        self.clear_sent = false;
        self.triangles = 0;
        self.stale.clear();
        self.references.clear();
        self.havok.clear_cache();
    }

    pub fn to_guest(&self, p: Vec3) -> Vec3 {
        (p - self.origin) * self.units
    }

    pub fn to_havok(&self, g: Vec3) -> Vec3 {
        g / self.units + self.origin
    }

    /// Sends what's missing around `at` (Havok metres). Returns false while cells are still owed.
    pub fn update(&mut self, link: &Link, at: Vec3) -> bool {
        if !self.clear_sent {
            if !link.write_collision(COL_CLEAR, &col::clear(self.epoch)) {
                return false;
            }
            self.clear_sent = true;
        }
        // cells around V1, nearest first. Cells are on a grid fixed to the guest's space (Havok
        // minus the origin), which a Havok shift doesn't move: what was sent stays valid.
        let at = at - self.origin;
        let here = cell_of(at);
        let reach = (LOAD / CELL).ceil() as i32;
        let mut wanted = Vec::new();
        for dx in -reach..=reach {
            for dz in -reach..=reach {
                for dy in -DOWN_CELLS..=UP_CELLS {
                    let c = (here.0 + dx, here.1 + dy, here.2 + dz);
                    let centre = Vec3::new((c.0 as f32 + 0.5) * CELL, at.y, (c.2 as f32 + 0.5) * CELL);
                    let d = Vec3::new(centre.x - at.x, 0.0, centre.z - at.z).length() - CELL * 0.71;
                    let resting = self.empty.get(&c).is_some_and(|t| t.elapsed().as_secs_f32() < 3.0);
                    if d <= LOAD && !self.sent.contains(&c) && !resting {
                        wanted.push((d + dy.abs() as f32 * 4.0, c));
                    }
                }
            }
        }
        wanted.sort_by(|a, b| a.0.total_cmp(&b.0));
        if !self.havok.refresh_bodies() {
            return false;
        }
        for &(_, c) in wanted.iter().take(PER_FRAME) {
            let lo = Vec3::new(c.0 as f32, c.1 as f32, c.2 as f32) * CELL + self.origin;
            let hi = lo + Vec3::splat(CELL);
            let centre = (lo + hi) * 0.5;
            let Some(tris) = self.havok.query_boxes(centre, CELL, &[(lo, hi)], MAX_TRIS_PER_CELL) else { return false };
            if tris.len() >= MAX_TRIS_PER_CELL {
                crate::log(format!("collision: cell {c:?} has more than {MAX_TRIS_PER_CELL} triangles: cut (holes possible)"));
            }
            // watch up to four static bodies, each with a different shape
            for &(_, _, body) in tris.iter().step_by(97) {
                if self.references.len() >= 4 {
                    break;
                }
                let shape = self.havok.shape_of(body);
                if shape == 0 || self.references.iter().any(|r| r.0 == body || r.1 == shape) {
                    continue;
                }
                if let Some((t, _)) = self.havok.transform(body) {
                    self.references.push((body, shape, t));
                }
            }
            let out: Vec<([f32; 9], u32)> = tris
                .iter()
                .map(|(t, _layer, body)| {
                    let g = t.map(|v| self.to_guest(v));
                    let n = (t[1] - t[0]).cross(t[2] - t[0]).normalize_or_zero();
                    // the body goes along (bits 8-31): the guest orients each body's walls by its floors
                    let flags = if n.y.abs() > 0.7 { TRI_WALKABLE } else { 0 } | ((body & 0xFF_FFFF) << 8);
                    ([g[0].x, g[0].y, g[0].z, g[1].x, g[1].y, g[1].z, g[2].x, g[2].y, g[2].z], flags)
                })
                .collect();
            if !out.is_empty() && !link.write_collision(COL_REGION, &col::region(region_id(c), self.epoch, &out)) {
                return false; // ring full: again next frame
            }
            self.triangles += out.len();
            if out.is_empty() {
                // (a cell that had collision and has none now: what was there goes)
                if self.cell_bodies.remove(&c).is_some() && !link.write_collision(COL_REMOVE, &col::remove(region_id(c), self.epoch)) {
                    return false;
                }
                self.empty.insert(c, std::time::Instant::now());
            } else {
                self.empty.remove(&c);
                self.sent.insert(c);
                let mut bodies: Vec<(u32, usize, Vec3)> = Vec::new();
                let mut seen = HashSet::new();
                for &(_, _, body) in &tris {
                    if seen.insert(body) {
                        if let Some((t, _)) = self.havok.transform(body) {
                            bodies.push((body, self.havok.shape_of(body), t));
                        }
                    }
                }
                self.cell_bodies.insert(c, bodies);
            }
        }
        self.drop_stale(link);
        // twice a second: nearby cells whose bodies moved, changed or left the world go again
        if self.recheck.elapsed().as_secs_f32() > 0.5 {
            self.recheck = std::time::Instant::now();
            // decoded shapes are cached by address; Elden Ring frees and re-allocates them as the
            // world streams, so old entries pile up: past ~12 M triangles the cache starts over
            let (shapes, cached) = self.havok.cache_stats();
            if cached > 12_000_000 {
                crate::log(format!("collision: shape cache at {shapes} shapes / {cached} triangles: cleared"));
                self.havok.clear_cache();
            }
            let mut changed = Vec::new();
            let now = std::time::Instant::now();
            for (&c, bodies) in self.cell_bodies.iter_mut() {
                let centre = Vec3::new((c.0 as f32 + 0.5) * CELL, (c.1 as f32 + 0.5) * CELL, (c.2 as f32 + 0.5) * CELL);
                if (centre - at).length() > 60.0 {
                    continue;
                }
                // a cell that keeps changing (something always moving) is left alone a while
                let h = self.cell_history.entry(c).or_default();
                h.retain(|t| now.duration_since(*t).as_secs_f32() < 20.0);
                if h.last().is_some_and(|t| now.duration_since(*t).as_secs_f32() < 3.0) || h.len() >= 3 {
                    continue;
                }
                let mut gone = false;
                let mut moved = 0usize;
                for (b, shape, p) in bodies.iter_mut() {
                    if !self.havok.in_world(*b) || self.havok.shape_of(*b) != *shape {
                        gone = true;
                        continue;
                    }
                    let Some((t, _)) = self.havok.transform(*b) else {
                        moved += 1;
                        continue;
                    };
                    let d = t - *p;
                    if d.length() <= 0.3 {
                        continue;
                    }
                    // a move by whole 8 m steps is Havok shifting its space (bodies catch up over a
                    // few frames, so a cell could look half moved): followed, not a gate
                    let g = (d / 8.0).round() * 8.0;
                    if (d - g).abs().max_element() < 0.05 {
                        *p = t;
                        continue;
                    }
                    moved += 1;
                }
                // most of the cell moved together: a shift of Havok's space, not a gate
                if moved * 2 > bodies.len() && !gone {
                    for b in bodies.iter_mut() {
                        if let Some((t, _)) = self.havok.transform(b.0) {
                            b.2 = t;
                        }
                    }
                    continue;
                }
                if gone || moved > 0 {
                    h.push(now);
                    changed.push(c);
                }
            }
            for c in changed {
                crate::er::dlog(format!("collision: cell {c:?} changed (a gate, door or lift moved): sent again"));
                self.sent.remove(&c);
            }
        }
        self.empty.retain(|_, t| t.elapsed().as_secs_f32() < 30.0);
        // far cells go
        let far: Vec<_> = self
            .sent
            .iter()
            .copied()
            .filter(|c| {
                let centre = Vec3::new((c.0 as f32 + 0.5) * CELL, at.y, (c.2 as f32 + 0.5) * CELL);
                Vec3::new(centre.x - at.x, 0.0, centre.z - at.z).length() > DROP
            })
            .collect();
        for c in far {
            if link.write_collision(COL_REMOVE, &col::remove(region_id(c), self.epoch)) {
                self.sent.remove(&c);
                self.cell_bodies.remove(&c);
            }
        }
        wanted.len() <= PER_FRAME
    }

    /// Elden Ring re-based its Havok space by `delta` (a floating origin; in Stormveil every ~20 s).
    /// The origin moves with Havok, so the guest's space, the cell grid and every triangle already
    /// sent stay valid: nothing is sent again. (Re-sending everything, ~3 million triangles each
    /// time, made ULTRAKILL rebuild all its colliders: stutter, and holes until they were back.)
    pub fn rebase(&mut self, delta: Vec3, link: &Link) {
        self.origin += delta;
        for bodies in self.cell_bodies.values_mut() {
            for b in bodies.iter_mut() {
                b.2 += delta;
            }
        }
        self.havok.clear_cache();
        let _ = link;
    }

    /// A Havok shift since last frame, seen on a static body: every static body moves by the same
    /// whole multiple of 8 m when Elden Ring shifts its physics space (a floating origin). The
    /// player's own position can't tell: a falling V1 pulled away from the Tarnished by multiples
    /// of 8 m too, and following those broke the collision.
    pub fn detect_shift(&mut self) -> Option<Vec3> {
        if !self.havok.refresh_bodies() {
            return None;
        }
        // Several bodies, each a different shape: a body slot reused by another copy of the same
        // model (shapes are shared between instances) looked like a shift of its own, and those
        // false shifts dragged everything 32 m up at a time. A shift is only real when at least
        // two of them moved by the same whole 8 m grid vector and none disagrees.
        let mut moved: Vec<Vec3> = Vec::new();
        let mut still = 0;
        let mut keep = Vec::new();
        for &(index, shape, before) in &self.references {
            if self.havok.shape_of(index) != shape {
                continue;
            }
            let Some((now, _)) = self.havok.transform(index) else { continue };
            let d = now - before;
            let r = d.round();
            let whole = (d - r).abs().max_element() < 0.01;
            let grid = [r.x, r.y, r.z].iter().all(|v| (v / 8.0).fract().abs() < 1e-3);
            if r.length() < 0.01 && d.length() < 0.01 {
                still += 1;
            } else if r.length() > 4.0 && whole && grid {
                moved.push(r);
            } else {
                continue; // it moved some other way (not static after all): drop it
            }
            keep.push((index, shape, now));
        }
        self.references = keep;
        let first = *moved.first()?;
        let agree = moved.iter().all(|m| (*m - first).length() < 0.01);
        (moved.len() >= 2 && still == 0 && agree).then_some(first)
    }

    /// Removes regions from before a re-base once the new ones had time to arrive.
    fn drop_stale(&mut self, link: &Link) {
        let mut keep = Vec::new();
        let live: HashSet<u64> = self.sent.iter().copied().map(region_id).collect();
        for (id, t) in self.stale.drain(..) {
            if live.contains(&id) {
                continue; // (never take away a cell that is current)
            }
            if t.elapsed().as_secs_f32() < 3.0 || !link.write_collision(COL_REMOVE, &col::remove(id, self.epoch)) {
                keep.push((id, t));
            }
        }
        self.stale = keep;
    }

    pub fn bodies_around(&mut self, p: Vec3, range: f32) -> Vec<String> {
        self.havok.refresh_bodies();
        self.havok.bodies_around(p, range)
    }

    pub fn cache_stats(&self) -> (usize, usize) {
        self.havok.cache_stats()
    }

    pub fn cells(&self) -> usize {
        self.sent.len()
    }
}

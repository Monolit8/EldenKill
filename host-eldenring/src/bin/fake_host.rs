//! A stand-in for Elden Ring: creates the link like the real host, sends known collision (a big
//! floor, walls, a ramp, pillars), three dummy enemies and a teleport, then reports what ULTRAKILL
//! does with them. Built to test the guest without starting Elden Ring (and before the ER half
//! existed), the way the Minecraft-in-GTA passthrough was built against a fake host.
//!
//!   fake-host [seconds] [--walk] [--dump overlay.bmp]
//!
//! --walk holds W for a second once V1 is driving and checks that V1 moved (input injection).
//! --dump writes the first overlay frame after 3 s of frames to a BMP (overlay transport).

use std::time::{Duration, Instant};

use eldenkill::link::{ActorRecord, HostState, Link, col};
use eldenkill::proto::*;

const UNITS: f32 = 2.0;

fn quad(a: [f32; 3], b: [f32; 3], c: [f32; 3], d: [f32; 3], flags: u32) -> [([f32; 9], u32); 2] {
    let s = |p: [f32; 3]| p.map(|x| x * UNITS);
    let (a, b, c, d) = (s(a), s(b), s(c), s(d));
    [
        ([a[0], a[1], a[2], b[0], b[1], b[2], c[0], c[1], c[2]], flags),
        ([a[0], a[1], a[2], c[0], c[1], c[2], d[0], d[1], d[2]], flags),
    ]
}

/// An axis-aligned box (metres) as 12 triangles, wound like Havok's (the guest flips them).
fn cuboid(min: [f32; 3], max: [f32; 3]) -> Vec<([f32; 9], u32)> {
    let [x0, y0, z0] = min;
    let [x1, y1, z1] = max;
    let mut t = Vec::new();
    t.extend(quad([x0, y1, z0], [x0, y1, z1], [x1, y1, z1], [x1, y1, z0], TRI_WALKABLE)); // top
    t.extend(quad([x0, y0, z0], [x1, y0, z0], [x1, y0, z1], [x0, y0, z1], 0)); // bottom
    t.extend(quad([x0, y0, z0], [x0, y1, z0], [x1, y1, z0], [x1, y0, z0], 0)); // -z
    t.extend(quad([x0, y0, z1], [x1, y0, z1], [x1, y1, z1], [x0, y1, z1], 0)); // +z
    t.extend(quad([x0, y0, z0], [x0, y0, z1], [x0, y1, z1], [x0, y1, z0], 0)); // -x
    t.extend(quad([x1, y0, z0], [x1, y1, z0], [x1, y1, z1], [x1, y0, z1], 0)); // +x
    t
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let seconds: u64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(120);
    let walk = args.iter().any(|a| a == "--walk");
    let dump = args.iter().position(|a| a == "--dump").and_then(|i| args.get(i + 1)).cloned();

    let link = Link::create().expect("create link");
    println!("fake host: link up, waiting for ULTRAKILL ({seconds} s)");

    let epoch = 1u32;
    // the test level, in metres
    let mut regions: Vec<(u64, Vec<([f32; 9], u32)>)> = Vec::new();
    regions.push((1, quad([-100.0, 0.0, -100.0], [-100.0, 0.0, 100.0], [100.0, 0.0, 100.0], [100.0, 0.0, -100.0], TRI_WALKABLE).to_vec()));
    regions.push((2, cuboid([-20.0, 0.0, 15.0], [20.0, 8.0, 16.0]))); // a wall ahead
    regions.push((3, cuboid([10.0, 0.0, -10.0], [14.0, 3.0, -6.0]))); // a block to climb
    regions.push((4, quad([-12.0, 0.0, -2.0], [-12.0, 0.0, 6.0], [-4.0, 4.0, 6.0], [-4.0, 4.0, -2.0], TRI_WALKABLE).to_vec())); // ramp
    for (i, x) in [-30.0f32, -24.0, -18.0].iter().enumerate() {
        regions.push((10 + i as u64, cuboid([*x, 0.0, -30.0], [x + 1.5, 10.0, -28.5]))); // pillars
    }

    let mut state = HostState {
        flags: HOST_IN_WORLD | HOST_ENABLED,
        epoch,
        origin: [0.0; 3],
        pos: [0.0, 0.0, 0.0],
        teleport_seq: 1,
        viewport: [1280, 720],
        units_per_metre: UNITS,
        hp: 1000,
        hp_max: 1000,
        yaw: 0.0,
        game_hour: 12.0,
        ..Default::default()
    };
    let actors: Vec<ActorRecord> = (0..3)
        .map(|i| ActorRecord {
            id: 0x1000 + i,
            flags: ACTOR_HOSTILE,
            pos: [(-6.0 + 6.0 * i as f32) * UNITS, 0.0, 10.0 * UNITS],
            yaw: 180.0,
            radius: 0.5 * UNITS,
            height: 1.8 * UNITS,
            hp_frac: 1.0,
            team: 6,
            name: *b"Godrick Soldier\0\0\0\0\0\0\0\0\0",
        })
        .collect();

    let start = Instant::now();
    let mut sent_collision = false;
    let mut was_alive = false;
    let mut last_print = Instant::now();
    let mut driving_since: Option<Instant> = None;
    let mut walk_stage = 0;
    let mut walk_from = [0.0f32; 3];
    let mut first_frame_at: Option<Instant> = None;
    let mut dumped = false;
    let mut hits = 0u32;
    let mut last_guest = None;

    while start.elapsed() < Duration::from_secs(seconds) {
        link.heartbeat();
        let alive = link.guest_alive();
        if alive && !was_alive {
            println!("fake host: ULTRAKILL linked (pid {})", link.guest_pid());
            link.restart_overlay();
            sent_collision = false;
        }
        was_alive = alive;
        if alive && !sent_collision {
            let mut ok = link.write_collision(COL_CLEAR, &col::clear(epoch));
            for (id, tris) in &regions {
                ok &= link.write_collision(COL_REGION, &col::region(*id, epoch, tris));
            }
            sent_collision = ok;
            println!("fake host: sent {} bodies ({} triangles)", regions.len(), regions.iter().map(|r| r.1.len()).sum::<usize>());
        }
        link.write_host_state(&state);
        link.write_actors(&actors);
        while let Some(e) = link.pop_event() {
            match e.kind {
                EV_HIT_ACTOR => {
                    hits += 1;
                    println!("fake host: V1 hit actor {:#x} for {:.2} (flags {:#x}) at ({:.1}, {:.1}, {:.1})", e.actor, e.a, e.flags, e.b, e.c, e.d);
                }
                EV_DIED => println!("fake host: V1 died"),
                k => println!("fake host: event {k}"),
            }
        }
        let g = link.read_guest_state();
        if let Some(g) = g {
            last_guest = Some(g);
            let driving = g.flags & GUEST_DRIVING != 0;
            if driving && driving_since.is_none() {
                driving_since = Some(Instant::now());
                println!("fake host: V1 is driving at ({:.2}, {:.2}, {:.2}) (teleport ack {})", g.pos[0], g.pos[1], g.pos[2], g.teleport_ack);
            }
            if walk && driving {
                let t = driving_since.unwrap().elapsed().as_secs_f32();
                if walk_stage == 0 && t > 3.0 {
                    walk_from = g.pos;
                    link.push_input(IN_KEY, 0x57, 1, 0, 0); // W down
                    walk_stage = 1;
                } else if walk_stage == 1 && t > 4.0 {
                    link.push_input(IN_KEY, 0x57, 0, 0, 0);
                    walk_stage = 2;
                } else if walk_stage == 2 && t > 5.0 {
                    let d = ((g.pos[0] - walk_from[0]).powi(2) + (g.pos[2] - walk_from[2]).powi(2)).sqrt();
                    println!("fake host: WALK TEST: V1 moved {d:.2} units in 1 s of W ({})", if d > 3.0 { "PASS" } else { "FAIL" });
                    // and a mouse turn
                    link.push_input(IN_MOUSE_MOVE, 0, 200, 0, 0);
                    walk_stage = 3;
                } else if walk_stage == 3 && t > 5.5 {
                    println!("fake host: after a 200-count mouse move, V1 looks along ({:.2}, {:.2}, {:.2})", g.fwd[0], g.fwd[1], g.fwd[2]);
                    link.push_input(IN_MOUSE_MOVE, 0, -200, 0, 0); // and back, facing the dummies
                    link.push_input(IN_KEY, 0x31, 1, 0, 0); // 1: the revolver (the Sandbox hands V1 the spawner arm)
                    walk_stage = 4;
                } else if walk_stage == 4 && t > 7.5 {
                    link.push_input(IN_KEY, 0x31, 0, 0, 0);
                    link.push_input(IN_MOUSE_BUTTON, 0, 1, 0, 0); // fire
                    walk_stage = 5;
                } else if walk_stage == 5 && t > 7.7 {
                    link.push_input(IN_MOUSE_BUTTON, 0, 0, 0, 0);
                    walk_stage = 6;
                } else if walk_stage == 6 && t > 9.0 {
                    println!("fake host: SHOOT TEST: {hits} hit events after one shot at the dummies ({})", if hits > 0 { "PASS" } else { "FAIL" });
                    walk_stage = 7;
                }
            }
        }
        let shot_frame = walk_stage == 5;
        if let Some(frame) = link.take_overlay() {
            if shot_frame {
                if let Some(path) = &dump {
                    write_bmp(&path.replace(".bmp", "-shot.bmp"), frame.width, frame.height, frame.flags & OV_BOTTOM_UP != 0, frame.pixels);
                }
            }
            // V1's arms and guns are only up once V1 drives
            if driving_since.is_some() {
                first_frame_at.get_or_insert_with(Instant::now);
            }
            if let (Some(path), false, Some(t0)) = (&dump, dumped, first_frame_at) {
                if t0.elapsed().as_secs_f32() > 3.0 {
                    write_bmp(path, frame.width, frame.height, frame.flags & OV_BOTTOM_UP != 0, frame.pixels);
                    let keyed = frame.pixels.chunks_exact(4).filter(|p| p[0] > 240 && p[1] < 16 && p[2] > 240).count();
                    println!(
                        "fake host: dumped overlay frame {} ({}x{}, flags {:#x}, {:.0}% key colour) to {path}",
                        frame.frame_id,
                        frame.width,
                        frame.height,
                        frame.flags,
                        100.0 * keyed as f32 / (frame.width * frame.height) as f32
                    );
                    dumped = true;
                }
            }
        }
        if last_print.elapsed() > Duration::from_secs(2) {
            last_print = Instant::now();
            match last_guest {
                Some(g) => println!(
                    "fake host: guest flags {:#04x} pos ({:.1}, {:.1}, {:.1}) eye ({:.1}, {:.1}, {:.1}) fwd ({:.2}, {:.2}, {:.2}) fov {:.0} hp {} | overlay frames {} | backlog {} | hits {hits}",
                    g.flags, g.pos[0], g.pos[1], g.pos[2], g.eye[0], g.eye[1], g.eye[2], g.fwd[0], g.fwd[1], g.fwd[2], g.fov, g.hp,
                    link.overlay_frames(),
                    link.collision_backlog()
                ),
                None => println!("fake host: no guest state yet (guest alive: {alive})"),
            }
        }
        // keep the teleport target where V1 is, as the real host does once V1 drives
        if let Some(g) = last_guest {
            if g.teleport_ack == state.teleport_seq && g.flags & GUEST_DRIVING != 0 {
                state.pos = g.pos;
            }
        }
        std::thread::sleep(Duration::from_millis(16));
    }
    println!("fake host: done");
}

fn write_bmp(path: &str, w: usize, h: usize, bottom_up: bool, rgba: &[u8]) {
    let row = w * 4;
    let mut out = Vec::with_capacity(54 + row * h);
    out.extend_from_slice(b"BM");
    out.extend_from_slice(&((54 + row * h) as u32).to_le_bytes());
    out.extend_from_slice(&[0; 4]);
    out.extend_from_slice(&54u32.to_le_bytes());
    out.extend_from_slice(&40u32.to_le_bytes());
    out.extend_from_slice(&(w as i32).to_le_bytes());
    // BMP rows are bottom-up when the height is positive
    out.extend_from_slice(&(if bottom_up { h as i32 } else { -(h as i32) }).to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&32u16.to_le_bytes());
    out.extend_from_slice(&[0; 24]);
    for p in rgba.chunks_exact(4) {
        out.extend_from_slice(&[p[2], p[1], p[0], p[3]]);
    }
    let _ = std::fs::write(path, out);
}

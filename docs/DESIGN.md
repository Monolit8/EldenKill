# EldenKill design

**V1 from ULTRAKILL in the Lands Between.** A passthrough mod in the style of
[Killcraft](https://github.com/goonsn/Killcraft) (Minecraft inside ULTRAKILL), which is built on
SkyCraft (Minecraft inside Skyrim). Two real games run at once and talk over shared memory.

> Killcraft has no `docs/DESIGN.md` (its README and source are the design). This file is written
> from those, plus [er-mario](https://github.com/deltarooo/er-mario) for the Elden Ring side.

## Roles

| | Killcraft | EldenKill |
|---|---|---|
| **Host**: the world you see, its enemies, renderer | ULTRAKILL (BepInEx plugin) | **Elden Ring** (Rust DLL, loaded by me3) |
| **Guest**: simulates the player, runs hidden | Minecraft (SkyCraft's Fabric mod) | **ULTRAKILL** (BepInEx plugin) |
| Who creates the shared memory | host | host |
| Who is authoritative for movement | guest (Minecraft) | guest (ULTRAKILL) |

Why this way round: the guest must run headless-ish and accept foreign collision. ULTRAKILL is a
Unity game: easy to strip its level, build Elden Ring's collision as MeshColliders and run V1's
real movement code on them. Elden Ring can't be made into a hidden player simulation.

## The loop (every frame)

```
 Elden Ring (host)                                     ULTRAKILL (guest, hidden window)
 ─────────────────                                     ────────────────────────────────
 Havok bodies near V1 ── collision ring (32 MB) ─────▶ MeshColliders on layer 8 (Environment)
 player pos, menus, loads ── HostState (seqlock) ────▶ teleport / freeze / respawn V1
 DirectInput keys+mouse ── input ring ───────────────▶ Input System devices (QueueStateEvent)
 enemies near V1 ── actor table (seqlock) ───────────▶ proxy hitboxes (EnemyIdentifier, layer 10)
 Tarnished HP loss ── input ring (HURT) ─────────────▶ NewMovement.GetHurt
 Tarnished + camera ◀── GuestState (seqlock) ─────────  V1 feet, eye, look, fov, hp, flags
 enemy HP ◀── event ring (HIT_ACTOR, DIED) ───────────  DeliverDamage on a proxy (Harmony)
 hudhook DX12 overlay ◀── overlay triple buffer ──────  V1's arms/guns/HUD, magenta background
```

The byte layout is [`protocol/eldenkill_protocol.h`](../protocol/eldenkill_protocol.h), mirrored in
`host-eldenring/src/proto.rs` and `guest-ultrakill/src/Proto.cs`.

### Coordinates
- Elden Ring's Havok space and Unity are both left-handed, Y up (er-mario mirrors X for SM64, which is
  right-handed). So `unity = (havok - origin) * units_per_metre`, with no axis swaps.
- `units_per_metre = 2`: V1's 3.5-unit capsule is about 1.75 m, the Tarnished's height (Killcraft
  uses 2 ULTRAKILL units per Minecraft block for the same reason).
- `origin` is snapped to 64 m near the player and moves when the player gets 1.5 km from it (floats
  stay precise). A new origin is a new **epoch**: the guest drops all collision and V1 is teleported.
- **To verify in game:** that strafing and turning aren't mirrored (if they are, X needs a flip on
  both sides) and that the camera's fov unit matches (ER's `fov` is taken as vertical radians).

### Collision
- Host: `er/stream.rs`. World cells of 32 m. Cells within 56 m of V1 (2 below, 1 above) are
  queried from the live Havok world (`er/havok_col.rs`, er-mario's decoder for compressed meshes,
  convex hulls, compounds) and sent as one REGION each, nearest first, 2 per frame. Cells past 120 m
  are removed.
- Guest: `World.cs` builds one MeshCollider per region on layer 8. Havok's winding isn't confirmed,
  so each body is oriented by its own floors (most near-horizontal faces must face up).
- Static world only for now; moving bodies (lifts, doors, the Grand Lift) are a later milestone (er-mario's
  `moving.rs` shows how).

### Hand-off (who has the player)
Like Killcraft's `Host.cs`, with the roles swapped:
- **V1 drives** when ULTRAKILL is linked, V1 has acked the current teleport and epoch, the player is
  in the world for 1.5 s, not dead, not in a game-driven animation (6xxxx events: doors, fog walls,
  graces; ladders). Then the Tarnished is put at V1's feet every frame (gravity off, "standing"
  flags on so interactions work), hidden, and Elden Ring's camera is V1's (er-mario's Lakitu
  technique: rewritten in every task group from camera update to draw).
- **Elden Ring has the player** otherwise. V1 is frozen (kinematic) in ULTRAKILL. If Elden Ring moves
  the player more than 4 m from where it was put (grace warp, respawn, fall respawn, a cutscene that
  relocates), `teleport_seq` is bumped and V1 is placed there once that floor has arrived.
- Menus: V1 freezes, input goes to Elden Ring.

### Input
Elden Ring reads DirectInput. `er/input.rs` hooks `GetDeviceState` / `GetDeviceData` (er-mario's
keyboard hook, extended to the mouse): every key and mouse delta goes to the input ring, and Elden
Ring gets a released keyboard and a still mouse, except `er_keys` (Esc, E). On the guest,
`InputInject.cs` queues the state on Unity's `Keyboard.current` / `Mouse.current` in
`InputSystem.onBeforeUpdate`. ULTRAKILL never has focus, so `backgroundBehavior = IgnoreFocus`, and
devices disabled by losing focus are re-enabled.

### Rendering
The guest switches its arena's level renderers off and clears the camera to magenta; at the end of
each frame the screen is captured (`ScreenCapture.CaptureScreenshotIntoRenderTexture` +
`AsyncGPUReadback`) into the overlay triple buffer. The host keys the magenta out and draws the
image full-screen with hudhook (DX12) behind ImGui's windows. So V1's guns, arms, projectiles,
explosions and ULTRAKILL's HUD sit on top of Elden Ring's world, from the same camera.

Limits (same as SkyCraft's overlay): no depth test against Elden Ring's world (a rocket behind a wall
is still drawn), no Elden Ring lighting on V1's things. The next step is compositing with Elden
Ring's depth buffer (the GTA V passthrough's approach).

### Combat
- Enemies: `er/combat.rs` lists living characters within 70 m (er-mario's `hittable` types) into the
  actor table. The guest makes a capsule (tag `Body`) and head sphere (tag `Head`) on layer 10 with an
  `EnemyIdentifier` whose Unity messages are skipped (Harmony). `DeliverDamage` on it is turned into a
  HIT_ACTOR event; the host takes `damage × damage_to_elden_ring` HP (bosses × 0.5).
  Every ULTRAKILL weapon goes through `DeliverDamage`: beams, pellets, punches, explosions.
- V1 hurt: Elden Ring's hits land on the (invisible) Tarnished. His HP loss each frame becomes
  ULTRAKILL damage (`damage_to_v1` per 1% of max HP) and is put back; `player_no_dead` keeps him
  alive while V1 lives.
- Deaths: V1 dies → the Tarnished's HP goes to 0 (Elden Ring's own death, runes, respawn). The
  Tarnished dies (falls, scripted deaths) → V1 dies. Elden Ring's respawn is a teleport, on which
  V1 respawns (`NewMovement.Respawn`). ULTRAKILL's own restart is blocked while linked.
- While linked, V1 has every weapon (a postfix on `GameProgressSaver.CheckGear`; the save isn't
  written).

## Milestones

| # | Milestone | State |
|---|---|---|
| 0 | Project, protocol, both halves build | done |
| 1 | Link: heartbeats, states, rings, overlay buffer (fake host ⇄ ULTRAKILL) | **done, tested** |
| 2 | Guest: arena stripped, V1 on streamed collision, input injection, overlay frames | **done, tested with the fake host** (walk, look, shoot PASS) |
| 3 | Guest: proxies, hits to the host | **done, tested** (revolver hit → HIT_ACTOR) |
| 4 | Host: Elden Ring DLL (collision cells, takeover, camera, input hooks, overlay, combat) | written, builds; **not yet run in game** (see below) |
| 5 | In-game tuning: axis/fov check, damage balance, death/respawn, graces, ladders | next |
| 6 | Depth compositing (ER depth buffer), moving platforms, Torrent off, V1 model for others | later |
| 7 | Release: README, me3 package, video (`publish-mod`, `showcase-video` skills) | later |

### Not verified yet (needs Elden Ring running)
The first in-game launch stopped before the game started: me3 calls `SteamAPI_Init`, which fails
because the Steam account logged in doesn't own this Elden Ring install (its `LastOwner` is another
account). Once Elden Ring starts, check in this order (`debug = 1` logs to `eldenkill.log`):
1. `link: shared memory ... created`, `frame task registered`, `launcher: started ULTRAKILL`, `ULTRAKILL linked`.
2. `collision: new origin`, then V1 placed (BepInEx log) and `V1 has the player`.
3. Axes: W walks where the camera looks, A/D aren't swapped, mouse turns the right way (else X needs flipping).
4. Mouse look: `input: hooked the game's GetCursorPos` � DirectInput or the cursor hook must deliver it.
5. Overlay: V1's arms on top, magenta gone, fov lines up (projectiles land where they appear).
6. Shooting an enemy: `combat: V1 hit ...`; getting hit: `the Tarnished was hit: V1 takes N`.

## Testing without Elden Ring
`tools/test-guest.ps1` runs the **fake host** (`host-eldenring/src/bin/fake_host.rs`: a floor, walls,
a ramp, pillars, three dummy enemies) against the real ULTRAKILL and prints PASS/FAIL for walking
(W through the input ring), turning (mouse), and shooting (a revolver hit coming back as an event),
plus an overlay frame as `test-output/overlay.png`.

## Where things are
```
EldenKill/
  protocol/eldenkill_protocol.h   the link (source of truth)
  host-eldenring/                 Rust: eldenkill.dll (Elden Ring) + fake-host.exe
    src/link.rs, proto.rs         host end of the link
    src/er/                       Elden Ring: mod.rs (frame), stream.rs, havok_col.rs, camera.rs,
                                  input.rs, overlay.rs, combat.rs, launcher.rs, version.rs
  guest-ultrakill/                C#: EldenKill.Guest.dll (BepInEx plugin)
    src/Guest.cs                  the guest loop (hand-off, teleports, deaths)
    src/Link.cs, Proto.cs         guest end of the link
    src/World.cs                  collision regions -> MeshColliders, arena stripping
    src/InputInject.cs            input ring -> Unity Input System
    src/OverlayOut.cs             V1's view -> overlay buffer
    src/Proxies.cs                enemy hitboxes, Harmony patches
  dist/EldenKill/                 what a player installs (dll, ini, me3 profile, launcher .bat)
  tools/test-guest.ps1            fake-host test
  reference/                      Killcraft, er-mario, gamedb clones + decompiled ULTRAKILL (never publish)
```

## Credits
- Killcraft (goonsn) and SkyCraft (chasmlol): the architecture and protocol shape.
- er-mario (deltarooo, MIT): Havok collision decoding, DirectInput hook, camera override, HUD and
  menu detection, task-group timings; `er/havok_col.rs`, `er/explore.rs`, `er/version.rs` are vendored.
- fromsoftware-rs (vswarte et al.), me3, hudhook, BepInEx, HarmonyX.
- gamedb (smileybaal): indexing the decompiled ULTRAKILL to find its APIs.

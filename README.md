# EldenKill

Play Elden Ring as V1 from ULTRAKILL. ULTRAKILL's real movement and weapons: dashes, slides, wall
jumps, slams, the whiplash, coins, rockets and parries, against Elden Ring's enemies and bosses in
the Lands Between. You also get ULTRAKILL's style meter.

**Work in progress.**

You need your own copies of both games. A real ULTRAKILL runs hidden in the background and simulates
V1, so everything V1 does is ULTRAKILL's own code. This repository contains no files from either game.

## Getting started

You need **Elden Ring** on Steam (exe 2.7.1.0) and **ULTRAKILL** on Steam.

1. Install me3, the mod loader: [me3.help](https://me3.help/)
2. Install [BepInEx 5](https://github.com/BepInEx/BepInEx/releases) into ULTRAKILL:
   `BepInEx_win_x64_5.4.x.zip` unzipped into the ULTRAKILL folder, then start ULTRAKILL once and close it.
3. Download `EldenKill-x.y.z.zip` from the [latest release](../../releases/latest) (under Assets) and
   unzip it somewhere you can write to, like your Documents folder.
4. Copy `ULTRAKILL-plugin\EldenKill.Guest.dll` from it into `ULTRAKILL\BepInEx\plugins\EldenKill\`
   (make the `EldenKill` folder).
   If ULTRAKILL isn't on your Desktop or in Steam's default folder, set `ultrakill = C:\path\to\ULTRAKILL.exe`
   in `eldenkill.ini`.
5. Start the game by double-clicking **launch-eldenkill.bat**. Don't use "Run as administrator":
   Elden Ring hangs at start when it runs elevated.

Elden Ring starts ULTRAKILL by itself, hidden and silent. Load a character: once Elden Ring's floor
under you has loaded, V1 takes over. The mod uses its own save file (`EldenKill.sl2`), so your
normal save is never touched. To bring a character over, copy your `ER0000.sl2` to `EldenKill.sl2`
in `%APPDATA%\EldenRing\<your id>\`.

To practise ULTRAKILL's movement on its own, **practice-ultrakill.bat** starts plain ULTRAKILL
straight into the Sandbox with every weapon. It never connects to Elden Ring.

## Stay offline

The launcher starts Elden Ring offline with Easy Anti-Cheat off. **Never play this mod online.**

## Controls

ULTRAKILL's own controls (WASD, Space, Shift dash, Ctrl slide, mouse to aim and shoot, 1-5
weapons, F punch, G change arm, R whiplash...), plus:

| Key | |
|---|---|
| **E** | Interact: doors, graces, items, NPCs, fog gates |
| **Esc** | Elden Ring's menu |
| **F1** | ULTRAKILL's menu: its Options (sound, HUD, mouse, FOV). F1 again to close |
| **F9** | V1 off (plain Elden Ring) and back on |
| F5 / F6 / F7 / F8 | Debug: kill yourself, save a snapshot, unstick V1, debug panel |

Keys that stay Elden Ring's: `er_keys` in `eldenkill.ini`.

## How it plays

**Leveling, stats and gear don't affect V1:**

- **Your damage:** every Elden Ring enemy counts as an ULTRAKILL enemy of its class, and each hit
  takes that share of its real max HP. A revolver shot (1 ULTRAKILL damage) does:
  - a third of a normal enemy;
  - a tenth of an elite (over 1000 HP);
  - an eightieth of a boss (over 3000 HP).

  Headshots, explosions and every gun keep their ULTRAKILL values.
- **V1's health:** always ULTRAKILL's 100, whatever your Vigor. A hit that would take a share of the
  Tarnished's health takes 1.5 times that share from V1.
- **Healing:** every kill heals V1, 20 for a normal enemy and 50 for a boss. It also sprays
  ULTRAKILL blood, and standing in it heals more, as in ULTRAKILL.
- **Style:** kills, headshots, multi-kills and boss kills fill ULTRAKILL's style meter.
- **Deaths go both ways:** when V1 dies the Tarnished dies, and Elden Ring's respawn at a grace
  brings V1 back.
- **Boss bars:** only for fights where Elden Ring itself shows one, drawn in Elden Ring's style
  with the boss's name.
- **Not used at all:** stamina, FP, equip load and the Tarnished's weapons.

**V1's weapons and mechanics:**

- coin ricochets find Elden Ring enemies and go for their heads;
- the whiplash pulls V1 to an enemy, as with ULTRAKILL's heavy enemies.

## Settings

**`eldenkill.ini` (Elden Ring side):**

| Setting | Default | |
|---|---|---|
| `uk_hp_normal`, `uk_hp_elite`, `uk_hp_boss` | 3, 10, 80 | how tough enemies are, in ULTRAKILL health |
| `damage_to_v1` | 1.5 | how hard Elden Ring hits V1 |
| `units_per_metre` | 1.5 | ULTRAKILL units per metre: lower is faster and bigger |
| `fps` | uncapped | Elden Ring's frame rate limit, or a number |
| `crosshair` | 0 | 0 none, 1 cross, 2 cross with corners, -1 ULTRAKILL's setting |
| `er_sfx_volume`, `er_voice_volume` | -1 | Elden Ring's effects and voice volume while V1 plays, 0-10 (-1 leaves yours) |
| `ultrawide` | 0 | removes the black bars on screens wider than 16:9 |
| `high_priority` | 1 | runs Elden Ring at high CPU priority, against stutter |

**`ULTRAKILL\BepInEx\config\dev.eldenkill.guest.cfg` (ULTRAKILL side):**

| Setting | |
|---|---|
| `Fps` | ULTRAKILL's frame rate while hidden |
| `OverlayScale` | V1's view resolution, 0.5 halves the cost |

**ULTRAKILL's own Options (F1 in game) carry over:**

- **Already applied:** FOV, mouse sensitivity, screen shake, camera tilt and every volume.
- **HUD options:** HUD type, style meter and background opacity.
- **Only affect V1's gun:** the graphics filters (pixelization, warping, dithering), because Elden
  Ring draws the world itself.

## Known issues

- **Falling through the map:** moving very fast into an area whose collision hasn't loaded can drop
  V1 through it. The mod catches it and puts V1 back.
- **Invisible walls:** some doorways block V1 where Elden Ring lets you through.
- **Getting stuck:** Elden Ring's props are hollow shells. A fast slide can wedge V1 into a tree or
  rock; the mod pulls it out after a moment.
- **Enemies don't react to V1's hits:** they don't flinch, and Elden Ring's own blood doesn't show.
  V1's ULTRAKILL blood does.
- **Bosses hit very hard:** big bosses can take most of V1's health in one hit.
- **Stutter on 16 GB of RAM:** two games run at once. Closing Discord and browsers helps.
- **ULTRAKILL's own HUD isn't shown:** the health bar, dashes, style meter and boss bars are drawn
  by the mod instead.
- **Ambience and voices:** Elden Ring has one effects volume for ambience, the Tarnished's voice
  and enemies, so they can't be muted separately.

## Reporting problems

Say what happened and when, and attach:

- `eldenkill.log` and `eldenkill.prev.log` from the `EldenKill` folder: the last session and the one
  before.
- `ULTRAKILL\BepInEx\LogOutput.log`.
- For a problem at one spot, like an invisible wall, a `debug-*.txt` snapshot: stand there and press
  **F6**. It lists the collision around you.

The logs contain your folder paths, which can include your Windows user name; blank them out if you
like.



- **Elden Ring side:** a Rust DLL loaded by [me3](https://me3.help/), offline only.
  - It streams Elden Ring's live Havok collision around V1 to ULTRAKILL, in 32 m patches. It
    follows Elden Ring's shifting coordinates and rebuilds after loading screens and respawns.
  - It moves Elden Ring's camera with V1's, including its tilt.
  - It turns Elden Ring's enemies into hitboxes, and V1's hits into Elden Ring damage.
- **ULTRAKILL side:** a BepInEx plugin.
  - It loads the Sandbox and switches its level off, so V1 only stands on Elden Ring's collision.
  - It feeds V1 Elden Ring's input.
  - It sends back V1's position, its camera and its view: arms, guns, projectiles and blood.
- **Overlay:** Elden Ring draws V1's view and the HUD on top of its world, through
  [hudhook](https://github.com/veeenu/hudhook).
- **The Tarnished:** he stays in the game underneath and follows V1, so doors, graces, fog gates,
  menus, deaths and saves keep working.
- **The link:** the two games talk through shared memory. The protocol is in
  [protocol/eldenkill_protocol.h](protocol/eldenkill_protocol.h).

## Building

Windows, with [Rust](https://rustup.rs/) (stable, MSVC), the .NET SDK 6 or newer, and ULTRAKILL with
BepInEx installed.

```
.\tools\build-all.ps1
```

This builds both halves and puts them in `dist\EldenKill`.

- **Elden Ring half:** `cd host-eldenring && cargo build --release`. Cargo fetches fromsoftware-rs
  itself, pinned to a commit in `Cargo.toml`. A patched hudhook is in `vendor/`.
- **ULTRAKILL half:** `cd guest-ultrakill && dotnet build -c Release`. Add
  `-p:GameDir="X:\path\to\ULTRAKILL"` if ULTRAKILL isn't on your Desktop. The build copies the DLL
  into BepInEx.
- **Testing the ULTRAKILL half without Elden Ring:** `tools\test-guest.ps1`.

The `reference` folder (decompiled ULTRAKILL, cloned projects) is for development only and must
never be published.

## License

No license chosen yet. The files taken from er-mario keep its [MIT license](https://github.com/deltarooo/er-mario/blob/master/LICENSE):
`host-eldenring/src/er/havok_col.rs`, `explore.rs`, `version.rs`.

## Credits

- [Killcraft](https://github.com/goonsn/Killcraft) by goonsn and SkyCraft by chasmlol: the idea and
  the architecture.
- [er-mario](https://github.com/deltarooo/er-mario) by deltarooo (MIT):
  - Elden Ring collision reading;
  - camera and input techniques;
  - Elden Ring's text table layout.
- [fromsoftware-rs](https://github.com/vswarte/fromsoftware-rs) (MIT / Apache-2.0) by Vincent Swarte
  and contributors.
- [me3](https://me3.help/) by the me3 team.
- [hudhook](https://github.com/veeenu/hudhook) (MIT) by veeenu, for the overlay.
- [er-patcher](https://github.com/gurrgur/er-patcher) (MIT) by gurrgur: the frame rate and ultrawide
  patterns.
- [BepInEx](https://github.com/BepInEx/BepInEx) and HarmonyX, for the ULTRAKILL plugin.
- [gamedb](https://github.com/smileybaal/gamedb), for searching ULTRAKILL's code.

ULTRAKILL is by Arsi "Hakita" Patala / New Blood Interactive. Elden Ring is FromSoftware's.
EldenKill is a free fan mod, not affiliated with either, and contains none of their files.

# EldenKill debug plan: one mechanic at a time

Each test checks **one** mechanic. Do them in order: later ones depend on earlier ones. Write down
PASS / FAIL and what you saw; on a FAIL press **F6** (snapshot) and keep `eldenkill.log` and
`ULTRAKILL\BepInEx\LogOutput.log`.

## Tools
| Key | Does |
|---|---|
| **F8** | debug panel (V1, who has the player, Tarnished, collision, events, last inputs, switches) |
| **F7** | unstick (V1 to the last ground it stood on) |
| **F6** | snapshot to `debug-<n>.txt` next to the mod |
| **F5** | kill (the Tarnished dies, V1 with him) |
| **F9** | V1 on / off |
| **Num1** | draw Elden Ring's collision (grey) in V1's view |
| **Num2** | draw enemy hitboxes (red capsules) |
| **Num3** | show the hidden Tarnished |
| **Num4** | Elden Ring's own camera (V1's view off) |
| **Num5** | god mode (V1 takes no damage) |

Panel lines to know: `feet (x, y, z)`, `speed`, flags (`ground dash slide jump driving`),
`Tarnished ... m from V1`, `ULTRAKILL overlay N fps`, events in red (`LAUNCH`, falls).

---

## 1. Link and start-up
**Do:** start with `launch-eldenkill.bat`, wait at the title screen.
**Pass:** `eldenkill.log` has `ULTRAKILL linked`; nothing else needed.
**Fail looks like:** no link line; ULTRAKILL never starts (check `launcher:` lines).

## 2. Hand-off (V1 takes over)
**Do:** Continue, stand still for 10 s. F8.
**Pass:** panel says `V1 drives`; `teleport a/a`, `epoch b/b` equal; flags show `ground`.
**Fail:** stuck on "Elden Ring has him"; teleport or epoch numbers differ.

## 3. Camera (alone)
**Do:** stand still, don't touch the mouse. Then Num4 on and off.
**Pass:** view steady at eye height, no wobble; Num4 shows Elden Ring's camera and back.
**Fail:** shaking while standing still (camera), view underground (coordinates).

## 4. Mouse look (alone)
**Do:** stand still, move the mouse slowly left, right, up, down.
**Pass:** turns the right way, smoothly, at a sensible speed; `mouse last 0.5 s` changes.
**Fail:** inverted, jerky, too fast or slow (note which), or no movement in the panel.

## 5. Walking on flat ground
**Do:** on flat ground, W for 3 s, then S, A, D.
**Pass:** goes where the camera faces; A/D not swapped; `ground` stays on; speed ~16.
**Fail:** wrong direction (axis mirror), `ground` flickers, speed 0 (input).

## 6. Collision lines up with the world
**Do:** Num1 on. Look at the ground, a rock, a wall, a slope.
**Pass:** grey surfaces sit exactly on Elden Ring's ground and walls.
**Fail:** grey offset from the world (note how far and which way), missing patches (holes).

## 7. Walking far (coordinate shifts)
**Do:** walk ~300 m in one direction over normal ground.
**Pass:** `Havok shifted by ... followed` events appear; `Tarnished ... m from V1` stays under 1 m;
no launch, no fall.
**Fail:** Tarnished far from V1, `flip-flopping`, launches right after a shift.

## 8. Slopes, stairs, walls
**Do:** walk up/down a hill, stairs, into a wall, along a cliff edge.
**Pass:** V1 climbs, stops at walls, can step off edges and land.
**Fail:** passes through walls/floors (where?), launched at a wall.

## 9. Movement tech, one at a time
Test each separately, 5 times, on flat ground first:
- **Jump** (Space)
- **Dash** (Shift)
- **Slide** (Ctrl)
- **Slam** (Ctrl in the air)
- **Wall jump** (Space against a wall in the air)
- **Grapple** (R) at the ground / a wall

**Pass:** behaves like ULTRAKILL; no red `LAUNCH` in the panel unless it's real ULTRAKILL tech.
**Fail:** note which move, and copy the LAUNCH line (it lists keys held and weapon).

## 10. The overlay (V1's view)
**Do:** look at your gun, switch 1-5, fire into the sky.
**Pass:** gun and HUD drawn, no pink edges, `overlay` ~60+ fps, projectiles fly where you aim.
**Fail:** pink fringe, gun lagging behind the view, projectiles off to one side (fov).

## 11. Weapons
**Do:** each slot 1-5: fire, alt fire (RMB); F punch, G arm swap, Q last weapon.
**Pass:** each works like ULTRAKILL.
**Fail:** which button does nothing, or does something else.

## 12. Hitting enemies
**Do:** Num2 on (red capsules). Shoot one enemy with the revolver.
**Pass:** capsules sit on the enemies; `hits on enemies` goes up; the enemy loses HP and dies.
**Fail:** capsules offset/missing; hits but no HP loss; HP loss but no death.

## 13. Taking damage
**Do:** let an enemy hit you (Num5 off).
**Pass:** V1's health goes down in ULTRAKILL's HUD; the Tarnished doesn't die.
Then Num5 on: hits do nothing.
**Fail:** no damage, or the Tarnished dies instead.

## 14. Death and respawn
**Do:** F5.
**Pass:** YOU DIED, respawn at the grace, V1 takes over there, can move.
**Fail:** stuck dead, V1 left at the death spot, falls after respawn.

## 15. Interacting (E)
**Do:** open a door, pick up an item, rest at a grace, talk to an NPC.
**Pass:** each works; during the animation Elden Ring has the player; V1 continues after.
**Fail:** E does nothing; V1 ends up somewhere else afterwards.

## 16. Menus
**Do:** Esc menu, inventory, map.
**Pass:** V1 frozen while open, Elden Ring gets the keys; back to V1 after closing.
**Fail:** V1 moves/shoots in menus; stuck after closing.

## 17. Ladders, lifts, fast travel
**Do:** climb a ladder, ride a lift, fast travel from a grace.
**Pass:** each completes; V1 ends up where the Tarnished is.
**Fail:** note which (lifts are known not to be done: moving platforms aren't streamed yet).

---

### Known not done yet
Moving platforms (lifts) as collision, depth (V1's projectiles drawn over walls), variant
switching (E is Elden Ring's interact), mouse wheel with the polling input fallback.
